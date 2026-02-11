import type { WorkerEnv } from "../env";
import { isLocalHostname, parseBoolean, parseInteger, safeErrorMessage } from "../utils";

const DEFAULT_CLAIM_RELAY_REQUEST_TIMEOUT_MS = 30_000;

export interface RelayClaimRequest {
  jobId: string;
  claimantAddress: string;
  journalRawHex: string;
  journalDigestHex: string;
  proverResponse: unknown;
}

interface RelaySuccessResponse {
  success: true;
  tx_hash?: string;
}

interface RelayErrorResponse {
  success?: false;
  error?: string;
  error_code?: string;
  retryable?: boolean;
}

export type RelaySubmitResult =
  | { type: "success"; txHash: string }
  | { type: "retry"; message: string }
  | { type: "fatal"; message: string };

function buildRelayUrl(env: WorkerEnv): URL {
  const raw = env.CLAIM_RELAY_URL?.trim();
  if (!raw) {
    throw new Error("missing CLAIM_RELAY_URL");
  }

  const url = new URL(raw);
  if (
    url.protocol !== "https:" &&
    !parseBoolean(env.ALLOW_INSECURE_PROVER_URL, false) &&
    !isLocalHostname(url.hostname)
  ) {
    throw new Error("CLAIM_RELAY_URL must use https in production");
  }

  return url;
}

async function fetchWithTimeout(url: URL, init: RequestInit, timeoutMs: number): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);

  try {
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

export async function submitClaimToRelay(
  env: WorkerEnv,
  request: RelayClaimRequest,
): Promise<RelaySubmitResult> {
  let relayUrl: URL;
  try {
    relayUrl = buildRelayUrl(env);
  } catch (error) {
    return { type: "fatal", message: safeErrorMessage(error) };
  }

  const timeoutMs = parseInteger(
    env.CLAIM_RELAY_REQUEST_TIMEOUT_MS,
    DEFAULT_CLAIM_RELAY_REQUEST_TIMEOUT_MS,
    1_000,
  );

  const headers = new Headers({
    "content-type": "application/json",
  });
  if (env.CLAIM_RELAY_API_KEY?.trim()) {
    headers.set("x-api-key", env.CLAIM_RELAY_API_KEY.trim());
  }

  let response: Response;
  try {
    response = await fetchWithTimeout(
      relayUrl,
      {
        method: "POST",
        headers,
        body: JSON.stringify({
          job_id: request.jobId,
          claimant_address: request.claimantAddress,
          journal_raw_hex: request.journalRawHex,
          journal_digest_hex: request.journalDigestHex,
          prover_response: request.proverResponse,
        }),
      },
      timeoutMs,
    );
  } catch (error) {
    return {
      type: "retry",
      message: `claim relay request failed: ${safeErrorMessage(error)}`,
    };
  }

  if (response.status === 429 || response.status >= 500) {
    let body: RelayErrorResponse | null = null;
    try {
      body = (await response.json()) as RelayErrorResponse;
    } catch {
      // ignore parse failures
    }
    return {
      type: "retry",
      message: `claim relay returned ${response.status}${body?.error ? `: ${body.error}` : ""}`,
    };
  }

  if (!response.ok) {
    let body: RelayErrorResponse | null = null;
    try {
      body = (await response.json()) as RelayErrorResponse;
    } catch {
      // ignore parse failures
    }

    const message = body?.error
      ? `claim relay rejected request: ${body.error}`
      : `claim relay rejected request with status ${response.status}`;

    if (body?.retryable) {
      return { type: "retry", message };
    }
    return { type: "fatal", message };
  }

  let payload: RelaySuccessResponse | RelayErrorResponse;
  try {
    payload = (await response.json()) as RelaySuccessResponse | RelayErrorResponse;
  } catch (error) {
    return {
      type: "retry",
      message: `claim relay returned non-JSON response: ${safeErrorMessage(error)}`,
    };
  }

  if (payload.success) {
    return {
      type: "success",
      txHash: typeof payload.tx_hash === "string" ? payload.tx_hash : "",
    };
  }

  if (payload.retryable) {
    return {
      type: "retry",
      message: payload.error ?? "claim relay asked for retry",
    };
  }

  return {
    type: "fatal",
    message: payload.error ?? "claim relay returned unsuccessful response",
  };
}
