import { Hono } from "hono";
import { DEFAULT_MAX_TAPE_BYTES } from "../constants";
import { asPublicJob, coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { parseAndValidateTape } from "../tape";
import { parseInteger, safeErrorMessage } from "../utils";

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

    return c.json({
      success: true,
      service: "stellar-zk-proof-gateway",
      mode: "single-active-job",
      active_job: activeJob ? asPublicJob(activeJob) : null,
    });
  });

  api.post("/proofs/jobs", async (c) => {
    const maxTapeBytes = parseInteger(c.env.MAX_TAPE_BYTES, DEFAULT_MAX_TAPE_BYTES, 1);
    const tapeBytes = new Uint8Array(await c.req.arrayBuffer());

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
      return jsonError(
        c,
        429,
        "proof queue is currently busy; retry when the active job completes",
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

    const coordinator = coordinatorStub(c.env);
    const job = await coordinator.getJob(jobId);
    if (!job) {
      return jsonError(c, 404, `job not found: ${jobId}`);
    }

    if (!job.result?.artifactKey) {
      return jsonError(c, 409, "proof result is not available for this job");
    }

    const artifact = await c.env.PROOF_ARTIFACTS.get(job.result.artifactKey);
    if (!artifact) {
      return jsonError(c, 404, "proof artifact missing from storage");
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
