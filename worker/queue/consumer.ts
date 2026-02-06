import { DEFAULT_MAX_JOB_WALL_TIME_MS } from "../constants";
import { coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { submitToProver } from "../prover/client";
import type { ProofQueueMessage } from "../types";
import { isTerminalProofStatus, parseInteger, retryDelaySeconds, safeErrorMessage } from "../utils";

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
    submitResult = await submitToProver(env, tapeBytes);
  } catch (error) {
    const delaySeconds = retryDelaySeconds(message.attempts);
    await coordinator.markRetry(
      jobId,
      `submit error: ${safeErrorMessage(error)}`,
      new Date(Date.now() + delaySeconds * 1000).toISOString(),
    );
    message.retry({ delaySeconds });
    return;
  }

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

  // Submission succeeded — markProverAccepted sets the first alarm for polling.
  await coordinator.markProverAccepted(jobId, submitResult.jobId, submitResult.statusUrl);
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
