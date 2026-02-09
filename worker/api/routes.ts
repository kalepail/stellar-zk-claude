import { Hono } from "hono";
import { DEFAULT_MAX_TAPE_BYTES, EXPECTED_RULES_DIGEST, EXPECTED_RULESET } from "../constants";
import { asPublicJob, coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { resultKey } from "../keys";
import { describeProverHealthError, getValidatedProverHealth } from "../prover/client";
import { parseAndValidateTape } from "../tape";
import { parseInteger, safeErrorMessage } from "../utils";

class PayloadTooLargeError extends Error {
  readonly sizeBytes: number;
  readonly maxBytes: number;

  constructor(sizeBytes: number, maxBytes: number) {
    super(`tape payload too large: ${sizeBytes} bytes (max ${maxBytes})`);
    this.name = "PayloadTooLargeError";
    this.sizeBytes = sizeBytes;
    this.maxBytes = maxBytes;
  }
}

function parseContentLength(value: string | undefined): number | null {
  if (!value) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return null;
  }

  return parsed;
}

async function readRequestBodyWithLimit(
  request: Request,
  maxTapeBytes: number,
): Promise<Uint8Array> {
  const reader = request.body?.getReader();
  if (!reader) {
    return new Uint8Array();
  }

  const chunks: Uint8Array[] = [];
  let totalSize = 0;

  try {
    /* eslint-disable no-await-in-loop */
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }

      if (!value || value.byteLength === 0) {
        continue;
      }

      totalSize += value.byteLength;
      if (totalSize > maxTapeBytes) {
        void reader.cancel("payload too large");
        throw new PayloadTooLargeError(totalSize, maxTapeBytes);
      }
      chunks.push(value);
    }
    /* eslint-enable no-await-in-loop */
  } finally {
    reader.releaseLock();
  }

  const body = new Uint8Array(totalSize);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }

  return body;
}

function jsonError(
  c: { json: (body: unknown, status?: number) => Response },
  status: number,
  error: string,
): Response {
  return c.json(
    {
      success: false,
      error,
    },
    status,
  );
}

export function createApiRouter(): Hono<{ Bindings: WorkerEnv }> {
  const api = new Hono<{ Bindings: WorkerEnv }>();

  api.get("/health", async (c) => {
    const coordinator = coordinatorStub(c.env);
    const activeJob = await coordinator.getActiveJob();
    const expectedImageIdRaw = c.env.PROVER_EXPECTED_IMAGE_ID?.trim() ?? "";
    const expectedImageId = expectedImageIdRaw.length > 0 ? expectedImageIdRaw : null;

    let prover:
      | {
          status: "compatible";
          image_id: string;
          rules_digest_hex: string;
          ruleset: string;
        }
      | {
          status: "degraded";
          error: string;
        };

    try {
      const health = await getValidatedProverHealth(c.env);
      prover = {
        status: "compatible",
        image_id: health.imageId,
        rules_digest_hex: health.rulesDigestHex,
        ruleset: health.ruleset,
      };
    } catch (error) {
      const healthError = describeProverHealthError(error);
      prover = {
        status: "degraded",
        error: healthError.message,
      };
    }

    return c.json({
      success: true,
      service: "stellar-zk-proof-gateway",
      mode: "single-active-job",
      expected: {
        rules_digest_hex: `0x${(EXPECTED_RULES_DIGEST >>> 0).toString(16).padStart(8, "0")}`,
        ruleset: EXPECTED_RULESET,
        image_id: expectedImageId,
      },
      checked_at: new Date().toISOString(),
      prover,
      active_job: activeJob ? asPublicJob(activeJob) : null,
    });
  });

  api.post("/proofs/jobs", async (c) => {
    const maxTapeBytes = parseInteger(c.env.MAX_TAPE_BYTES, DEFAULT_MAX_TAPE_BYTES, 1);
    const declaredLength = parseContentLength(c.req.header("content-length"));
    if (declaredLength !== null && declaredLength > maxTapeBytes) {
      return jsonError(
        c,
        413,
        `tape payload too large: ${declaredLength} bytes (max ${maxTapeBytes})`,
      );
    }

    let tapeBytes: Uint8Array;
    try {
      tapeBytes = await readRequestBodyWithLimit(c.req.raw, maxTapeBytes);
    } catch (error) {
      if (error instanceof PayloadTooLargeError) {
        return jsonError(c, 413, error.message);
      }
      return jsonError(c, 400, `failed reading request body: ${safeErrorMessage(error)}`);
    }

    let metadata;
    try {
      metadata = parseAndValidateTape(tapeBytes, maxTapeBytes);
    } catch (error) {
      return jsonError(c, 400, safeErrorMessage(error));
    }

    const coordinator = coordinatorStub(c.env);
    const createResult = await coordinator.createJob({
      sizeBytes: tapeBytes.byteLength,
      metadata,
    });

    if (!createResult.accepted) {
      return c.json(
        {
          success: false,
          error: "proof queue is currently busy; retry when the active job completes",
          active_job: asPublicJob(createResult.activeJob),
        },
        429,
      );
    }

    const { job } = createResult;

    try {
      await c.env.PROOF_ARTIFACTS.put(job.tape.key, tapeBytes, {
        httpMetadata: {
          contentType: "application/octet-stream",
        },
        customMetadata: {
          jobId: job.jobId,
        },
      });
    } catch (error) {
      await coordinator.markFailed(
        job.jobId,
        `failed storing tape in R2: ${safeErrorMessage(error)}`,
      );
      return jsonError(c, 503, "failed storing tape artifact");
    }

    try {
      await c.env.PROOF_QUEUE.send(
        {
          jobId: job.jobId,
        },
        {
          contentType: "json",
        },
      );
    } catch (error) {
      await coordinator.markFailed(
        job.jobId,
        `failed enqueueing proof job: ${safeErrorMessage(error)}`,
      );
      await c.env.PROOF_ARTIFACTS.delete(job.tape.key);
      return jsonError(c, 503, "failed enqueueing proof job");
    }

    const refreshed = await coordinator.getJob(job.jobId);
    if (!refreshed) {
      return jsonError(c, 500, "job disappeared after enqueue");
    }

    return c.json(
      {
        success: true,
        status_url: `/api/proofs/jobs/${job.jobId}`,
        job: asPublicJob(refreshed),
      },
      202,
    );
  });

  api.get("/proofs/jobs/:jobId", async (c) => {
    const jobId = c.req.param("jobId");
    if (!jobId) {
      return jsonError(c, 400, "invalid job id in path");
    }

    const coordinator = coordinatorStub(c.env);
    const job = await coordinator.getJob(jobId);
    if (!job) {
      return jsonError(c, 404, `job not found: ${jobId}`);
    }

    return c.json({
      success: true,
      job: asPublicJob(job),
    });
  });

  api.get("/proofs/jobs/:jobId/result", async (c) => {
    const jobId = c.req.param("jobId");
    if (!jobId) {
      return jsonError(c, 400, "invalid job id in path");
    }

    // Try the DO first for the canonical artifact key.
    const coordinator = coordinatorStub(c.env);
    const job = await coordinator.getJob(jobId);

    let artifact: R2ObjectBody | null = null;

    if (job?.result?.artifactKey) {
      artifact = await c.env.PROOF_ARTIFACTS.get(job.result.artifactKey);
    } else if (!job) {
      // DO record was pruned — fall back to the well-known R2 key.
      // result.json is retained in R2 beyond DO pruning so users can
      // fetch proof data for on-chain submission.
      artifact = await c.env.PROOF_ARTIFACTS.get(resultKey(jobId));
    }

    if (!artifact) {
      if (job && !job.result?.artifactKey) {
        return jsonError(c, 409, "proof result is not available for this job");
      }
      return jsonError(c, 404, "proof result not found");
    }

    return new Response(artifact.body, {
      status: 200,
      headers: {
        "content-type": "application/json; charset=utf-8",
      },
    });
  });

  api.notFound((c) => {
    return jsonError(c, 404, `unknown api route: ${c.req.path}`);
  });

  return api;
}
