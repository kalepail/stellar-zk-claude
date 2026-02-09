export type ProofJobStatus =
  | "queued"
  | "dispatching"
  | "prover_running"
  | "retrying"
  | "succeeded"
  | "failed";

export type ProverJobStatus = "queued" | "running" | "succeeded" | "failed";

export interface ProofQueueMessage {
  jobId: string;
}

export interface TapeMetadata {
  seed: number;
  frameCount: number;
  finalScore: number;
  finalRngState: number;
  checksum: number;
}

export interface ProofTapeInfo {
  sizeBytes: number;
  key: string;
  metadata: TapeMetadata;
}

export interface ProofJournal {
  seed: number;
  frame_count: number;
  final_score: number;
  final_rng_state: number;
  tape_checksum: number;
  rules_digest: number;
}

export interface ProofStats {
  segments: number;
  total_cycles: number;
  user_cycles: number;
  paging_cycles: number;
  reserved_cycles: number;
}

export interface ProofResultSummary {
  elapsedMs: number;
  requestedReceiptKind: string;
  producedReceiptKind: string | null;
  journal: ProofJournal;
  stats: ProofStats;
}

export interface ProofResultInfo {
  artifactKey: string;
  summary: ProofResultSummary;
}

export interface QueueTracking {
  attempts: number;
  lastAttemptAt: string | null;
  lastError: string | null;
  nextRetryAt: string | null;
}

export interface ProverTracking {
  jobId: string | null;
  status: ProverJobStatus | null;
  statusUrl: string | null;
  lastPolledAt: string | null;
  pollingErrors: number;
}

export interface ProofJobRecord {
  jobId: string;
  status: ProofJobStatus;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  tape: ProofTapeInfo;
  queue: QueueTracking;
  prover: ProverTracking;
  result: ProofResultInfo | null;
  error: string | null;
}

export interface PublicProofTapeInfo {
  sizeBytes: number;
  metadata: TapeMetadata;
}

export interface PublicProofJob {
  jobId: string;
  status: ProofJobStatus;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  tape: PublicProofTapeInfo;
  queue: QueueTracking;
  prover: ProverTracking;
  result: ProofResultInfo | null;
  error: string | null;
}

export interface CreateJobAccepted {
  accepted: true;
  job: ProofJobRecord;
}

export interface CreateJobRejected {
  accepted: false;
  message: string;
  activeJob: ProofJobRecord;
}

export type CreateJobResult = CreateJobAccepted | CreateJobRejected;

export interface ProverCreateJobResponse {
  success: boolean;
  job_id: string;
  status: ProverJobStatus;
  status_url: string;
  error?: string;
}

export interface ProverHealthResponse {
  status: string;
  service: string;
  accelerator?: string;
  image_id?: string;
  rules_digest?: number;
  rules_digest_hex?: string;
  ruleset?: string;
  dev_mode?: boolean;
  auth_required?: boolean;
}

export interface ProverJobResultEnvelope {
  proof: {
    journal: ProofJournal;
    requested_receipt_kind: string;
    produced_receipt_kind?: string | null;
    stats: ProofStats;
    receipt: unknown;
  };
  elapsed_ms: number;
}

export interface ProverGetJobResponse {
  job_id: string;
  status: ProverJobStatus;
  created_at_unix_s: number;
  started_at_unix_s?: number;
  finished_at_unix_s?: number;
  tape_size_bytes: number;
  options: {
    max_frames: number;
    receipt_kind: string;
    segment_limit_po2: number;
    allow_dev_mode: boolean;
    verify_receipt: boolean;
    accelerator: string;
  };
  result?: ProverJobResultEnvelope;
  error?: string;
  error_code?: string;
}

export interface ProverErrorResponse {
  success: false;
  error: string;
  error_code?: string;
}

export type ProverSubmitResult =
  | { type: "success"; jobId: string; statusUrl: string }
  | { type: "retry"; message: string }
  | { type: "fatal"; message: string };

export type ProverPollResult =
  | { type: "running"; status: Extract<ProverJobStatus, "queued" | "running"> }
  | { type: "success"; response: ProverGetJobResponse }
  | { type: "retry"; message: string; clearProverJob?: boolean }
  | { type: "fatal"; message: string };
