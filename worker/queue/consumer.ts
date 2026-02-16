import { DEFAULT_MAX_JOB_WALL_TIME_MS, MAX_QUEUE_RETRIES } from "../constants";
import { submitClaim } from "../claim/submit";
import { coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { submitToProver } from "../prover/client";
import type { ClaimQueueMessage, ProofQueueMessage, ProofJournal } from "../types";
import { isTerminalProofStatus, parseInteger, retryDelaySeconds, safeErrorMessage } from "../utils";

function journalRawHex(journal: ProofJournal): string {
  const buf = new Uint8Array(24);
  const view = new DataView(buf.buffer);
  view.setUint32(0, journal.seed >>> 0, true);
  view.setUint32(4, journal.frame_count >>> 0, true);
  view.setUint32(8, journal.final_score >>> 0, true);
  view.setUint32(12, journal.final_rng_state >>> 0, true);
  view.setUint32(16, journal.tape_checksum >>> 0, true);
  view.setUint32(20, journal.rules_digest >>> 0, true);
  return Array.from(buf)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function sha256HexFromHex(hex: string): Promise<string> {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(digest)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function processQueueMessage(
  message: Message<ProofQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  const payload = message.body;
  if (!payload || typeof payload.jobId !== "string" || payload.jobId.length === 0) {
    message.ack();
    return;
  }

  const jobId = payload.jobId;
  const maxWallTimeMs = parseInteger(
    env.MAX_JOB_WALL_TIME_MS,
    DEFAULT_MAX_JOB_WALL_TIME_MS,
    60_000,
  );

  const coordinator = coordinatorStub(env);
  const startedJob = await coordinator.beginQueueAttempt(jobId, message.attempts);
  if (!startedJob || isTerminalProofStatus(startedJob.status)) {
    message.ack();
    return;
  }

  if (startedJob.tape.metadata.finalScore >>> 0 === 0) {
    await coordinator.markFailed(jobId, "zero-score runs are not accepted");
    message.ack();
    return;
  }

  // If the prover job already exists (re-delivered message after crash),
  // beginQueueAttempt ensured the alarm is running. Just ack.
  if (startedJob.prover.jobId) {
    message.ack();
    return;
  }

  const jobAgeMs = Date.now() - new Date(startedJob.createdAt).getTime();
  if (jobAgeMs > maxWallTimeMs) {
    const ageMin = Math.round(jobAgeMs / 60_000);
    await coordinator.markFailed(
      jobId,
      `proof job timed out after ${ageMin} minutes (attempt ${message.attempts})`,
    );
    message.ack();
    return;
  }

  const tapeObject = await env.PROOF_ARTIFACTS.get(startedJob.tape.key);
  if (!tapeObject) {
    await coordinator.markFailed(jobId, "missing tape artifact in R2");
    message.ack();
    return;
  }

  const tapeBytes = new Uint8Array(await tapeObject.arrayBuffer());
  let submitResult: Awaited<ReturnType<typeof submitToProver>>;
  try {
    submitResult = await submitToProver(env, tapeBytes, {});
  } catch (error) {
    const reason = `submit error: ${safeErrorMessage(error)}`;
    if (message.attempts >= MAX_QUEUE_RETRIES) {
      await coordinator.markFailed(
        jobId,
        `${reason} (exhausted ${message.attempts} delivery attempts)`,
      );
      message.ack();
      return;
    }

    const delaySeconds = retryDelaySeconds(message.attempts);
    await coordinator.markRetry(
      jobId,
      reason,
      new Date(Date.now() + delaySeconds * 1000).toISOString(),
    );
    message.retry({ delaySeconds });
    return;
  }

  if (submitResult.type === "retry") {
    if (message.attempts >= MAX_QUEUE_RETRIES) {
      await coordinator.markFailed(
        jobId,
        `${submitResult.message} (exhausted ${message.attempts} delivery attempts)`,
      );
      message.ack();
      return;
    }

    const delaySeconds = retryDelaySeconds(message.attempts);
    const nextRetryAt = new Date(Date.now() + delaySeconds * 1000).toISOString();
    await coordinator.markRetry(jobId, submitResult.message, nextRetryAt);
    message.retry({ delaySeconds });
    return;
  }

  if (submitResult.type === "fatal") {
    await coordinator.markFailed(jobId, submitResult.message);
    message.ack();
    return;
  }

  // Submission succeeded — markProverAccepted sets the first alarm for polling.
  await coordinator.markProverAccepted(
    jobId,
    submitResult.jobId,
    submitResult.statusUrl,
    submitResult.segmentLimitPo2,
  );
  message.ack();
}

async function processClaimQueueMessage(
  message: Message<ClaimQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  const payload = message.body;
  if (!payload || typeof payload.jobId !== "string" || payload.jobId.length === 0) {
    message.ack();
    return;
  }

  const coordinator = coordinatorStub(env);
  const job = await coordinator.beginClaimAttempt(payload.jobId, message.attempts);
  if (!job) {
    message.ack();
    return;
  }

  if (job.status !== "succeeded") {
    message.ack();
    return;
  }

  if (job.claim.status === "succeeded") {
    message.ack();
    return;
  }

  if (!job.result?.summary || !job.result?.artifactKey) {
    await coordinator.markClaimFailed(payload.jobId, "missing proof result for claim submission");
    message.ack();
    return;
  }

  const artifact = await env.PROOF_ARTIFACTS.get(job.result.artifactKey);
  if (!artifact) {
    await coordinator.markClaimFailed(payload.jobId, "missing proof artifact in R2");
    message.ack();
    return;
  }

  let artifactJson: { prover_response?: unknown };
  try {
    artifactJson = (await artifact.json()) as { prover_response?: unknown };
  } catch (error) {
    await coordinator.markClaimFailed(
      payload.jobId,
      `failed parsing proof artifact json: ${safeErrorMessage(error)}`,
    );
    message.ack();
    return;
  }

  const journalHex = journalRawHex(job.result.summary.journal);
  const digestHex = await sha256HexFromHex(journalHex);

  let relayResult: Awaited<ReturnType<typeof submitClaim>>;
  try {
    relayResult = await submitClaim(env, {
      jobId: payload.jobId,
      claimantAddress: job.claim.claimantAddress,
      journalRawHex: journalHex,
      journalDigestHex: digestHex,
      proverResponse: artifactJson.prover_response ?? null,
    });
  } catch (error) {
    const reason = `claim submit error: ${safeErrorMessage(error)}`;
    if (message.attempts >= MAX_QUEUE_RETRIES) {
      await coordinator.markClaimFailed(
        payload.jobId,
        `${reason} (exhausted ${message.attempts} delivery attempts)`,
      );
      message.ack();
      return;
    }

    const delaySeconds = retryDelaySeconds(message.attempts);
    await coordinator.markClaimRetry(
      payload.jobId,
      reason,
      new Date(Date.now() + delaySeconds * 1000).toISOString(),
    );
    message.retry({ delaySeconds });
    return;
  }

  if (relayResult.type === "success") {
    console.log("[claim-queue] claim result", {
      jobId: payload.jobId,
      type: relayResult.type,
      txHash: relayResult.txHash,
    });
    await coordinator.markClaimSucceeded(payload.jobId, relayResult.txHash);
    message.ack();
    return;
  }

  if (relayResult.type === "retry") {
    console.log("[claim-queue] claim result", {
      jobId: payload.jobId,
      type: relayResult.type,
      message: relayResult.message,
      attempts: message.attempts,
    });
    if (message.attempts >= MAX_QUEUE_RETRIES) {
      await coordinator.markClaimFailed(
        payload.jobId,
        `${relayResult.message} (exhausted ${message.attempts} delivery attempts)`,
      );
      message.ack();
      return;
    }

    const delaySeconds = retryDelaySeconds(message.attempts);
    await coordinator.markClaimRetry(
      payload.jobId,
      relayResult.message,
      new Date(Date.now() + delaySeconds * 1000).toISOString(),
    );
    message.retry({ delaySeconds });
    return;
  }

  console.log("[claim-queue] claim result", {
    jobId: payload.jobId,
    type: relayResult.type,
    message: relayResult.message,
  });
  await coordinator.markClaimFailed(payload.jobId, relayResult.message);
  message.ack();
}

export async function handleQueueBatch(
  batch: MessageBatch<ProofQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  // Processing one message at a time is intentional. Each message corresponds
  // to the single active proof slot and must avoid concurrent dispatch/polling.
  /* eslint-disable no-await-in-loop */
  for (const message of batch.messages) {
    await processQueueMessage(message, env);
  }
  /* eslint-enable no-await-in-loop */
}

/**
 * Handles messages that land in the dead-letter queue after all retries are
 * exhausted. This is a safety net — the primary consumer already detects last
 * attempts and marks jobs failed. The DLQ catches edge cases like unhandled
 * consumer crashes on the final delivery.
 */
export async function handleDlqBatch(
  batch: MessageBatch<ProofQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  // Sequential processing is intentional — each message must finish before the next.
  /* eslint-disable no-await-in-loop */
  for (const message of batch.messages) {
    const payload = message.body;
    if (!payload || typeof payload.jobId !== "string" || payload.jobId.length === 0) {
      message.ack();
      continue;
    }

    const coordinator = coordinatorStub(env);
    const job = await coordinator.getJob(payload.jobId);

    if (job && !isTerminalProofStatus(job.status)) {
      await coordinator.markFailed(
        payload.jobId,
        "proof job failed: all queue delivery attempts exhausted (dead-letter)",
      );
    }

    message.ack();
  }
  /* eslint-enable no-await-in-loop */
}

export async function handleClaimQueueBatch(
  batch: MessageBatch<ClaimQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  /* eslint-disable no-await-in-loop */
  for (const message of batch.messages) {
    try {
      await processClaimQueueMessage(message, env);
    } catch (error) {
      const payload = message.body;
      if (!payload || typeof payload.jobId !== "string" || payload.jobId.length === 0) {
        message.ack();
        continue;
      }

      const reason = `claim queue consumer crashed: ${safeErrorMessage(error)}`;
      const coordinator = coordinatorStub(env);
      if (message.attempts >= MAX_QUEUE_RETRIES) {
        await coordinator.markClaimFailed(
          payload.jobId,
          `${reason} (exhausted ${message.attempts} delivery attempts)`,
        );
        message.ack();
      } else {
        const delaySeconds = retryDelaySeconds(message.attempts);
        await coordinator.markClaimRetry(
          payload.jobId,
          reason,
          new Date(Date.now() + delaySeconds * 1000).toISOString(),
        );
        message.retry({ delaySeconds });
      }
    }
  }
  /* eslint-enable no-await-in-loop */
}

export async function handleClaimDlqBatch(
  batch: MessageBatch<ClaimQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  /* eslint-disable no-await-in-loop */
  for (const message of batch.messages) {
    const payload = message.body;
    if (!payload || typeof payload.jobId !== "string" || payload.jobId.length === 0) {
      message.ack();
      continue;
    }

    const coordinator = coordinatorStub(env);
    const job = await coordinator.getJob(payload.jobId);
    const priorError = job?.claim.lastError?.trim();
    const dlqMessage =
      priorError && priorError.length > 0
        ? `${priorError} (dead-letter)`
        : "claim submission failed: all queue delivery attempts exhausted (dead-letter)";
    await coordinator.markClaimFailed(payload.jobId, dlqMessage);
    message.ack();
  }
  /* eslint-enable no-await-in-loop */
}
