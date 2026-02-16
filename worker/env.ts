import type { ProofCoordinatorDO } from "./durable/coordinator";
import type { ClaimQueueMessage, ProofQueueMessage } from "./types";

export interface WorkerEnv {
  ASSETS: Fetcher;
  PROOF_QUEUE: Queue<ProofQueueMessage>;
  CLAIM_QUEUE: Queue<ClaimQueueMessage>;
  PROOF_COORDINATOR: DurableObjectNamespace<ProofCoordinatorDO>;
  PROOF_ARTIFACTS: R2Bucket;
  PROVER_BASE_URL: string;
  PROVER_API_KEY?: string;
  PROVER_ACCESS_CLIENT_ID?: string;
  PROVER_ACCESS_CLIENT_SECRET?: string;
  PROVER_EXPECTED_IMAGE_ID?: string;
  PROVER_HEALTH_CACHE_MS?: string;
  PROVER_POLL_INTERVAL_MS?: string;
  PROVER_POLL_TIMEOUT_MS?: string;
  PROVER_REQUEST_TIMEOUT_MS?: string;
  PROVER_POLL_BUDGET_MS?: string;
  MAX_TAPE_BYTES?: string;
  MAX_JOB_WALL_TIME_MS?: string;
  MAX_COMPLETED_JOBS?: string;
  COMPLETED_JOB_RETENTION_MS?: string;
  ALLOW_INSECURE_PROVER_URL?: string;
  RELAYER_URL?: string;
  RELAYER_API_KEY?: string;
  RELAYER_PLUGIN_ID?: string;
  RELAYER_REQUEST_TIMEOUT_MS?: string;
  SCORE_CONTRACT_ID?: string;
}
