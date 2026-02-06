import {
  DEFAULT_POLL_BUDGET_MS,
  DEFAULT_POLL_INTERVAL_MS,
  DEFAULT_POLL_TIMEOUT_MS,
  DEFAULT_PROVER_REQUEST_TIMEOUT_MS,
} from "../constants";
import type { WorkerEnv } from "../env";
import type {
  ProverCreateJobResponse,
  ProverGetJobResponse,
  ProverPollResult,
  ProverSubmitResult,
  ProofResultSummary,
} from "../types";
import { isLocalHostname, parseBoolean, parseInteger, safeErrorMessage, sleep } from "../utils";

function buildProverCreateUrl(env: WorkerEnv): URL {
  const base = env.PROVER_BASE_URL?.trim();
  if (!base) {
    throw new Error("missing PROVER_BASE_URL");
  }

  const url = new URL("/api/jobs/prove-tape/raw", base);
  if (
    url.protocol !== "https:" &&
    !parseBoolean(env.ALLOW_INSECURE_PROVER_URL, false) &&
    !isLocalHostname(url.hostname)
  ) {
    throw new Error("PROVER_BASE_URL must use https in production");
  }

  const receiptKind = env.PROVER_RECEIPT_KIND?.trim();
  if (receiptKind) {
    url.searchParams.set("receipt_kind", receiptKind);
  }

  const segmentLimitPo2 = parseInteger(env.PROVER_SEGMENT_LIMIT_PO2, 19, 1);
  url.searchParams.set("segment_limit_po2", String(segmentLimitPo2));

  const maxFrames = parseInteger(env.PROVER_MAX_FRAMES, 18_000, 1);
  url.searchParams.set("max_frames", String(maxFrames));

  return url;
}

function buildProverStatusUrl(env: WorkerEnv, proverJobId: string): URL {
  const base = env.PROVER_BASE_URL?.trim();
  if (!base) {
    throw new Error("missing PROVER_BASE_URL");
  }

  const url = new URL(`/api/jobs/${proverJobId}`, base);
  if (
    url.protocol !== "https:" &&
    !parseBoolean(env.ALLOW_INSECURE_PROVER_URL, false) &&
    !isLocalHostname(url.hostname)
  ) {
    throw new Error("PROVER_BASE_URL must use https in production");
  }

  return url;
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

export async function submitToProver(
  env: WorkerEnv,
  tapeBytes: Uint8Array,
): Promise<ProverSubmitResult> {
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
    return {
      type: "retry",
      message: `prover create endpoint returned ${response.status}`,
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
  // eslint-disable-next-line no-await-in-loop
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
      return {
        type: "retry",
        message: `prover status endpoint returned ${response.status}`,
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
        // eslint-disable-next-line no-await-in-loop
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
      // eslint-disable-next-line no-await-in-loop
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
          type: "fatal",
          message: "prover reported success but result payload was incomplete",
        };
      }
      return {
        type: "success",
        response: payload,
      };
    }

    if (payload.status === "failed") {
      return {
        type: "fatal",
        message: payload.error ?? "prover marked job as failed",
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

    // eslint-disable-next-line no-await-in-loop
    await sleep(pollIntervalMs);
  }

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

  return {
    elapsedMs: result.elapsed_ms,
    requestedReceiptKind: result.proof.requested_receipt_kind,
    producedReceiptKind: result.proof.produced_receipt_kind ?? null,
    journal: result.proof.journal,
    stats: result.proof.stats,
  };
}
