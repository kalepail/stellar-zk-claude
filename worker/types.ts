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

export interface ClaimQueueMessage {
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
  segmentLimitPo2: number | null;
  lastPolledAt: string | null;
  pollingErrors: number;
  recoveryAttempts: number;
}

export type ClaimStatus = "queued" | "submitting" | "retrying" | "succeeded" | "failed";

export interface ClaimTracking {
  claimantAddress: string;
  status: ClaimStatus;
  attempts: number;
  lastAttemptAt: string | null;
  lastError: string | null;
  nextRetryAt: string | null;
  submittedAt: string | null;
  txHash: string | null;
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
  claim: ClaimTracking;
  error: string | null;
}

export type LeaderboardWindow = "10m" | "day" | "all";

export interface PlayerProfileRecord {
  claimantAddress: string;
  username: string | null;
  linkUrl: string | null;
  updatedAt: string;
}

export interface LeaderboardRunRecord {
  jobId: string;
  claimantAddress: string;
  score: number;
  mintedDelta: number;
  seed: number;
  frameCount: number | null;
  finalRngState: number | null;
  tapeChecksum: number | null;
  rulesDigest: number | null;
  completedAt: string;
  claimStatus: ClaimStatus;
  claimTxHash: string | null;
}

export interface LeaderboardRankedEntry extends LeaderboardRunRecord {
  rank: number;
}

export interface LeaderboardWindowMetadata {
  startAt: string | null;
  endAt: string | null;
}

export interface LeaderboardComputedPage {
  window: LeaderboardWindow;
  generatedAt: string;
  windowRange: LeaderboardWindowMetadata;
  totalPlayers: number;
  limit: number;
  offset: number;
  nextOffset: number | null;
  entries: LeaderboardRankedEntry[];
  me: LeaderboardRankedEntry | null;
}

export interface LeaderboardEventRecord {
  eventId: string;
  claimantAddress: string;
  seed: number;
  frameCount: number | null;
  finalScore: number;
  finalRngState: number | null;
  tapeChecksum: number | null;
  rulesDigest: number | null;
  previousBest: number;
  newBest: number;
  mintedDelta: number;
  journalDigest: string | null;
  txHash: string | null;
  eventIndex: number | null;
  ledger: number | null;
  closedAt: string;
  source: "galexie" | "rpc";
  ingestedAt: string;
}

export interface LeaderboardIngestionState {
  provider: "galexie" | "rpc";
  sourceMode: "rpc" | "events_api" | "datalake";
  cursor: string | null;
  highestLedger: number | null;
  lastSyncedAt: string | null;
  lastBackfillAt: string | null;
  totalEvents: number;
  lastError: string | null;
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
  claim: ClaimTracking;
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
  image_id?: string;
  rules_digest?: number;
  ruleset?: string;
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
    proof_mode: "secure" | "dev";
    verify_mode: "policy" | "verify";
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
  | { type: "success"; jobId: string; statusUrl: string; segmentLimitPo2: number }
  | { type: "retry"; message: string }
  | { type: "fatal"; message: string };

export type ProverPollResult =
  | { type: "running"; status: Extract<ProverJobStatus, "queued" | "running"> }
  | { type: "success"; response: ProverGetJobResponse }
  | { type: "retry"; message: string; clearProverJob?: boolean }
  | { type: "fatal"; message: string };
