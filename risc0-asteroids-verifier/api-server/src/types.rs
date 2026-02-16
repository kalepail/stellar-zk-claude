pub(crate) use host::{ProofMode, ReceiptKind, TapeProof, VerifyMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ProveTapeQuery {
    #[serde(default)]
    pub(crate) max_frames: Option<u32>,
    #[serde(default)]
    pub(crate) receipt_kind: Option<ReceiptKind>,
    #[serde(default)]
    pub(crate) segment_limit_po2: Option<u32>,
    #[serde(default)]
    pub(crate) verify_mode: Option<VerifyMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProofEnvelope {
    pub(crate) proof: TapeProof,
    pub(crate) elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobCreatedResponse {
    pub(crate) success: bool,
    pub(crate) job_id: Uuid,
    pub(crate) status: JobStatus,
    pub(crate) status_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProveOptionsSummary {
    pub(crate) max_frames: u32,
    pub(crate) receipt_kind: ReceiptKind,
    pub(crate) segment_limit_po2: u32,
    pub(crate) proof_mode: ProofMode,
    pub(crate) verify_mode: VerifyMode,
    pub(crate) accelerator: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProofJob {
    pub(crate) job_id: Uuid,
    pub(crate) status: JobStatus,
    pub(crate) created_at_unix_s: u64,
    pub(crate) started_at_unix_s: Option<u64>,
    pub(crate) finished_at_unix_s: Option<u64>,
    pub(crate) tape_size_bytes: usize,
    pub(crate) options: ProveOptionsSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<ProofEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
    pub(crate) service: &'static str,
    pub(crate) accelerator: &'static str,
    pub(crate) image_id: String,
    pub(crate) rules_digest: u32,
    pub(crate) rules_digest_hex: String,
    pub(crate) ruleset: &'static str,
    pub(crate) dev_mode: bool,
    pub(crate) queued_jobs: usize,
    pub(crate) running_jobs: usize,
    pub(crate) stored_jobs: usize,
    pub(crate) max_jobs: usize,
    pub(crate) prover_concurrency: usize,
    pub(crate) max_tape_bytes: usize,
    pub(crate) max_frames: u32,
    pub(crate) min_segment_limit_po2: u32,
    pub(crate) max_segment_limit_po2: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http_workers: Option<usize>,
    pub(crate) http_max_connections: usize,
    pub(crate) http_keep_alive_secs: u64,
    pub(crate) timed_out_proof_kill_secs: u64,
    pub(crate) auth_required: bool,
}
