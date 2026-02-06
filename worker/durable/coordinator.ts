import { DurableObject } from "cloudflare:workers";
import { ACTIVE_JOB_KEY, COORDINATOR_OBJECT_NAME } from "../constants";
import type { WorkerEnv } from "../env";
import { jobKey, tapeKey } from "../keys";
import type {
  CreateJobResult,
  ProofJobRecord,
  ProofResultSummary,
  ProverJobStatus,
  PublicProofJob,
  ProofTapeInfo,
} from "../types";
import { isTerminalProofStatus, nowIso } from "../utils";

export function coordinatorStub(env: WorkerEnv): DurableObjectStub<ProofCoordinatorDO> {
  const id = env.PROOF_COORDINATOR.idFromName(COORDINATOR_OBJECT_NAME);
  return env.PROOF_COORDINATOR.get(id);
}

export function asPublicJob(job: ProofJobRecord): PublicProofJob {
  return {
    jobId: job.jobId,
    status: job.status,
    createdAt: job.createdAt,
    updatedAt: job.updatedAt,
    completedAt: job.completedAt,
    tape: {
      sizeBytes: job.tape.sizeBytes,
      metadata: job.tape.metadata,
    },
    queue: job.queue,
    prover: job.prover,
    result: job.result,
    error: job.error,
  };
}

export class ProofCoordinatorDO extends DurableObject<WorkerEnv> {
  private async getActiveJobId(): Promise<string | null> {
    return (await this.ctx.storage.get<string>(ACTIVE_JOB_KEY)) ?? null;
  }

  private async loadJob(jobId: string): Promise<ProofJobRecord | null> {
    return (await this.ctx.storage.get<ProofJobRecord>(jobKey(jobId))) ?? null;
  }

  private async saveJob(job: ProofJobRecord): Promise<void> {
    await this.ctx.storage.put(jobKey(job.jobId), job);
  }

  private async releaseActiveIfMatches(jobId: string): Promise<void> {
    const activeJobId = await this.getActiveJobId();
    if (activeJobId === jobId) {
      await this.ctx.storage.delete(ACTIVE_JOB_KEY);
    }
  }

  async createJob(tapeInfo: Omit<ProofTapeInfo, "key">): Promise<CreateJobResult> {
    const activeJobId = await this.getActiveJobId();
    if (activeJobId) {
      const activeJob = await this.loadJob(activeJobId);
      if (activeJob && !isTerminalProofStatus(activeJob.status)) {
        return {
          accepted: false,
          message: "another proof job is already active",
          activeJob,
        };
      }
      await this.ctx.storage.delete(ACTIVE_JOB_KEY);
    }

    const jobId = crypto.randomUUID();
    const now = nowIso();

    const job: ProofJobRecord = {
      jobId,
      status: "queued",
      createdAt: now,
      updatedAt: now,
      completedAt: null,
      tape: {
        ...tapeInfo,
        key: tapeKey(jobId),
      },
      queue: {
        attempts: 0,
        lastAttemptAt: null,
        lastError: null,
        nextRetryAt: null,
      },
      prover: {
        jobId: null,
        status: null,
        statusUrl: null,
        lastPolledAt: null,
      },
      result: null,
      error: null,
    };

    await this.saveJob(job);
    await this.ctx.storage.put(ACTIVE_JOB_KEY, jobId);

    return {
      accepted: true,
      job,
    };
  }

  async getJob(jobId: string): Promise<ProofJobRecord | null> {
    return this.loadJob(jobId);
  }

  async getActiveJob(): Promise<ProofJobRecord | null> {
    const activeJobId = await this.getActiveJobId();
    if (!activeJobId) {
      return null;
    }

    return this.loadJob(activeJobId);
  }

  async beginQueueAttempt(jobId: string, attempts: number): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return job;
    }

    const now = nowIso();
    job.status = job.prover.jobId ? "prover_running" : "dispatching";
    job.updatedAt = now;
    job.queue.attempts = Math.max(job.queue.attempts, attempts);
    job.queue.lastAttemptAt = now;
    job.queue.nextRetryAt = null;
    await this.saveJob(job);
    return job;
  }

  async markRetry(
    jobId: string,
    reason: string,
    nextRetryAt: string,
  ): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return job;
    }

    job.status = "retrying";
    job.updatedAt = nowIso();
    job.queue.lastError = reason;
    job.queue.nextRetryAt = nextRetryAt;
    await this.saveJob(job);
    return job;
  }

  async markProverAccepted(
    jobId: string,
    proverJobId: string,
    statusUrl: string,
  ): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return job;
    }

    job.status = "prover_running";
    job.updatedAt = nowIso();
    job.queue.lastError = null;
    job.queue.nextRetryAt = null;
    job.prover.jobId = proverJobId;
    job.prover.status = "queued";
    job.prover.statusUrl = statusUrl;
    await this.saveJob(job);
    return job;
  }

  async markProverPolled(jobId: string, status: ProverJobStatus): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return job;
    }

    job.status = "prover_running";
    job.updatedAt = nowIso();
    job.queue.lastError = null;
    job.queue.nextRetryAt = null;
    job.prover.status = status;
    job.prover.lastPolledAt = nowIso();
    await this.saveJob(job);
    return job;
  }

  async markSucceeded(
    jobId: string,
    summary: ProofResultSummary,
    artifactKey: string,
  ): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job) {
      return null;
    }

    const now = nowIso();
    job.status = "succeeded";
    job.updatedAt = now;
    job.completedAt = now;
    job.queue.lastError = null;
    job.queue.nextRetryAt = null;
    job.prover.status = "succeeded";
    job.prover.lastPolledAt = now;
    job.result = {
      artifactKey,
      summary,
    };
    job.error = null;

    await this.saveJob(job);
    await this.releaseActiveIfMatches(jobId);
    return job;
  }

  async markFailed(jobId: string, reason: string): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job) {
      return null;
    }

    const now = nowIso();
    job.status = "failed";
    job.updatedAt = now;
    job.completedAt = now;
    job.error = reason;
    job.queue.lastError = reason;
    job.queue.nextRetryAt = null;
    if (job.prover.status !== "succeeded") {
      job.prover.status = "failed";
      job.prover.lastPolledAt = now;
    }

    await this.saveJob(job);
    await this.releaseActiveIfMatches(jobId);
    return job;
  }
}
