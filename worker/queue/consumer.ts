import { DEFAULT_MAX_JOB_WALL_TIME_MS, DEFAULT_POLL_INTERVAL_MS } from "../constants";
import { coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { resultKey } from "../keys";
import { pollProver, submitToProver, summarizeProof } from "../prover/client";
import type { ProofQueueMessage, ProofResultSummary } from "../types";
import {
  isTerminalProofStatus,
  nowIso,
  parseInteger,
  retryDelaySeconds,
  safeErrorMessage,
} from "../utils";

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
  const maxWallTimeMs = parseInteger(env.MAX_JOB_WALL_TIME_MS, DEFAULT_MAX_JOB_WALL_TIME_MS, 60_000);
  const pollIntervalMs = parseInteger(env.PROVER_POLL_INTERVAL_MS, DEFAULT_POLL_INTERVAL_MS, 500);

  const coordinator = coordinatorStub(env);
  const startedJob = await coordinator.beginQueueAttempt(jobId, message.attempts);
  if (!startedJob || isTerminalProofStatus(startedJob.status)) {
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

  let proverJobId = startedJob.prover.jobId;

  if (!proverJobId) {
    const tapeObject = await env.PROOF_ARTIFACTS.get(startedJob.tape.key);
    if (!tapeObject) {
      await coordinator.markFailed(jobId, "missing tape artifact in R2");
      message.ack();
      return;
    }

    const tapeBytes = new Uint8Array(await tapeObject.arrayBuffer());
    const submitResult = await submitToProver(env, tapeBytes);

    if (submitResult.type === "retry") {
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

    proverJobId = submitResult.jobId;
    await coordinator.markProverAccepted(jobId, submitResult.jobId, submitResult.statusUrl);
  }

  const pollResult = await pollProver(env, proverJobId);

  if (pollResult.type === "retry") {
    const delaySeconds = retryDelaySeconds(message.attempts);
    const nextRetryAt = new Date(Date.now() + delaySeconds * 1000).toISOString();
    await coordinator.markRetry(jobId, pollResult.message, nextRetryAt, pollResult.clearProverJob);
    message.retry({ delaySeconds });
    return;
  }

  if (pollResult.type === "running") {
    await coordinator.markProverPolled(jobId, pollResult.status);
    message.retry({ delaySeconds: Math.max(1, Math.ceil(pollIntervalMs / 1000)) });
    return;
  }

  if (pollResult.type === "fatal") {
    await coordinator.markFailed(jobId, pollResult.message);
    message.ack();
    return;
  }

  let summary: ProofResultSummary;
  try {
    summary = summarizeProof(pollResult.response);
  } catch (error) {
    await coordinator.markFailed(
      jobId,
      `invalid prover success payload: ${safeErrorMessage(error)}`,
    );
    message.ack();
    return;
  }

  const artifactStorageKey = resultKey(jobId);
  try {
    await env.PROOF_ARTIFACTS.put(
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
        httpMetadata: {
          contentType: "application/json",
        },
        customMetadata: {
          jobId,
        },
      },
    );
  } catch (error) {
    const delaySeconds = retryDelaySeconds(message.attempts);
    const nextRetryAt = new Date(Date.now() + delaySeconds * 1000).toISOString();
    await coordinator.markRetry(
      jobId,
      `failed writing proof artifact to R2: ${safeErrorMessage(error)}`,
      nextRetryAt,
    );
    message.retry({ delaySeconds });
    return;
  }

  await coordinator.markSucceeded(jobId, summary, artifactStorageKey);
  message.ack();
}

export async function handleQueueBatch(
  batch: MessageBatch<ProofQueueMessage>,
  env: WorkerEnv,
): Promise<void> {
  // Processing one message at a time is intentional. Each message corresponds
  // to the single active proof slot and must avoid concurrent dispatch/polling.
  // eslint-disable-next-line no-await-in-loop
  for (const message of batch.messages) {
    await processQueueMessage(message, env);
  }
}
