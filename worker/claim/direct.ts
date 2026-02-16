import { Address, xdr } from "@stellar/stellar-sdk";
import {
  ChannelsClient,
  PluginExecutionError,
  PluginTransportError,
} from "@openzeppelin/relayer-plugin-channels/dist/client";
import {
  Client as ScoreContractClient,
} from "../../shared/stellar/bindings/asteroids-score/dist/index.js";
import {
  DEFAULT_BINDINGS_RPC_URL,
  DEFAULT_RELAYER_REQUEST_TIMEOUT_MS,
  OPENZEPPELIN_CHANNELS_HOSTNAME,
  TESTNET_NETWORK_PASSPHRASE,
} from "../constants";
import type { WorkerEnv } from "../env";
import { parseInteger, safeErrorMessage } from "../utils";
import type { RelayClaimRequest, RelaySubmitResult } from "./types";

interface ChannelsConfig {
  baseUrl: string;
  apiKey: string;
  pluginId: string | null;
  timeoutMs: number;
}

interface DirectClaimConfig {
  scoreContractId: string;
  channels: ChannelsConfig;
}

function nonEmpty(value: string | undefined): string | null {
  if (!value) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function resolveChannelsConfig(env: WorkerEnv): ChannelsConfig | null {
  const relayUrlRaw = nonEmpty(env.RELAYER_URL);
  const apiKey = nonEmpty(env.RELAYER_API_KEY);
  if (!relayUrlRaw || !apiKey) {
    return null;
  }

  let relayUrl: URL;
  try {
    relayUrl = new URL(relayUrlRaw);
  } catch {
    return null;
  }

  const pluginId = nonEmpty(env.RELAYER_PLUGIN_ID);
  const isManagedChannels = relayUrl.hostname
    .toLowerCase()
    .includes(OPENZEPPELIN_CHANNELS_HOSTNAME);
  if (!isManagedChannels && !pluginId) {
    return null;
  }

  const normalizedPath = relayUrl.pathname.replace(/\/+$/g, "");
  relayUrl.pathname = normalizedPath.length > 0 ? normalizedPath : "/";

  return {
    baseUrl: relayUrl.toString(),
    apiKey,
    pluginId,
    timeoutMs: parseInteger(
      env.RELAYER_REQUEST_TIMEOUT_MS,
      DEFAULT_RELAYER_REQUEST_TIMEOUT_MS,
      1_000,
    ),
  };
}

export function resolveDirectClaimConfig(env: WorkerEnv): DirectClaimConfig | null {
  const scoreContractId = nonEmpty(env.SCORE_CONTRACT_ID);
  const channels = resolveChannelsConfig(env);
  if (!scoreContractId || !channels) {
    return null;
  }

  return {
    scoreContractId,
    channels,
  };
}

export function isDirectClaimConfigured(env: WorkerEnv): boolean {
  return resolveDirectClaimConfig(env) !== null;
}

function hexToBytes(hex: string, fieldName: string): Uint8Array {
  const normalized = hex.trim().toLowerCase();
  if (normalized.length === 0 || normalized.length % 2 !== 0 || /[^0-9a-f]/.test(normalized)) {
    throw new Error(`${fieldName} must be a valid even-length hex string`);
  }

  const bytes = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function asObject(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  return value as Record<string, unknown>;
}

function asByte(value: unknown, fieldName: string, index: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > 255) {
    throw new Error(`${fieldName}[${index}] must be a byte`);
  }
  return value & 0xff;
}

function asU32(value: unknown, fieldName: string, index: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${fieldName}[${index}] must be a u32`);
  }
  return value >>> 0;
}

function extractGroth16SealFromProverResponse(proverResponse: unknown): Uint8Array {
  const responseObj = asObject(proverResponse);
  const resultObj = responseObj ? asObject(responseObj.result) : null;
  const proofObj = resultObj ? asObject(resultObj.proof) : null;
  const receiptObj = proofObj ? asObject(proofObj.receipt) : null;
  const innerObj = receiptObj ? asObject(receiptObj.inner) : null;
  const groth16 = innerObj ? asObject(innerObj.Groth16) : null;

  if (!groth16) {
    throw new Error("prover_response.result.proof.receipt.inner.Groth16 is required");
  }

  const seal = groth16.seal;
  const verifierParameters = groth16.verifier_parameters;
  if (!Array.isArray(seal) || seal.length !== 256) {
    throw new Error("receipt.inner.Groth16.seal must be a 256-byte array");
  }
  if (!Array.isArray(verifierParameters) || verifierParameters.length !== 8) {
    throw new Error("receipt.inner.Groth16.verifier_parameters must be an 8-word array");
  }

  const rawSeal = Uint8Array.from(
    seal.map((value, index) => asByte(value, "receipt.inner.Groth16.seal", index)),
  );
  const params = verifierParameters.map((value, index) =>
    asU32(value, "receipt.inner.Groth16.verifier_parameters", index),
  );

  const paramsBytes = new Uint8Array(32);
  const paramsView = new DataView(paramsBytes.buffer);
  for (let index = 0; index < params.length; index += 1) {
    paramsView.setUint32(index * 4, params[index], true);
  }

  const selector = paramsBytes.slice(0, 4);
  const stellarSeal = new Uint8Array(260);
  stellarSeal.set(selector, 0);
  stellarSeal.set(rawSeal, 4);
  return stellarSeal;
}

interface SorobanInvokePayload {
  func: string;
  auth: string[];
}

function retryableChannelsExecutionCode(rawCode: string | null): boolean {
  if (!rawCode) {
    return false;
  }
  const code = rawCode.toLowerCase();
  return (
    code === "pool_capacity" ||
    code === "rate_limit" ||
    code === "locked_conflict" ||
    code === "service_unavailable"
  );
}

function extractExecutionCode(details: unknown): string | null {
  if (!details || typeof details !== "object") {
    return null;
  }
  const code = (details as Record<string, unknown>).code;
  if (typeof code === "string" && code.trim().length > 0) {
    return code.trim();
  }
  return null;
}

function isRetryableChannelsExecution(message: string, code: string | null): boolean {
  if (code?.toLowerCase() === "simulation_failed") {
    // Channels can surface transient RPC/plugin errors as SIMULATION_FAILED.
    // Only treat it as fatal when the message clearly carries deterministic
    // contract/input failures.
    return isRetryableDirectClaimMessage(message);
  }

  if (retryableChannelsExecutionCode(code)) {
    return true;
  }
  const normalized = message.toLowerCase();
  return (
    (normalized.includes("internal error") && normalized.includes("reference")) ||
    normalized.includes("too many transactions queued") ||
    normalized.includes("temporarily unavailable") ||
    normalized.includes("try again later")
  );
}

function buildChannelsClient(config: ChannelsConfig): ChannelsClient {
  if (config.pluginId) {
    return new ChannelsClient({
      baseUrl: config.baseUrl,
      apiKey: config.apiKey,
      pluginId: config.pluginId,
      timeout: config.timeoutMs,
    });
  }

  return new ChannelsClient({
    baseUrl: config.baseUrl,
    apiKey: config.apiKey,
    timeout: config.timeoutMs,
  });
}

export function isRetryableDirectClaimMessage(rawMessage: string): boolean {
  const message = rawMessage.toLowerCase();

  if (
    message.includes("error(contract") ||
    message.includes("hosterror") ||
    message.includes("missing proof") ||
    message.includes("required") ||
    message.includes("must be a valid") ||
    message.includes("score not improved") ||
    message.includes("already claimed")
  ) {
    return false;
  }

  return (
    message.includes("network connection lost") ||
    message.includes("failed to fetch") ||
    (message.includes("internal error") && message.includes("reference")) ||
    message.includes("reference =") ||
    message.includes("network error") ||
    message.includes("networkerror") ||
    message.includes("connection lost") ||
    message.includes("connection reset") ||
    message.includes("connection refused") ||
    message.includes("simulation failed") ||
    message.includes("simulation_failed") ||
    message.includes("socket hang up") ||
    message.includes("temporarily unavailable") ||
    message.includes("timed out") ||
    message.includes("timeout") ||
    message.includes("etimedout") ||
    message.includes("econnreset") ||
    message.includes("econnrefused") ||
    message.includes("enotfound") ||
    message.includes("try again later") ||
    message.includes("http 429") ||
    message.includes("http 500") ||
    message.includes("http 502") ||
    message.includes("http 503") ||
    message.includes("http 504")
  );
}

async function submitSorobanOperationViaChannels(
  config: ChannelsConfig,
  payload: SorobanInvokePayload,
): Promise<RelaySubmitResult> {
  const client = buildChannelsClient(config);

  try {
    const result = await client.submitSorobanTransaction(payload);

    const txHash = result.hash?.trim() ?? "";
    const status = result.status?.trim().toLowerCase() ?? "";
    if (txHash.length === 0) {
      return {
        type: status === "failed" || status === "error" ? "fatal" : "retry",
        message: "channels relayer accepted soroban transaction but did not return hash",
      };
    }

    return {
      type: "success",
      txHash,
    };
  } catch (error) {
    if (error instanceof PluginTransportError) {
      return {
        type: "retry",
        message: `channels relayer transport failed: ${error.message}`,
      };
    }

    if (error instanceof PluginExecutionError) {
      const code = extractExecutionCode(error.errorDetails);
      const detail = code ? `${error.message} (${code})` : error.message;
      return {
        type: isRetryableChannelsExecution(error.message, code) ? "retry" : "fatal",
        message: `channels relayer soroban submission failed: ${detail}`,
      };
    }

    const detail = safeErrorMessage(error);
    return {
      type: isRetryableDirectClaimMessage(detail) ? "retry" : "fatal",
      message: `channels relayer soroban submission failed: ${detail}`,
    };
  }
}

async function buildSubmitScorePayloadViaBindings(
  scoreContractId: string,
  seal: Uint8Array,
  journalRaw: Uint8Array,
  claimantAddress: string,
): Promise<SorobanInvokePayload> {
  const client = new ScoreContractClient({
    contractId: scoreContractId,
    rpcUrl: DEFAULT_BINDINGS_RPC_URL,
    networkPassphrase: TESTNET_NETWORK_PASSPHRASE,
  });

  type SubmitScoreArgs = Parameters<ScoreContractClient["submit_score"]>[0];
  const args: SubmitScoreArgs = {
    seal: seal as unknown as SubmitScoreArgs["seal"],
    journal_raw: journalRaw as unknown as SubmitScoreArgs["journal_raw"],
    claimant: claimantAddress,
  };

  const assembled = await client.submit_score(args, { simulate: false });
  const built = assembled.raw?.build();
  const operation = built?.operations?.[0] as
    | {
        func?: xdr.HostFunction;
        auth?: xdr.SorobanAuthorizationEntry[];
      }
    | undefined;

  if (!operation?.func) {
    throw new Error("generated bindings did not produce invokeHostFunction operation");
  }

  const authEntries = Array.isArray(operation.auth) ? operation.auth : [];
  return {
    func: operation.func.toXDR("base64"),
    auth: authEntries.map((entry) => entry.toXDR("base64")),
  };
}

export async function submitClaimDirect(
  env: WorkerEnv,
  request: RelayClaimRequest,
): Promise<RelaySubmitResult> {
  const config = resolveDirectClaimConfig(env);
  if (!config) {
    return {
      type: "fatal",
      message:
        "direct claim is not configured; set SCORE_CONTRACT_ID, RELAYER_URL, and RELAYER_API_KEY for relayer-only submission",
    };
  }

  let phase = "init";
  try {
    phase = "parse_payload";
    const seal = extractGroth16SealFromProverResponse(request.proverResponse);
    const journalRaw = hexToBytes(request.journalRawHex, "journal_raw_hex");
    // Validate claimant formatting before constructing invoke payload.
    Address.fromString(request.claimantAddress);

    phase = "build_payload_bindings";
    const payload = await buildSubmitScorePayloadViaBindings(
      config.scoreContractId,
      seal,
      journalRaw,
      request.claimantAddress,
    );

    console.log("[claim-direct] relayer-only submit", {
      jobId: request.jobId,
      relayerUrl: config.channels.baseUrl,
      claimant: request.claimantAddress,
      journalDigestHex: request.journalDigestHex,
      journalBytes: journalRaw.length,
      sealBytes: seal.length,
      authEntries: payload.auth.length,
    });

    phase = "send_tx_channels_soroban";
    return submitSorobanOperationViaChannels(config.channels, payload);
  } catch (error) {
    const detail = safeErrorMessage(error);
    console.error("[claim-direct] submit failed", {
      jobId: request.jobId,
      phase,
      message: detail,
    });

    const retryable = isRetryableDirectClaimMessage(detail);
    return {
      type: retryable ? "retry" : "fatal",
      message: `direct claim failed during ${phase}: ${detail}`,
    };
  }
}
