import { Address, Asset, contract, rpc, scValToNative, xdr } from "@stellar/stellar-sdk";
import {
  ChannelsClient,
  PluginExecutionError,
  PluginTransportError,
} from "@openzeppelin/relayer-plugin-channels/dist/client";
import type { TransactionResult } from "smart-account-kit";

export interface SubmitScoreTransactionInput {
  scoreContractId: string;
  claimantAddress: string;
  seal: Uint8Array;
  journalRaw: Uint8Array;
}

export interface TokenBalanceInput {
  walletAddress: string;
  scoreContractId?: string | null;
  tokenContractId?: string | null;
}

export interface TokenBalanceResult {
  tokenContractId: string;
  balance: bigint;
}

type SmartWalletModule = typeof import("../wallet/smartAccount");

let smartWalletModulePromise: Promise<SmartWalletModule> | null = null;

async function loadSmartWalletModule(): Promise<SmartWalletModule> {
  if (!smartWalletModulePromise) {
    smartWalletModulePromise = import("../wallet/smartAccount");
  }
  return smartWalletModulePromise;
}

function asBuffer(bytes: Uint8Array): Buffer {
  // SDK typing requires Buffer, but runtime accepts Uint8Array-compatible bytes.
  return bytes as unknown as Buffer;
}

function normalizeErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }
  return String(error);
}

function formatRelayerError(error: unknown): string {
  if (error instanceof PluginExecutionError) {
    const code =
      typeof error.errorDetails?.code === "string" && error.errorDetails.code.trim().length > 0
        ? ` (${error.errorDetails.code.trim()})`
        : "";
    return `${error.message}${code}`;
  }

  if (error instanceof PluginTransportError) {
    if (typeof error.statusCode === "number") {
      return `${error.message} (status ${error.statusCode})`;
    }
    return error.message;
  }

  return normalizeErrorMessage(error);
}

function mapScoreContractError(errorMessage: string): string | null {
  const normalized = errorMessage.toLowerCase();
  if (!normalized.includes("error(contract, #")) {
    return null;
  }

  if (normalized.includes("error(contract, #3)")) {
    return "this proof journal was already claimed on-chain; submit a new tape/proof";
  }
  if (normalized.includes("error(contract, #5)")) {
    return "score was not improved for this seed/claimant; submit a higher score";
  }
  if (normalized.includes("error(contract, #4)")) {
    return "zero-score proofs are not eligible for minting";
  }
  if (normalized.includes("error(contract, #2)")) {
    return "journal rules digest does not match contract policy (expected AST3)";
  }
  if (normalized.includes("error(contract, #1)")) {
    return "journal payload length is invalid (expected 24 bytes)";
  }

  return "contract rejected the proof submission";
}

export function explainScoreSubmissionError(errorMessage: string): string {
  const mapped = mapScoreContractError(errorMessage);
  if (mapped) {
    return `on-chain submission rejected: ${mapped}`;
  }

  const normalized = errorMessage.toLowerCase();
  if (normalized.includes("failed to fetch")) {
    return "on-chain submission failed: network/relayer request could not be completed (check relayer configuration and connectivity)";
  }

  return errorMessage;
}

function nonEmptyEnv(value: string | undefined): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : null;
}

export function getScoreContractIdFromEnv(): string | null {
  return nonEmptyEnv(import.meta.env.VITE_SCORE_CONTRACT_ID);
}

export function getTokenContractIdFromEnv(): string | null {
  return nonEmptyEnv(import.meta.env.VITE_TOKEN_CONTRACT_ID);
}

function parseU32Result(value: xdr.ScVal): number {
  const tag = value.switch().name;
  if (tag !== "scvU32") {
    throw new Error(`unexpected contract return type: expected u32, got ${tag}`);
  }
  return value.u32() >>> 0;
}

function parseStringResult(value: xdr.ScVal): string {
  const native = scValToNative(value);
  if (typeof native !== "string" || native.trim().length === 0) {
    throw new Error(`expected non-empty string contract result, got ${value.switch().name}`);
  }
  return native;
}

export function parseSacAssetFromName(name: string): Asset {
  const normalized = name.trim();
  if (normalized === "native") {
    return Asset.native();
  }

  const separator = normalized.indexOf(":");
  const missingParts =
    separator <= 0 || separator >= normalized.length - 1 || normalized.indexOf(":", separator + 1) >= 0;
  if (missingParts) {
    throw new Error(`invalid stellar asset name "${name}"`);
  }

  const code = normalized.slice(0, separator);
  const issuer = normalized.slice(separator + 1);

  try {
    return new Asset(code, issuer);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`invalid stellar asset name "${name}": ${detail}`, { cause: error });
  }
}

function extractSacMetadataName(storage: xdr.ScMapEntry[] | null | undefined): string {
  if (!storage || storage.length === 0) {
    throw new Error("stellar asset contract instance storage is empty");
  }

  for (const entry of storage) {
    const key = scValToNative(entry.key());
    if (key !== "METADATA") {
      continue;
    }

    const value = scValToNative(entry.val());
    if (!value || typeof value !== "object") {
      throw new Error("stellar asset metadata has unexpected shape");
    }

    const name = (value as Record<string, unknown>).name;
    if (typeof name !== "string" || name.trim().length === 0) {
      throw new Error("stellar asset metadata is missing name");
    }

    return name;
  }

  throw new Error("stellar asset metadata entry is missing");
}

async function resolveSacAssetFromContractId(
  server: rpc.Server,
  tokenContractId: string,
): Promise<Asset> {
  const instanceEntry = await server.getContractData(
    tokenContractId,
    xdr.ScVal.scvLedgerKeyContractInstance(),
  );
  const instance = instanceEntry.val.contractData().val().instance();
  const executableKind = instance.executable().switch().name;
  if (executableKind !== "contractExecutableStellarAsset") {
    throw new Error(
      `token contract ${tokenContractId} is not a Stellar Asset Contract (${executableKind})`,
    );
  }

  return parseSacAssetFromName(extractSacMetadataName(instance.storage()));
}

export async function submitScoreTransaction(
  input: SubmitScoreTransactionInput,
): Promise<TransactionResult> {
  const walletModule = await loadSmartWalletModule();
  const kit = walletModule.getSmartAccountKit();
  if (!kit.isConnected) {
    return {
      success: false,
      hash: "",
      error: "wallet is not connected",
    };
  }

  const config = walletModule.getSmartAccountConfig();
  const relayerUrl = config.relayerUrl?.trim() ?? "";
  const relayerApiKey = config.relayerApiKey?.trim() ?? "";
  if (relayerUrl.length === 0 || relayerApiKey.length === 0) {
    return {
      success: false,
      hash: "",
      error: "relayer is required; set VITE_RELAYER_URL and VITE_RELAYER_API_KEY",
    };
  }

  const assembled = await contract.AssembledTransaction.build<number>({
    contractId: input.scoreContractId,
    method: "submit_score",
    args: [
      xdr.ScVal.scvBytes(asBuffer(input.seal)),
      xdr.ScVal.scvBytes(asBuffer(input.journalRaw)),
      Address.fromString(input.claimantAddress).toScVal(),
    ],
    parseResultXdr: parseU32Result,
    rpcUrl: config.rpcUrl,
    networkPassphrase: config.networkPassphrase,
    publicKey: kit.deployerPublicKey,
  });

  const built = assembled.raw?.build();
  const operation = built?.operations?.[0] as
    | {
        func?: xdr.HostFunction;
        auth?: xdr.SorobanAuthorizationEntry[];
      }
    | undefined;
  if (!operation?.func) {
    return {
      success: false,
      hash: "",
      error: "failed to build soroban invoke payload for relayer submission",
    };
  }

  const payload = {
    func: operation.func.toXDR("base64"),
    auth: (operation.auth ?? []).map((entry) => entry.toXDR("base64")),
  };

  try {
    const channels = new ChannelsClient({
      baseUrl: relayerUrl,
      apiKey: relayerApiKey,
      pluginId: config.relayerPluginId ?? undefined,
    });
    const result = await channels.submitSorobanTransaction(payload);
    const txHash = result.hash?.trim() ?? "";
    if (txHash.length === 0) {
      return {
        success: false,
        hash: "",
        error: "relayer accepted submit_score but did not return tx hash",
      };
    }

    return {
      success: true,
      hash: txHash,
    };
  } catch (error) {
    return {
      success: false,
      hash: "",
      error: formatRelayerError(error),
    };
  }
}

export async function resolveTokenContractId(scoreContractId: string): Promise<string> {
  const walletModule = await loadSmartWalletModule();
  const config = walletModule.getSmartAccountConfig();
  const kit = walletModule.getSmartAccountKit();

  const tx = await contract.AssembledTransaction.build<string>({
    contractId: scoreContractId,
    method: "token_id",
    args: [],
    parseResultXdr: parseStringResult,
    rpcUrl: config.rpcUrl,
    networkPassphrase: config.networkPassphrase,
    publicKey: kit.deployerPublicKey,
  });

  return tx.result;
}

export async function readTokenBalance(input: TokenBalanceInput): Promise<TokenBalanceResult> {
  const tokenContractId =
    input.tokenContractId?.trim() ||
    (input.scoreContractId ? await resolveTokenContractId(input.scoreContractId) : "");
  if (!tokenContractId) {
    throw new Error(
      "token contract is not configured; set VITE_TOKEN_CONTRACT_ID or VITE_SCORE_CONTRACT_ID",
    );
  }

  const walletModule = await loadSmartWalletModule();
  const config = walletModule.getSmartAccountConfig();
  const server = new rpc.Server(config.rpcUrl);
  const holderAddress = Address.fromString(input.walletAddress).toString();
  const asset = await resolveSacAssetFromContractId(server, tokenContractId);
  const balance = await server.getAssetBalance(holderAddress, asset, config.networkPassphrase);

  return {
    tokenContractId,
    balance: balance.balanceEntry?.amount ? BigInt(balance.balanceEntry.amount) : 0n,
  };
}
