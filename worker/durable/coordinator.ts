import { DurableObject } from "cloudflare:workers";
import {
  ACTIVE_JOB_KEY,
  COORDINATOR_OBJECT_NAME,
  DEFAULT_COMPLETED_JOB_RETENTION_MS,
  DEFAULT_MAX_JOB_WALL_TIME_MS,
  DEFAULT_MAX_COMPLETED_JOBS,
  DEFAULT_POLL_INTERVAL_MS,
  DEFAULT_SEGMENT_LIMIT_PO2,
  JOB_KEY_PREFIX,
  LEADERBOARD_EVENT_KEY_PREFIX,
  LEADERBOARD_INGESTION_STATE_KEY,
  MAX_PROVER_RECOVERY_ATTEMPTS,
  PROFILE_KEY_PREFIX,
} from "../constants";
import type { WorkerEnv } from "../env";
import { jobKey, resultKey, tapeKey } from "../keys";
import { pollProver, pollProverOnce, submitToProver, summarizeProof } from "../prover/client";
import type {
  CreateJobResult,
  LeaderboardEventRecord,
  LeaderboardIngestionState,
  PlayerProfileRecord,
  ProofJobRecord,
  ProofResultSummary,
  ProverPollResult,
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
    claim: job.claim,
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

  private profileKey(claimantAddress: string): string {
    return `${PROFILE_KEY_PREFIX}${claimantAddress}`;
  }

  private leaderboardEventKey(eventId: string): string {
    return `${LEADERBOARD_EVENT_KEY_PREFIX}${eventId}`;
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

  private segmentFallbackForOom(job: ProofJobRecord, reason: string): number | null {
    const currentSegment =
      typeof job.prover.segmentLimitPo2 === "number"
        ? job.prover.segmentLimitPo2
        : DEFAULT_SEGMENT_LIMIT_PO2;
    const fallbackSegment = DEFAULT_SEGMENT_LIMIT_PO2;
    const normalizedReason = reason.toLowerCase();
    const looksLikeOom =
      normalizedReason.includes("out of memory") || normalizedReason.includes("allocation failed");

    if (!looksLikeOom || currentSegment <= fallbackSegment) {
      return null;
    }

    return fallbackSegment;
  }

  async createJob(
    tapeInfo: Omit<ProofTapeInfo, "key"> & { claimantAddress: string },
  ): Promise<CreateJobResult> {
    const { claimantAddress, ...proofTape } = tapeInfo;
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
        ...proofTape,
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
        segmentLimitPo2: null,
        lastPolledAt: null,
        pollingErrors: 0,
        recoveryAttempts: 0,
      },
      result: null,
      claim: {
        claimantAddress,
        status: "queued",
        attempts: 0,
        lastAttemptAt: null,
        lastError: null,
        nextRetryAt: null,
        submittedAt: null,
        txHash: null,
      },
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

  async listSucceededJobs(): Promise<ProofJobRecord[]> {
    const listPageSize = 128;
    const jobs: ProofJobRecord[] = [];
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

      for (const [, value] of page) {
        if (value?.status === "succeeded" && value.result?.summary) {
          jobs.push(value);
        }
      }

      const pageKeys = Array.from(page.keys());
      const lastKey = pageKeys[pageKeys.length - 1];
      if (!lastKey || page.size < listPageSize) {
        break;
      }
      startAfter = lastKey;
    }
    /* eslint-enable no-await-in-loop */

    return jobs;
  }

  async getProfile(claimantAddress: string): Promise<PlayerProfileRecord | null> {
    return (
      (await this.ctx.storage.get<PlayerProfileRecord>(this.profileKey(claimantAddress))) ?? null
    );
  }

  async getProfiles(claimantAddresses: string[]): Promise<Record<string, PlayerProfileRecord>> {
    const unique = Array.from(
      new Set(claimantAddresses.filter((value) => value.trim().length > 0)),
    );
    const entries = await Promise.all(
      unique.map(async (address) => [address, await this.getProfile(address)] as const),
    );

    const out: Record<string, PlayerProfileRecord> = {};
    for (const [address, profile] of entries) {
      if (profile) {
        out[address] = profile;
      }
    }
    return out;
  }

  async upsertProfile(
    claimantAddress: string,
    updates: { username: string | null; linkUrl: string | null },
  ): Promise<PlayerProfileRecord> {
    const profile: PlayerProfileRecord = {
      claimantAddress,
      username: updates.username,
      linkUrl: updates.linkUrl,
      updatedAt: nowIso(),
    };

    await this.ctx.storage.put(this.profileKey(claimantAddress), profile);
    return profile;
  }

  async listLeaderboardEvents(): Promise<LeaderboardEventRecord[]> {
    const listPageSize = 256;
    const events: LeaderboardEventRecord[] = [];
    let startAfter: string | undefined;

    /* eslint-disable no-await-in-loop */
    while (true) {
      const page = await this.ctx.storage.list<LeaderboardEventRecord>({
        prefix: LEADERBOARD_EVENT_KEY_PREFIX,
        startAfter,
        limit: listPageSize,
      });
      if (page.size === 0) {
        break;
      }

      for (const [, value] of page) {
        if (value?.eventId) {
          events.push(value);
        }
      }

      const pageKeys = Array.from(page.keys());
      const lastKey = pageKeys[pageKeys.length - 1];
      if (!lastKey || page.size < listPageSize) {
        break;
      }
      startAfter = lastKey;
    }
    /* eslint-enable no-await-in-loop */

    return events;
  }

  async upsertLeaderboardEvents(
    events: LeaderboardEventRecord[],
  ): Promise<{ inserted: number; updated: number }> {
    let inserted = 0;
    let updated = 0;

    /* eslint-disable no-await-in-loop */
    for (const event of events) {
      const key = this.leaderboardEventKey(event.eventId);
      const existing = await this.ctx.storage.get<LeaderboardEventRecord>(key);
      if (!existing) {
        inserted += 1;
      } else if (JSON.stringify(existing) !== JSON.stringify(event)) {
        updated += 1;
      } else {
        continue;
      }

      await this.ctx.storage.put(key, event);
    }
    /* eslint-enable no-await-in-loop */

    return { inserted, updated };
  }

  async getLeaderboardIngestionState(): Promise<LeaderboardIngestionState> {
    const current =
      (await this.ctx.storage.get<LeaderboardIngestionState>(LEADERBOARD_INGESTION_STATE_KEY)) ??
      null;
    if (current) {
      return {
        provider: current.provider === "rpc" ? "rpc" : "galexie",
        sourceMode:
          current.sourceMode === "rpc" ||
          current.sourceMode === "events_api" ||
          current.sourceMode === "datalake"
            ? current.sourceMode
            : current.provider === "rpc"
              ? "rpc"
              : "datalake",
        cursor: current.cursor ?? null,
        highestLedger: current.highestLedger ?? null,
        lastSyncedAt: current.lastSyncedAt ?? null,
        lastBackfillAt: current.lastBackfillAt ?? null,
        totalEvents: current.totalEvents ?? 0,
        lastError: current.lastError ?? null,
      };
    }

    return {
      provider: "galexie",
      sourceMode: "datalake",
      cursor: null,
      highestLedger: null,
      lastSyncedAt: null,
      lastBackfillAt: null,
      totalEvents: 0,
      lastError: null,
    };
  }

  async setLeaderboardIngestionState(state: LeaderboardIngestionState): Promise<void> {
    await this.ctx.storage.put(LEADERBOARD_INGESTION_STATE_KEY, state);
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
      job.prover.segmentLimitPo2 = null;
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
    segmentLimitPo2: number,
    recoveryAttempts?: number,
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
    job.prover.segmentLimitPo2 = segmentLimitPo2;
    job.prover.pollingErrors = 0;
    job.prover.recoveryAttempts = recoveryAttempts ?? job.prover.recoveryAttempts;
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
    job.claim.status = "queued";
    job.claim.lastError = null;
    job.claim.nextRetryAt = null;

    await this.saveJob(job);
    await this.releaseActiveIfMatches(jobId);
    await this.enqueueClaimJob(jobId);
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
    if (job.claim.status !== "succeeded") {
      job.claim.status = "failed";
      job.claim.lastError = `proof failed before on-chain claim: ${reason}`;
      job.claim.nextRetryAt = null;
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

  private async enqueueClaimJob(jobId: string): Promise<void> {
    const job = await this.loadJob(jobId);
    if (!job || !job.result) {
      return;
    }
    if (job.claim.status === "succeeded") {
      return;
    }

    try {
      await this.env.CLAIM_QUEUE.send(
        { jobId },
        {
          contentType: "json",
        },
      );
      job.claim.status = "queued";
      job.claim.nextRetryAt = null;
      await this.saveJob(job);
    } catch (error) {
      job.claim.status = "failed";
      job.claim.lastError = `failed enqueueing claim job: ${safeErrorMessage(error)}`;
      job.claim.nextRetryAt = null;
      await this.saveJob(job);
    }
  }

  async beginClaimAttempt(jobId: string, attempts: number): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || job.status !== "succeeded") {
      return job;
    }
    if (job.claim.status === "succeeded") {
      return job;
    }
    if (!job.result?.summary) {
      return job;
    }

    job.claim.status = "submitting";
    job.claim.attempts = Math.max(job.claim.attempts, attempts);
    job.claim.lastAttemptAt = nowIso();
    job.claim.lastError = null;
    job.claim.nextRetryAt = null;
    job.updatedAt = nowIso();
    await this.saveJob(job);
    return job;
  }

  async markClaimRetry(
    jobId: string,
    reason: string,
    nextRetryAt: string,
  ): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || job.status !== "succeeded") {
      return job;
    }
    if (job.claim.status === "succeeded") {
      return job;
    }

    job.claim.status = "retrying";
    job.claim.lastError = reason;
    job.claim.nextRetryAt = nextRetryAt;
    job.updatedAt = nowIso();
    await this.saveJob(job);
    return job;
  }

  async markClaimSucceeded(jobId: string, txHash: string): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || job.status !== "succeeded") {
      return job;
    }
    if (job.claim.status === "succeeded") {
      return job;
    }

    job.claim.status = "succeeded";
    job.claim.submittedAt = nowIso();
    job.claim.txHash = txHash;
    job.claim.lastError = null;
    job.claim.nextRetryAt = null;
    job.updatedAt = nowIso();
    await this.saveJob(job);
    return job;
  }

  async markClaimFailed(jobId: string, reason: string): Promise<ProofJobRecord | null> {
    const job = await this.loadJob(jobId);
    if (!job || job.status !== "succeeded") {
      return job;
    }
    if (job.claim.status === "succeeded") {
      return job;
    }

    job.claim.status = "failed";
    job.claim.lastError = reason;
    job.claim.nextRetryAt = null;
    job.updatedAt = nowIso();
    await this.saveJob(job);
    return job;
  }

  /**
   * Shared poll-result state machine. Both alarm() and kickAlarm() delegate
   * here after obtaining a ProverPollResult.
   *
   * @param scheduleNext  true from alarm() (schedules next alarm on "running"
   *                      and does backoff retries); false from kickAlarm()
   *                      (just writes the state update, no alarm scheduling).
   */
  private async applyPollResult(
    activeJobId: string,
    job: ProofJobRecord,
    pollResult: ProverPollResult,
    scheduleNext: boolean,
  ): Promise<void> {
    const pollIntervalMs = parseInteger(
      this.env.PROVER_POLL_INTERVAL_MS,
      DEFAULT_POLL_INTERVAL_MS,
      500,
    );

    if (pollResult.type === "running") {
      job.prover.pollingErrors = 0;
      job.prover.status = pollResult.status;
      job.prover.lastPolledAt = nowIso();
      job.updatedAt = nowIso();
      job.queue.lastError = null;
      job.queue.nextRetryAt = null;
      await this.saveJob(job);
      if (scheduleNext) {
        await this.scheduleAlarm(pollIntervalMs);
      }
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
          JSON.stringify({ stored_at: nowIso(), prover_response: pollResult.response }, null, 2),
          {
            httpMetadata: { contentType: "application/json" },
            customMetadata: { jobId: activeJobId },
          },
        );
      } catch (error) {
        if (scheduleNext) {
          // R2 write failed — retry with backoff rather than failing the job.
          job.prover.pollingErrors += 1;
          job.status = "retrying";
          job.queue.lastError = `failed writing proof artifact to R2: ${safeErrorMessage(error)}`;
          job.updatedAt = nowIso();
          const delaySec = retryDelaySeconds(job.prover.pollingErrors);
          job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
          await this.saveJob(job);
          await this.scheduleAlarm(delaySec * 1000);
        }
        // kickAlarm path: next kick will retry.
        return;
      }

      await this.markSucceeded(activeJobId, summary, artifactStorageKey);
      return;
    }

    if (pollResult.type === "retry") {
      if (pollResult.clearProverJob) {
        if (scheduleNext) {
          // alarm path: attempt recovery re-submit
          const recoveryAttempts = job.prover.recoveryAttempts;
          if (recoveryAttempts >= MAX_PROVER_RECOVERY_ATTEMPTS) {
            await this.markFailed(
              activeJobId,
              `prover recovery exhausted after ${recoveryAttempts} attempt(s): ${pollResult.message}`,
            );
            return;
          }

          const nextRecoveryAttempts = recoveryAttempts + 1;
          const tapeObject = await this.env.PROOF_ARTIFACTS.get(job.tape.key);
          if (!tapeObject) {
            await this.markFailed(activeJobId, "missing tape artifact in R2 during re-submit");
            return;
          }

          const tapeBytes = new Uint8Array(await tapeObject.arrayBuffer());
          const fallbackSegmentPo2 = this.segmentFallbackForOom(job, pollResult.message);
          if (fallbackSegmentPo2 !== null) {
            console.warn(
              `[proof-worker] falling back segment_limit_po2 ${job.prover.segmentLimitPo2 ?? "unknown"} -> ${fallbackSegmentPo2} after OOM`,
            );
          }
          const submitResult = await submitToProver(
            this.env,
            tapeBytes,
            fallbackSegmentPo2 !== null ? { segmentLimitPo2: fallbackSegmentPo2 } : {},
          );

          if (submitResult.type === "success") {
            await this.markProverAccepted(
              activeJobId,
              submitResult.jobId,
              submitResult.statusUrl,
              submitResult.segmentLimitPo2,
              nextRecoveryAttempts,
            );
            return;
          }

          if (submitResult.type === "retry") {
            if (nextRecoveryAttempts >= MAX_PROVER_RECOVERY_ATTEMPTS) {
              await this.markFailed(
                activeJobId,
                `prover recovery exhausted after ${nextRecoveryAttempts} attempt(s): ${submitResult.message}`,
              );
              return;
            }

            job.prover.jobId = null;
            job.prover.status = null;
            job.prover.statusUrl = null;
            job.prover.segmentLimitPo2 = null;
            job.prover.lastPolledAt = null;
            job.prover.pollingErrors += 1;
            job.prover.recoveryAttempts = nextRecoveryAttempts;
            job.status = "retrying";
            job.updatedAt = nowIso();
            job.queue.lastError = submitResult.message;
            const delaySec = retryDelaySeconds(job.prover.pollingErrors);
            job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
            await this.saveJob(job);
            await this.scheduleAlarm(delaySec * 1000);
            return;
          }

          // fatal re-submit
          await this.markFailed(activeJobId, submitResult.message);
          return;
        }

        // kickAlarm path: just clear prover job, let alarm handle recovery.
        job.prover.jobId = null;
        job.prover.status = null;
        job.prover.statusUrl = null;
        job.prover.lastPolledAt = nowIso();
        job.prover.pollingErrors += 1;
        job.prover.recoveryAttempts += 1;
        job.status = "retrying";
        job.updatedAt = nowIso();
        job.queue.lastError = pollResult.message;
        await this.saveJob(job);
        return;
      }

      // Transient poll error without clearing the prover job.
      job.prover.pollingErrors += 1;
      job.prover.lastPolledAt = nowIso();
      job.updatedAt = nowIso();
      job.queue.lastError = pollResult.message;
      if (scheduleNext) {
        job.status = "retrying";
        const delaySec = retryDelaySeconds(job.prover.pollingErrors);
        job.queue.nextRetryAt = new Date(Date.now() + delaySec * 1000).toISOString();
        await this.saveJob(job);
        await this.scheduleAlarm(delaySec * 1000);
      } else {
        await this.saveJob(job);
      }
      return;
    }

    // pollResult.type === "fatal"
    await this.markFailed(activeJobId, pollResult.message);
  }

  /**
   * Lightweight single-shot poll triggered by the GET endpoint.
   * Unlike alarm(), this does NOT schedule follow-up alarms — it checks the
   * prover once and writes the state update so the frontend sees progress.
   */
  async kickAlarm(): Promise<void> {
    const activeJobId = await this.getActiveJobId();
    if (!activeJobId) {
      return;
    }

    const job = await this.loadJob(activeJobId);
    if (!job || isTerminalProofStatus(job.status)) {
      return;
    }

    const proverJobId = job.prover.jobId;
    if (!proverJobId) {
      // No prover job yet — the queue consumer handles submission.
      // Just ensure the alarm is scheduled.
      await this.scheduleAlarm(500);
      return;
    }

    let pollResult: ProverPollResult;
    try {
      pollResult = await pollProverOnce(this.env, proverJobId);
    } catch (error) {
      job.prover.pollingErrors += 1;
      job.prover.lastPolledAt = nowIso();
      job.updatedAt = nowIso();
      job.queue.lastError = `poll error: ${safeErrorMessage(error)}`;
      await this.saveJob(job);
      return;
    }

    await this.applyPollResult(activeJobId, job, pollResult, false);
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
    const jobAgeMs = Date.now() - new Date(job.createdAt).getTime();

    if (jobAgeMs > maxWallTimeMs) {
      const ageMin = Math.round(jobAgeMs / 60_000);
      await this.markFailed(activeJobId, `proof job timed out after ${ageMin} minutes`);
      return;
    }

    const proverJobId = job.prover.jobId;
    if (!proverJobId) {
      const recoveryAttempts = job.prover.recoveryAttempts;
      if (recoveryAttempts >= MAX_PROVER_RECOVERY_ATTEMPTS) {
        await this.markFailed(
          activeJobId,
          `prover recovery exhausted after ${recoveryAttempts} attempt(s): missing prover job ID`,
        );
        return;
      }

      const nextRecoveryAttempts = recoveryAttempts + 1;
      const tapeObject = await this.env.PROOF_ARTIFACTS.get(job.tape.key);
      if (!tapeObject) {
        await this.markFailed(activeJobId, "missing tape artifact in R2 during re-submit");
        return;
      }

      const tapeBytes = new Uint8Array(await tapeObject.arrayBuffer());
      const submitResult = await submitToProver(this.env, tapeBytes, {});

      if (submitResult.type === "success") {
        await this.markProverAccepted(
          activeJobId,
          submitResult.jobId,
          submitResult.statusUrl,
          submitResult.segmentLimitPo2,
          nextRecoveryAttempts,
        );
        // markProverAccepted already schedules the next alarm
        return;
      }

      if (submitResult.type === "retry") {
        if (nextRecoveryAttempts >= MAX_PROVER_RECOVERY_ATTEMPTS) {
          await this.markFailed(
            activeJobId,
            `prover recovery exhausted after ${nextRecoveryAttempts} attempt(s): ${submitResult.message}`,
          );
          return;
        }

        job.prover.pollingErrors += 1;
        job.prover.recoveryAttempts = nextRecoveryAttempts;
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

    let pollResult: ProverPollResult;
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

    await this.applyPollResult(activeJobId, job, pollResult, true);
  }
}
