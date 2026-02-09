import {
  DEFAULT_POLL_BUDGET_MS,
  DEFAULT_POLL_INTERVAL_MS,
  DEFAULT_POLL_TIMEOUT_MS,
  DEFAULT_PROVER_HEALTH_CACHE_MS,
  DEFAULT_PROVER_REQUEST_TIMEOUT_MS,
  EXPECTED_RULES_DIGEST,
  EXPECTED_RULESET,
  RETRYABLE_JOB_ERROR_CODES,
} from "../constants";
import type { WorkerEnv } from "../env";
import type {
  ProverCreateJobResponse,
  ProverErrorResponse,
  ProverGetJobResponse,
  ProverHealthResponse,
  ProverPollResult,
  ProverSubmitResult,
  ProofResultSummary,
} from "../types";
import { isLocalHostname, parseBoolean, parseInteger, safeErrorMessage, sleep } from "../utils";

export interface ValidatedProverHealth {
  imageId: string;
  rulesDigest: number;
  rulesDigestHex: string;
  ruleset: string;
}

class ProverHealthCheckError extends Error {
  readonly retryable: boolean;

  constructor(message: string, retryable: boolean) {
    super(message);
    this.name = "ProverHealthCheckError";
    this.retryable = retryable;
  }
}

let proverHealthCache: {
  cacheKey: string;
  fetchedAtMs: number;
  value: ValidatedProverHealth;
} | null = null;

function normalizeHex32Bytes(raw: string): string | null {
  const normalized = raw.trim().toLowerCase().replace(/^0x/, "");
  return /^[0-9a-f]{64}$/.test(normalized) ? normalized : null;
}

function buildProverUrl(env: WorkerEnv, pathname: string): URL {
  const base = env.PROVER_BASE_URL?.trim();
  if (!base) {
    throw new Error("missing PROVER_BASE_URL");
  }

  const url = new URL(pathname, base);
  if (
    url.protocol !== "https:" &&
    !parseBoolean(env.ALLOW_INSECURE_PROVER_URL, false) &&
    !isLocalHostname(url.hostname)
  ) {
    throw new Error("PROVER_BASE_URL must use https in production");
  }

  return url;
}

function buildProverCreateUrl(env: WorkerEnv): URL {
  const url = buildProverUrl(env, "/api/jobs/prove-tape/raw");

  const receiptKind = env.PROVER_RECEIPT_KIND?.trim();
  if (receiptKind) {
    url.searchParams.set("receipt_kind", receiptKind);
  }

  const segmentLimitPo2 = parseInteger(env.PROVER_SEGMENT_LIMIT_PO2, 19, 1);
  url.searchParams.set("segment_limit_po2", String(segmentLimitPo2));

  const maxFrames = parseInteger(env.PROVER_MAX_FRAMES, 18_000, 1);
  url.searchParams.set("max_frames", String(maxFrames));

  const verifyReceipt = parseBoolean(env.PROVER_VERIFY_RECEIPT, true);
  url.searchParams.set("verify_receipt", verifyReceipt ? "true" : "false");

  return url;
}

function buildProverStatusUrl(env: WorkerEnv, proverJobId: string): URL {
  return buildProverUrl(env, `/api/jobs/${proverJobId}`);
}

function buildProverHealthUrl(env: WorkerEnv): URL {
  return buildProverUrl(env, "/health");
}

function buildProverHeaders(env: WorkerEnv, includeContentType: boolean): Headers {
  const headers = new Headers();

  if (includeContentType) {
    headers.set("content-type", "application/octet-stream");
  }

  if (env.PROVER_API_KEY) {
    headers.set("x-api-key", env.PROVER_API_KEY);
  }

  if (env.PROVER_ACCESS_CLIENT_ID && env.PROVER_ACCESS_CLIENT_SECRET) {
    headers.set("CF-Access-Client-Id", env.PROVER_ACCESS_CLIENT_ID);
    headers.set("CF-Access-Client-Secret", env.PROVER_ACCESS_CLIENT_SECRET);
  }

  return headers;
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

async function parseJson<T>(response: Response): Promise<T> {
  return (await response.json()) as T;
}

export function describeProverHealthError(error: unknown): { retryable: boolean; message: string } {
  if (error instanceof ProverHealthCheckError) {
    return {
      retryable: error.retryable,
      message: error.message,
    };
  }

  return {
    retryable: true,
    message: safeErrorMessage(error),
  };
}

function cacheKeyForHealthCheck(env: WorkerEnv): string {
  const proverBaseUrl = env.PROVER_BASE_URL?.trim() ?? "";
  const expectedImageId = env.PROVER_EXPECTED_IMAGE_ID?.trim() ?? "";
  return `${proverBaseUrl}|${expectedImageId}`;
}

export async function getValidatedProverHealth(
  env: WorkerEnv,
  options?: { forceRefresh?: boolean },
): Promise<ValidatedProverHealth> {
  const forceRefresh = options?.forceRefresh ?? false;
  const cacheMs = parseInteger(env.PROVER_HEALTH_CACHE_MS, DEFAULT_PROVER_HEALTH_CACHE_MS, 1_000);
  const cacheKey = cacheKeyForHealthCheck(env);
  const now = Date.now();

  if (
    !forceRefresh &&
    proverHealthCache &&
    proverHealthCache.cacheKey === cacheKey &&
    now - proverHealthCache.fetchedAtMs <= cacheMs
  ) {
    return proverHealthCache.value;
  }

  const timeoutMs = parseInteger(
    env.PROVER_REQUEST_TIMEOUT_MS,
    DEFAULT_PROVER_REQUEST_TIMEOUT_MS,
    1_000,
  );

  let response: Response;
  try {
    response = await fetchWithTimeout(
      buildProverHealthUrl(env),
      {
        method: "GET",
        headers: buildProverHeaders(env, false),
      },
      timeoutMs,
    );
  } catch (error) {
    throw new ProverHealthCheckError(
      `failed reaching prover health endpoint: ${safeErrorMessage(error)}`,
      true,
    );
  }

  if (response.status >= 500 || response.status === 429) {
    throw new ProverHealthCheckError(`prover health endpoint returned ${response.status}`, true);
  }

  if (!response.ok) {
    let detail = "";
    try {
      detail = await response.text();
    } catch {
      // Ignore parse errors.
    }
    throw new ProverHealthCheckError(
      `prover health endpoint returned ${response.status}: ${detail || "no body"}`,
      false,
    );
  }

  let payload: ProverHealthResponse;
  try {
    payload = await parseJson<ProverHealthResponse>(response);
  } catch (error) {
    throw new ProverHealthCheckError(
      `failed parsing prover health response: ${safeErrorMessage(error)}`,
      true,
    );
  }

  const normalizedImageId =
    typeof payload.image_id === "string" ? normalizeHex32Bytes(payload.image_id) : null;
  if (!normalizedImageId) {
    throw new ProverHealthCheckError(
      "prover health missing valid image_id (expected 32-byte hex)",
      false,
    );
  }

  const rulesDigest =
    typeof payload.rules_digest === "number" && Number.isFinite(payload.rules_digest)
      ? payload.rules_digest >>> 0
      : null;
  if (rulesDigest === null) {
    throw new ProverHealthCheckError("prover health missing rules_digest (u32)", false);
  }

  if (rulesDigest !== EXPECTED_RULES_DIGEST >>> 0) {
    throw new ProverHealthCheckError(
      `prover health rules_digest mismatch: 0x${rulesDigest.toString(16).padStart(8, "0")} (expected 0x${EXPECTED_RULES_DIGEST.toString(16).padStart(8, "0")})`,
      false,
    );
  }

  const expectedImageIdRaw = env.PROVER_EXPECTED_IMAGE_ID?.trim();
  if (expectedImageIdRaw && expectedImageIdRaw.length > 0) {
    const normalizedExpectedImageId = normalizeHex32Bytes(expectedImageIdRaw);
    if (!normalizedExpectedImageId) {
      throw new ProverHealthCheckError("PROVER_EXPECTED_IMAGE_ID must be 32-byte hex", false);
    }
    if (normalizedExpectedImageId !== normalizedImageId) {
      throw new ProverHealthCheckError(
        `prover health image_id mismatch: ${normalizedImageId} (expected ${normalizedExpectedImageId})`,
        false,
      );
    }
  }

  const validated: ValidatedProverHealth = {
    imageId: normalizedImageId,
    rulesDigest,
    rulesDigestHex: `0x${rulesDigest.toString(16).padStart(8, "0")}`,
    ruleset: typeof payload.ruleset === "string" ? payload.ruleset : EXPECTED_RULESET,
  };

  proverHealthCache = {
    cacheKey,
    fetchedAtMs: now,
    value: validated,
  };

  return validated;
}

export async function submitToProver(
  env: WorkerEnv,
  tapeBytes: Uint8Array,
): Promise<ProverSubmitResult> {
  try {
    await getValidatedProverHealth(env);
  } catch (error) {
    const healthError = describeProverHealthError(error);
    return {
      type: healthError.retryable ? "retry" : "fatal",
      message: `prover health check failed: ${healthError.message}`,
    };
  }

  const timeoutMs = parseInteger(
    env.PROVER_REQUEST_TIMEOUT_MS,
    DEFAULT_PROVER_REQUEST_TIMEOUT_MS,
    1_000,
  );

  let response: Response;
  try {
    response = await fetchWithTimeout(
      buildProverCreateUrl(env),
      {
        method: "POST",
        headers: buildProverHeaders(env, true),
        body: tapeBytes,
      },
      timeoutMs,
    );
  } catch (error) {
    return {
      type: "retry",
      message: `failed reaching prover create endpoint: ${safeErrorMessage(error)}`,
    };
  }

  if (response.status === 429 || response.status >= 500) {
    let errorBody: ProverErrorResponse | undefined;
    try {
      errorBody = (await response.json()) as ProverErrorResponse;
    } catch {
      // Ignore parse errors.
    }
    const codePart = errorBody?.error_code ? ` (${errorBody.error_code})` : "";
    const detailPart = errorBody?.error ? `: ${errorBody.error}` : "";
    return {
      type: "retry",
      message: `prover create endpoint returned ${response.status}${codePart}${detailPart}`,
    };
  }

  if (!response.ok) {
    let detail = "";
    try {
      detail = await response.text();
    } catch {
      // Ignore parse errors.
    }

    return {
      type: "fatal",
      message: `prover rejected tape submission (${response.status}): ${detail || "no body"}`,
    };
  }

  let payload: ProverCreateJobResponse;
  try {
    payload = await parseJson<ProverCreateJobResponse>(response);
  } catch (error) {
    return {
      type: "retry",
      message: `failed parsing prover create response: ${safeErrorMessage(error)}`,
    };
  }

  if (!payload.success || !payload.job_id) {
    return {
      type: "fatal",
      message: payload.error ?? "prover create response was missing job_id",
    };
  }

  const statusUrl = payload.status_url || `/api/jobs/${payload.job_id}`;
  return {
    type: "success",
    jobId: payload.job_id,
    statusUrl,
  };
}

export async function pollProver(env: WorkerEnv, proverJobId: string): Promise<ProverPollResult> {
  const requestTimeoutMs = parseInteger(
    env.PROVER_REQUEST_TIMEOUT_MS,
    DEFAULT_PROVER_REQUEST_TIMEOUT_MS,
    1_000,
  );
  const pollTimeoutMs = parseInteger(env.PROVER_POLL_TIMEOUT_MS, DEFAULT_POLL_TIMEOUT_MS, 5_000);
  const pollIntervalMs = parseInteger(env.PROVER_POLL_INTERVAL_MS, DEFAULT_POLL_INTERVAL_MS, 500);
  const pollBudgetMs = parseInteger(
    env.PROVER_POLL_BUDGET_MS,
    DEFAULT_POLL_BUDGET_MS,
    pollIntervalMs,
  );

  const budgetDeadline = Date.now() + pollBudgetMs;
  const absoluteDeadline = Date.now() + pollTimeoutMs;

  // Polling is intentionally sequential to preserve strict single-job semantics.
  /* eslint-disable no-await-in-loop */
  while (Date.now() < budgetDeadline && Date.now() < absoluteDeadline) {
    let response: Response;
    try {
      response = await fetchWithTimeout(
        buildProverStatusUrl(env, proverJobId),
        {
          method: "GET",
          headers: buildProverHeaders(env, false),
        },
        requestTimeoutMs,
      );
    } catch (error) {
      return {
        type: "retry",
        message: `failed reading prover status: ${safeErrorMessage(error)}`,
      };
    }

    if (response.status === 429 || response.status >= 500) {
      let errorBody: ProverErrorResponse | undefined;
      try {
        errorBody = (await response.json()) as ProverErrorResponse;
      } catch {
        // Ignore parse errors.
      }
      const codePart = errorBody?.error_code ? ` (${errorBody.error_code})` : "";
      const detailPart = errorBody?.error ? `: ${errorBody.error}` : "";
      return {
        type: "retry",
        message: `prover status endpoint returned ${response.status}${codePart}${detailPart}`,
      };
    }

    // A 404 means the prover lost the job (crash/restart). The tape is
    // still valid — clear the prover job ID so the next attempt
    // re-submits rather than polling a dead job forever.
    if (response.status === 404) {
      return {
        type: "retry",
        message: "prover job not found (likely prover restart); will re-submit",
        clearProverJob: true,
      };
    }

    if (!response.ok) {
      let detail = "";
      try {
        detail = await response.text();
      } catch {
        // Ignore parse errors.
      }

      return {
        type: "fatal",
        message: `prover status endpoint returned ${response.status}: ${detail || "no body"}`,
      };
    }

    let payload: ProverGetJobResponse;
    try {
      payload = await parseJson<ProverGetJobResponse>(response);
    } catch (error) {
      return {
        type: "retry",
        message: `failed parsing prover status response: ${safeErrorMessage(error)}`,
      };
    }

    if (payload.status === "succeeded") {
      if (!payload.result?.proof || !payload.result.proof.journal || !payload.result.proof.stats) {
        return {
          type: "retry",
          message: "prover reported success but result payload was incomplete; will re-submit",
          clearProverJob: true,
        };
      }
      return {
        type: "success",
        response: payload,
      };
    }

    if (payload.status === "failed") {
      if (payload.error_code && RETRYABLE_JOB_ERROR_CODES.has(payload.error_code)) {
        return {
          type: "retry",
          message: `prover job failed with retryable error_code=${payload.error_code}: ${payload.error ?? "unknown"}`,
          clearProverJob: true,
        };
      }
      const codePart = payload.error_code ? ` (error_code=${payload.error_code})` : "";
      return {
        type: "fatal",
        message: payload.error
          ? `prover marked job as failed${codePart}: ${payload.error}`
          : `prover marked job as failed${codePart}`,
      };
    }

    if (payload.status !== "queued" && payload.status !== "running") {
      return {
        type: "fatal",
        message: `prover returned unknown job status: ${payload.status}`,
      };
    }

    if (Date.now() + pollIntervalMs >= budgetDeadline) {
      return {
        type: "running",
        status: payload.status,
      };
    }

    await sleep(pollIntervalMs);
  }
  /* eslint-enable no-await-in-loop */

  return {
    type: "running",
    status: "running",
  };
}

export function summarizeProof(response: ProverGetJobResponse): ProofResultSummary {
  const result = response.result;
  if (!result) {
    throw new Error("prover result payload missing");
  }
  const digest = result.proof.journal.rules_digest >>> 0;
  if (digest !== EXPECTED_RULES_DIGEST >>> 0) {
    throw new Error(
      `unexpected rules digest 0x${digest.toString(16).padStart(8, "0")} (expected 0x${EXPECTED_RULES_DIGEST.toString(16).padStart(8, "0")})`,
    );
  }

  return {
    elapsedMs: result.elapsed_ms,
    requestedReceiptKind: result.proof.requested_receipt_kind,
    producedReceiptKind: result.proof.produced_receipt_kind ?? null,
    journal: result.proof.journal,
    stats: result.proof.stats,
  };
}
