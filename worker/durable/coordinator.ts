import { DurableObject } from "cloudflare:workers";
import {
  ACTIVE_JOB_KEY,
  COORDINATOR_OBJECT_NAME,
  DEFAULT_COMPLETED_JOB_RETENTION_MS,
  DEFAULT_MAX_JOB_WALL_TIME_MS,
  DEFAULT_MAX_COMPLETED_JOBS,
  DEFAULT_POLL_INTERVAL_MS,
  JOB_KEY_PREFIX,
} from "../constants";
import type { WorkerEnv } from "../env";
import { jobKey, resultKey, tapeKey } from "../keys";
import { pollProver, submitToProver, summarizeProof } from "../prover/client";
import type {
  CreateJobResult,
  ProofJobRecord,
  ProofResultSummary,
  PublicProofJob,
  ProofTapeInfo,
} from "../types";
import {
  isTerminalProofStatus,
  nowIso,
  parseInteger,
  retryDelaySeconds,
  safeErrorMessage,
} from "../utils";

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
  private timestampMs(value: string | null): number {
    if (!value) {
      return 0;
    }

    const parsed = new Date(value).getTime();
    return Number.isFinite(parsed) ? parsed : 0;
  }

  private async deleteArtifact(key: string | null | undefined): Promise<void> {
    if (!key) {
      return;
    }

    try {
      await this.env.PROOF_ARTIFACTS.delete(key);
    } catch (error) {
      console.warn(`[proof-worker] failed deleting artifact ${key}: ${safeErrorMessage(error)}`);
    }
  }

  private async pruneCompletedJobs(): Promise<void> {
    const maxCompletedJobs = parseInteger(
      this.env.MAX_COMPLETED_JOBS,
      DEFAULT_MAX_COMPLETED_JOBS,
      1,
    );
    const retentionMs = parseInteger(
      this.env.COMPLETED_JOB_RETENTION_MS,
      DEFAULT_COMPLETED_JOB_RETENTION_MS,
      60_000,
    );
    const nowMs = Date.now();

    const completed: Array<{
      storageKey: string;
      job: ProofJobRecord;
      terminalAtMs: number;
    }> = [];

    const listPageSize = 128;
    let startAfter: string | undefined;
    /* eslint-disable no-await-in-loop */
    while (true) {
      const page = await this.ctx.storage.list<ProofJobRecord>({
        prefix: JOB_KEY_PREFIX,
        startAfter,
        limit: listPageSize,
      });
      if (page.size === 0) {
        break;
      }

      for (const [storageKey, value] of page) {
        if (!value || !isTerminalProofStatus(value.status)) {
          continue;
        }

        completed.push({
          storageKey,
          job: value,
          terminalAtMs: Math.max(
            this.timestampMs(value.completedAt),
            this.timestampMs(value.updatedAt),
            this.timestampMs(value.createdAt),
          ),
        });
      }

      const pageKeys = Array.from(page.keys());
      const lastKey = pageKeys[pageKeys.length - 1];
      if (!lastKey || page.size < listPageSize) {
        break;
      }

      startAfter = lastKey;
    }
    /* eslint-enable no-await-in-loop */

    if (completed.length === 0) {
      return;
    }

    completed.sort((a, b) => a.terminalAtMs - b.terminalAtMs);

    const toDelete = new Set<string>();
    for (const entry of completed) {
      if (nowMs - entry.terminalAtMs > retentionMs) {
        toDelete.add(entry.storageKey);
      }
    }

    const overflow = Math.max(0, completed.length - maxCompletedJobs);
    for (let index = 0; index < overflow; index += 1) {
      toDelete.add(completed[index].storageKey);
    }

    if (toDelete.size === 0) {
      return;
    }

    /* eslint-disable no-await-in-loop */
    for (const entry of completed) {
      if (!toDelete.has(entry.storageKey)) {
        continue;
      }

      await this.ctx.storage.delete(entry.storageKey);
      await this.deleteArtifact(entry.job.tape.key);
      // result.json is intentionally kept in R2 so users can fetch proof
      // data after the DO record is pruned.  The R2 lifecycle rule
      // (expire-proof-jobs, 7 days) handles cleanup.
    }
    /* eslint-enable no-await-in-loop */
  }

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

  private async scheduleAlarm(delayMs: number): Promise<void> {
    await this.ctx.storage.setAlarm(Date.now() + delayMs);
  }

  async createJob(tapeInfo: Omit<ProofTapeInfo, "key">): Promise<CreateJobResult> {
    const activeJobId = await this.getActiveJobId();
    if (activeJobId) {
      const activeJob = await this.loadJob(activeJobId);
      if (activeJob && !isTerminalProofStatus(activeJob.status)) {
        const maxWallTimeMs = parseInteger(
          this.env.MAX_JOB_WALL_TIME_MS,
          DEFAULT_MAX_JOB_WALL_TIME_MS,
          60_000,
        );
        const jobAgeMs = Date.now() - new Date(activeJob.createdAt).getTime();
        if (jobAgeMs <= maxWallTimeMs) {
          return {
            accepted: false,
            message: "another proof job is already active",
            activeJob,
          };
        }

        // Zombie recovery: the active job has exceeded the wall-time limit
        // but was never moved to a terminal state (alarm lost, queue exhausted, etc.).
        console.warn(
          `[proof-worker] force-failing zombie job ${activeJob.jobId} (age ${Math.round(jobAgeMs / 60_000)} min)`,
        );
        await this.markFailed(
          activeJob.jobId,
          `zombie recovery: job exceeded wall-time limit (${Math.round(jobAgeMs / 60_000)} min)`,
        );
      } else {
        await this.ctx.storage.delete(ACTIVE_JOB_KEY);
      }
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
        pollingErrors: 0,
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

    // Re-delivered queue message after crash: prover job already exists,
    // ensure alarm is running so polling resumes. Consumer will just ack.
    if (job.prover.jobId) {
      const pollIntervalMs = parseInteger(
        this.env.PROVER_POLL_INTERVAL_MS,
        DEFAULT_POLL_INTERVAL_MS,
        500,
      );
      await this.scheduleAlarm(pollIntervalMs);
    }

    return job;
  }

  async markRetry(
    jobId: string,
    reason: string,
    nextRetryAt: string,
    clearProverJob?: boolean,
  ): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return job;
    }

    job.status = "retrying";
    job.updatedAt = nowIso();
    job.queue.lastError = reason;
    job.queue.nextRetryAt = nextRetryAt;
    if (clearProverJob) {
      job.prover.jobId = null;
      job.prover.status = null;
      job.prover.statusUrl = null;
      job.prover.lastPolledAt = null;
      job.prover.pollingErrors = 0;
    }
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
    job.prover.pollingErrors = 0;
    await this.saveJob(job);

    const pollIntervalMs = parseInteger(
      this.env.PROVER_POLL_INTERVAL_MS,
      DEFAULT_POLL_INTERVAL_MS,
      500,
    );
    await this.scheduleAlarm(pollIntervalMs);

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
    try {
      await this.pruneCompletedJobs();
    } catch (error) {
      console.warn(`[proof-worker] prune after success failed: ${safeErrorMessage(error)}`);
    }
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
    try {
      await this.pruneCompletedJobs();
    } catch (error) {
      console.warn(`[proof-worker] prune after failure failed: ${safeErrorMessage(error)}`);
    }
    return job;
  }

  async alarm(): Promise<void> {
    const activeJobId = await this.getActiveJobId();
    if (!activeJobId) {
      return;
    }

    const job = await this.loadJob(activeJobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return;
    }

    const maxWallTimeMs = parseInteger(
      this.env.MAX_JOB_WALL_TIME_MS,
      DEFAULT_MAX_JOB_WALL_TIME_MS,
      60_000,
    );
    const pollIntervalMs = parseInteger(
      this.env.PROVER_POLL_INTERVAL_MS,
      DEFAULT_POLL_INTERVAL_MS,
      500,
    );
    const jobAgeMs = Date.now() - new Date(job.createdAt).getTime();

    if (jobAgeMs > maxWallTimeMs) {
      const ageMin = Math.round(jobAgeMs / 60_000);
      await this.markFailed(activeJobId, `proof job timed out after ${ageMin} minutes`);
      return;
    }

    const proverJobId = job.prover.jobId;
    if (!proverJobId) {
      await this.markFailed(activeJobId, "alarm fired but no prover job ID set");
      return;
    }

    let pollResult: Awaited<ReturnType<typeof pollProver>>;
    try {
      pollResult = await pollProver(this.env, proverJobId);
    } catch (error) {
      job.prover.pollingErrors += 1;
      job.status = "retrying";
      job.updatedAt = nowIso();
      job.queue.lastError = `poll error: ${safeErrorMessage(error)}`;
      const delaySec = retryDelaySeconds(job.prover.pollingErrors);
      job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
      await this.saveJob(job);
      await this.scheduleAlarm(delaySec * 1000);
      return;
    }

    if (pollResult.type === "running") {
      job.prover.pollingErrors = 0;
      job.prover.status = pollResult.status;
      job.prover.lastPolledAt = nowIso();
      job.updatedAt = nowIso();
      job.queue.lastError = null;
      job.queue.nextRetryAt = null;
      await this.saveJob(job);
      await this.scheduleAlarm(pollIntervalMs);
      return;
    }

    if (pollResult.type === "success") {
      let summary: Awaited<ReturnType<typeof summarizeProof>>;
      try {
        summary = summarizeProof(pollResult.response);
      } catch (error) {
        await this.markFailed(
          activeJobId,
          `invalid prover success payload: ${safeErrorMessage(error)}`,
        );
        return;
      }

      const artifactStorageKey = resultKey(activeJobId);
      try {
        await this.env.PROOF_ARTIFACTS.put(
          artifactStorageKey,
          JSON.stringify(
            {
              stored_at: nowIso(),
              prover_response: pollResult.response,
            },
            null,
            2,
          ),
          {
            httpMetadata: { contentType: "application/json" },
            customMetadata: { jobId: activeJobId },
          },
        );
      } catch (error) {
        // R2 write failed — retry with backoff rather than failing the job.
        job.prover.pollingErrors += 1;
        job.status = "retrying";
        job.queue.lastError = `failed writing proof artifact to R2: ${safeErrorMessage(error)}`;
        job.updatedAt = nowIso();
        const delaySec = retryDelaySeconds(job.prover.pollingErrors);
        job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
        await this.saveJob(job);
        await this.scheduleAlarm(delaySec * 1000);
        return;
      }

      await this.markSucceeded(activeJobId, summary, artifactStorageKey);
      return;
    }

    if (pollResult.type === "retry") {
      if (pollResult.clearProverJob) {
        // Prover lost the job (e.g. restart). Re-read tape and re-submit.
        const tapeObject = await this.env.PROOF_ARTIFACTS.get(job.tape.key);
        if (!tapeObject) {
          await this.markFailed(activeJobId, "missing tape artifact in R2 during re-submit");
          return;
        }

        const tapeBytes = new Uint8Array(await tapeObject.arrayBuffer());
        const submitResult = await submitToProver(this.env, tapeBytes);

        if (submitResult.type === "success") {
          await this.markProverAccepted(activeJobId, submitResult.jobId, submitResult.statusUrl);
          // markProverAccepted already schedules the next alarm
          return;
        }

        if (submitResult.type === "retry") {
          job.prover.jobId = null;
          job.prover.status = null;
          job.prover.statusUrl = null;
          job.prover.lastPolledAt = null;
          job.prover.pollingErrors += 1;
          job.status = "retrying";
          job.updatedAt = nowIso();
          job.queue.lastError = submitResult.message;
          const delaySec = retryDelaySeconds(job.prover.pollingErrors);
          job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
          await this.saveJob(job);
          await this.scheduleAlarm(delaySec * 1000);
          return;
        }

        // fatal
        await this.markFailed(activeJobId, submitResult.message);
        return;
      }

      // Transient poll error without clearing the prover job — backoff and retry.
      job.prover.pollingErrors += 1;
      job.status = "retrying";
      job.updatedAt = nowIso();
      job.queue.lastError = pollResult.message;
      const delaySec = retryDelaySeconds(job.prover.pollingErrors);
      job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
      await this.saveJob(job);
      await this.scheduleAlarm(delaySec * 1000);
      return;
    }

    // pollResult.type === "fatal"
    await this.markFailed(activeJobId, pollResult.message);
  }
}
