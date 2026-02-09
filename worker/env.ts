import type { ProofCoordinatorDO } from "./durable/coordinator";
import type { ProofQueueMessage } from "./types";

export interface WorkerEnv {
  ASSETS: Fetcher;
  PROOF_QUEUE: Queue<ProofQueueMessage>;
  PROOF_COORDINATOR: DurableObjectNamespace<ProofCoordinatorDO>;
  PROOF_ARTIFACTS: R2Bucket;
  PROVER_BASE_URL: string;
  PROVER_API_KEY?: string;
  PROVER_ACCESS_CLIENT_ID?: string;
  PROVER_ACCESS_CLIENT_SECRET?: string;
  PROVER_RECEIPT_KIND?: string;
  PROVER_SEGMENT_LIMIT_PO2?: string;
  PROVER_MAX_FRAMES?: string;
  PROVER_VERIFY_RECEIPT?: string;
  PROVER_POLL_INTERVAL_MS?: string;
  PROVER_POLL_TIMEOUT_MS?: string;
  PROVER_REQUEST_TIMEOUT_MS?: string;
  PROVER_POLL_BUDGET_MS?: string;
  MAX_TAPE_BYTES?: string;
  MAX_JOB_WALL_TIME_MS?: string;
  MAX_COMPLETED_JOBS?: string;
  COMPLETED_JOB_RETENTION_MS?: string;
  ALLOW_INSECURE_PROVER_URL?: string;
}
