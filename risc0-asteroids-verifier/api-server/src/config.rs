use std::{env, sync::Arc};

use asteroids_verifier_core::constants::MAX_FRAMES_DEFAULT;
use host::{risc0_dev_mode_enabled, ProofMode, ProveOptions, VerifyMode, SEGMENT_LIMIT_PO2_DEFAULT};
use tokio::sync::Semaphore;

use crate::{JobStore, ProveTapeQuery};

pub(crate) const DEFAULT_MAX_TAPE_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const FIXED_PROVER_CONCURRENCY: usize = 1;
pub(crate) const DEFAULT_JOB_TTL_SECS: u64 = 24 * 60 * 60;
pub(crate) const DEFAULT_JOB_SWEEP_SECS: u64 = 60;
pub(crate) const DEFAULT_MAX_JOBS: usize = 64;
// Target: typical proofs ~5 min; accept up to 10 min before timing out.
pub(crate) const DEFAULT_RUNNING_JOB_TIMEOUT_SECS: u64 = 10 * 60;
pub(crate) const DEFAULT_MIN_SEGMENT_LIMIT_PO2: u32 = 16;
pub(crate) const DEFAULT_MAX_SEGMENT_LIMIT_PO2: u32 = 21;
pub(crate) const DEFAULT_HTTP_MAX_CONNECTIONS: usize = 25_000;
pub(crate) const DEFAULT_HTTP_KEEP_ALIVE_SECS: u64 = 75;
// After a proof timeout, give the detached task a short grace window, then abort
// so the supervisor can restart the process (prevents a permanently wedged prover).
pub(crate) const DEFAULT_TIMED_OUT_PROOF_KILL_SECS: u64 = 60;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ServerPolicy {
    pub(crate) max_frames: u32,
    pub(crate) min_segment_limit_po2: u32,
    pub(crate) max_segment_limit_po2: u32,
    pub(crate) dev_mode_enabled: bool,
}

impl ServerPolicy {
    pub(crate) fn from_env() -> Self {
        let mut min_segment_limit_po2 =
            read_env_u32("MIN_SEGMENT_LIMIT_PO2", DEFAULT_MIN_SEGMENT_LIMIT_PO2);
        let mut max_segment_limit_po2 =
            read_env_u32("MAX_SEGMENT_LIMIT_PO2", DEFAULT_MAX_SEGMENT_LIMIT_PO2);

        if min_segment_limit_po2 > max_segment_limit_po2 {
            tracing::warn!(
                "MIN_SEGMENT_LIMIT_PO2 ({}) > MAX_SEGMENT_LIMIT_PO2 ({}). Falling back to defaults.",
                min_segment_limit_po2,
                max_segment_limit_po2
            );
            min_segment_limit_po2 = DEFAULT_MIN_SEGMENT_LIMIT_PO2;
            max_segment_limit_po2 = DEFAULT_MAX_SEGMENT_LIMIT_PO2;
        }

        Self {
            max_frames: read_env_u32("MAX_FRAMES", MAX_FRAMES_DEFAULT),
            min_segment_limit_po2,
            max_segment_limit_po2,
            dev_mode_enabled: risc0_dev_mode_enabled(),
        }
    }

    /// Returns `(error_message, error_code)` on failure.
    pub(crate) fn to_options(
        &self,
        query: &ProveTapeQuery,
    ) -> Result<ProveOptions, (String, &'static str)> {
        let max_frames = query.max_frames.unwrap_or(self.max_frames);
        if max_frames == 0 || max_frames > self.max_frames {
            return Err((
                format!("max_frames must be between 1 and {}", self.max_frames),
                "invalid_max_frames",
            ));
        }

        let segment_limit_po2 = query.segment_limit_po2.unwrap_or(SEGMENT_LIMIT_PO2_DEFAULT);
        if segment_limit_po2 < self.min_segment_limit_po2
            || segment_limit_po2 > self.max_segment_limit_po2
        {
            return Err((
                format!(
                    "segment_limit_po2 must be in [{}..={}]",
                    self.min_segment_limit_po2, self.max_segment_limit_po2
                ),
                "invalid_segment_limit",
            ));
        }

        // Keep a single proving path:
        // - Local/dev: RISC0_DEV_MODE=1 forces dev receipts.
        // - Vast/prod: RISC0_DEV_MODE=0 forces secure proving.
        let proof_mode = if self.dev_mode_enabled {
            ProofMode::Dev
        } else {
            ProofMode::Secure
        };

        let verify_mode = query.verify_mode.unwrap_or(VerifyMode::Policy);

        Ok(ProveOptions {
            max_frames,
            segment_limit_po2,
            receipt_kind: query.receipt_kind.unwrap_or_default(),
            proof_mode,
            verify_mode,
        })
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) jobs: Arc<JobStore>,
    pub(crate) prover_semaphore: Arc<Semaphore>,
    pub(crate) max_tape_bytes: usize,
    pub(crate) max_jobs: usize,
    pub(crate) job_ttl_secs: u64,
    pub(crate) running_job_timeout_secs: u64,
    pub(crate) policy: ServerPolicy,
    pub(crate) http_workers: Option<usize>,
    pub(crate) http_max_connections: usize,
    pub(crate) http_keep_alive_secs: u64,
    pub(crate) timed_out_proof_kill_secs: u64,
    pub(crate) auth_required: bool,
}

pub(crate) fn read_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(crate) fn read_env_optional_usize(name: &str) -> Option<usize> {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

pub(crate) fn read_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(crate) fn read_env_u64_allow_zero(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

pub(crate) fn read_env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
