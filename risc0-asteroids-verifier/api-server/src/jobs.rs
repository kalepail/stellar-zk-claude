use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use actix_web::{http::StatusCode, web, HttpResponse};
use host::{accelerator, prove_tape, ProveOptions};
use uuid::Uuid;

use crate::response::json_error_with_code;
use crate::{
    AppState, EnqueueResult, JobCreatedResponse, JobStatus, ProofEnvelope, ProofJob,
    ProveOptionsSummary,
};

pub(crate) fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn options_summary(options: ProveOptions) -> ProveOptionsSummary {
    ProveOptionsSummary {
        max_frames: options.max_frames,
        receipt_kind: options.receipt_kind,
        segment_limit_po2: options.segment_limit_po2,
        allow_dev_mode: options.allow_dev_mode,
        verify_receipt: options.verify_receipt,
        accelerator: accelerator(),
    }
}

async fn run_proof(
    tape: Vec<u8>,
    options: ProveOptions,
    started: Instant,
) -> Result<ProofEnvelope, String> {
    let proof = tokio::task::spawn_blocking(move || prove_tape(tape, options))
        .await
        .map_err(|err| format!("prover worker join failure: {err}"))?
        .map_err(|err| format!("{err:#}"))?;

    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(ProofEnvelope { proof, elapsed_ms })
}

pub(crate) async fn run_proof_job(
    state: AppState,
    job_id: Uuid,
    tape: Vec<u8>,
    options: ProveOptions,
) {
    let permit = match state.prover_semaphore.clone().acquire_owned().await {
        Ok(permit) => permit,
        Err(err) => {
            tracing::error!(job_id = %job_id, "failed to acquire prover semaphore: {err}");
            return;
        }
    };

    if let Err(err) = state
        .jobs
        .update_status(job_id, JobStatus::Running, Some(now_unix_s()))
    {
        tracing::error!(job_id = %job_id, "failed to mark job running: {err}");
        return;
    }

    let started = Instant::now();
    let timeout = Duration::from_secs(state.running_job_timeout_secs);
    let mut proof_task = Box::pin(run_proof(tape, options, started));

    let prove_result = tokio::select! {
        result = &mut proof_task => {
            drop(permit);
            result
        }
        _ = tokio::time::sleep(timeout) => {
            tracing::error!(
                job_id = %job_id,
                timeout_secs = state.running_job_timeout_secs,
                "proof generation timed out"
            );
            let mut detached_proof_task = proof_task;
            let timed_out_job_id = job_id;
            let timed_out_proof_kill_secs = state.timed_out_proof_kill_secs;
            tokio::spawn(async move {
                let _permit_guard = permit;
                if timed_out_proof_kill_secs == 0 {
                    match detached_proof_task.as_mut().await {
                        Ok(_) => tracing::warn!(
                            job_id = %timed_out_job_id,
                            "proof completed after timeout and result was discarded"
                        ),
                        Err(err) => tracing::warn!(
                            job_id = %timed_out_job_id,
                            "proof task ended after timeout with error: {err}"
                        ),
                    }
                    return;
                }

                tokio::select! {
                    result = detached_proof_task.as_mut() => {
                        match result {
                            Ok(_) => tracing::warn!(
                                job_id = %timed_out_job_id,
                                "proof completed after timeout and result was discarded"
                            ),
                            Err(err) => tracing::warn!(
                                job_id = %timed_out_job_id,
                                "proof task ended after timeout with error: {err}"
                            ),
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(timed_out_proof_kill_secs)) => {
                        tracing::error!(
                            job_id = %timed_out_job_id,
                            timeout_secs = timed_out_proof_kill_secs,
                            "timed-out proof is still running; aborting process for supervisor restart"
                        );
                        std::process::abort();
                    }
                }
            });
            Err("proof generation timed out".to_string())
        }
    };

    match prove_result {
        Ok(result) => {
            if result.proof.journal.final_score == 0 {
                if let Err(e) = state.jobs.fail(
                    job_id,
                    "prover returned final_score=0; zero-score runs are not accepted".to_string(),
                    "zero_score_not_allowed",
                ) {
                    tracing::error!(job_id = %job_id, "failed to mark job failed: {e}");
                }
                return;
            }

            if let Err(e) = state.jobs.complete(job_id, result) {
                tracing::error!(job_id = %job_id, "failed to store proof result: {e}");
                let failure = format!("failed to persist proof result: {e}");
                if let Err(mark_err) = state.jobs.fail(job_id, failure, "internal_error") {
                    tracing::error!(
                        job_id = %job_id,
                        "failed to mark job failed after persistence error: {mark_err}"
                    );
                }
            }
        }
        Err(err) => {
            let error_code = if err.contains("timed out") {
                "proof_timeout"
            } else if err.contains("prover worker join failure") {
                "internal_error"
            } else {
                "proof_error"
            };
            if let Err(e) = state.jobs.fail(job_id, err, error_code) {
                tracing::error!(job_id = %job_id, "failed to mark job failed: {e}");
            }
        }
    }
}

pub(crate) fn spawn_job_cleanup_task(state: AppState, sweep_secs: u64) {
    tokio::spawn(async move {
        let sweep = Duration::from_secs(sweep_secs);
        loop {
            tokio::time::sleep(sweep).await;
            match state
                .jobs
                .sweep(state.job_ttl_secs, state.running_job_timeout_secs)
            {
                Ok(0) => {}
                Ok(n) => tracing::info!(reaped = n, "sweep completed"),
                Err(e) => tracing::error!("sweep failed: {e}"),
            }
        }
    });
}

pub(crate) async fn enqueue_proof_job(
    state: web::Data<AppState>,
    tape: Vec<u8>,
    options: ProveOptions,
) -> HttpResponse {
    if state.prover_semaphore.available_permits() == 0 {
        return json_error_with_code(
            StatusCode::TOO_MANY_REQUESTS,
            "prover is busy: no execution slots available",
            Some("no_slots"),
        );
    }

    let job_id = Uuid::new_v4();
    let job = ProofJob {
        job_id,
        status: JobStatus::Queued,
        created_at_unix_s: now_unix_s(),
        started_at_unix_s: None,
        finished_at_unix_s: None,
        tape_size_bytes: tape.len(),
        options: options_summary(options),
        result: None,
        error: None,
        error_code: None,
    };

    match state.jobs.try_enqueue(&job, state.max_jobs) {
        Ok(EnqueueResult::Inserted) => {}
        Ok(EnqueueResult::ProverBusy) => {
            return json_error_with_code(
                StatusCode::TOO_MANY_REQUESTS,
                "prover is busy (single-flight mode): retry after the active job finishes",
                Some("prover_busy"),
            );
        }
        Ok(EnqueueResult::AtCapacity(cap)) => {
            return json_error_with_code(
                StatusCode::TOO_MANY_REQUESTS,
                format!("job store is at capacity ({cap}) with no finished jobs to evict"),
                Some("at_capacity"),
            );
        }
        Err(e) => {
            tracing::error!("try_enqueue failed: {e}");
            return json_error_with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "job store error",
                Some("internal_error"),
            );
        }
    }

    let state_for_task = state.get_ref().clone();
    tokio::spawn(async move {
        run_proof_job(state_for_task, job_id, tape, options).await;
    });

    HttpResponse::Accepted().json(JobCreatedResponse {
        success: true,
        job_id,
        status: JobStatus::Queued,
        status_url: format!("/api/jobs/{job_id}"),
    })
}
