use actix_web::{
    http::StatusCode,
    web::{Bytes, Data, Path, Query},
    HttpResponse, Responder,
};
use asteroids_verifier_core::constants::{RULESET_V2_NAME, RULES_DIGEST_V2};
use asteroids_verifier_core::tape::parse_tape;
use host::accelerator;
use uuid::Uuid;

use crate::jobs::enqueue_proof_job;
use crate::response::json_error_with_code;
use crate::{AppState, HealthResponse, JobStatus, ProveTapeQuery};

/// Returns `(error_message, error_code)` on failure.
pub(crate) fn validate_tape_size(
    size: usize,
    max_tape_bytes: usize,
) -> Result<(), (String, &'static str)> {
    if size == 0 {
        return Err(("tape payload is empty".to_string(), "tape_empty"));
    }
    if size > max_tape_bytes {
        return Err((
            format!("tape payload too large: {size} bytes (max {max_tape_bytes})"),
            "tape_too_large",
        ));
    }
    Ok(())
}

/// Returns `(error_message, error_code)` on failure.
pub(crate) fn validate_non_zero_score_tape(
    tape_bytes: &[u8],
    max_frames: u32,
) -> Result<(), (String, &'static str)> {
    let tape = parse_tape(tape_bytes, max_frames)
        .map_err(|err| (format!("invalid tape payload: {err}"), "invalid_tape"))?;

    if tape.footer.final_score == 0 {
        return Err((
            "final_score must be greater than zero".to_string(),
            "zero_score_not_allowed",
        ));
    }

    Ok(())
}

pub(crate) async fn health(state: Data<AppState>) -> impl Responder {
    let (queued_jobs, running_jobs, stored_jobs) = match state.jobs.count_by_status() {
        Ok(counts) => counts,
        Err(e) => {
            tracing::error!("health check failed: {e}");
            return json_error_with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "job store error",
                Some("internal_error"),
            );
        }
    };

    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "risc0-asteroids-api",
        accelerator: accelerator(),
        image_id: host::image_id_hex(),
        rules_digest: RULES_DIGEST_V2,
        rules_digest_hex: format!("0x{RULES_DIGEST_V2:08x}"),
        ruleset: RULESET_V2_NAME,
        dev_mode: host::risc0_dev_mode_enabled(),
        queued_jobs,
        running_jobs,
        stored_jobs,
        max_jobs: state.max_jobs,
        prover_concurrency: state.prover_semaphore.available_permits() + running_jobs,
        max_tape_bytes: state.max_tape_bytes,
        max_frames: state.policy.max_frames,
        min_segment_limit_po2: state.policy.min_segment_limit_po2,
        max_segment_limit_po2: state.policy.max_segment_limit_po2,
        http_workers: state.http_workers,
        http_max_connections: state.http_max_connections,
        http_keep_alive_secs: state.http_keep_alive_secs,
        timed_out_proof_kill_secs: state.timed_out_proof_kill_secs,
        auth_required: state.auth_required,
    })
}

pub(crate) async fn create_prove_job_raw(
    state: Data<AppState>,
    query: Query<ProveTapeQuery>,
    body: Bytes,
) -> impl Responder {
    if let Err((msg, code)) = validate_tape_size(body.len(), state.max_tape_bytes) {
        return json_error_with_code(StatusCode::BAD_REQUEST, msg, Some(code));
    }
    let options = match state.policy.to_options(&query) {
        Ok(options) => options,
        Err((msg, code)) => return json_error_with_code(StatusCode::BAD_REQUEST, msg, Some(code)),
    };
    if let Err((msg, code)) = validate_non_zero_score_tape(body.as_ref(), options.max_frames) {
        return json_error_with_code(StatusCode::BAD_REQUEST, msg, Some(code));
    }

    enqueue_proof_job(state, body.to_vec(), options).await
}

pub(crate) async fn get_job(state: Data<AppState>, path: Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    match state.jobs.get(job_id) {
        Ok(Some(job)) => HttpResponse::Ok().json(job),
        Ok(None) => json_error_with_code(
            StatusCode::NOT_FOUND,
            format!("job not found: {job_id}"),
            Some("job_not_found"),
        ),
        Err(e) => {
            tracing::error!(job_id = %job_id, "get_job failed: {e}");
            json_error_with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "job store error",
                Some("internal_error"),
            )
        }
    }
}

pub(crate) async fn delete_job(state: Data<AppState>, path: Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    match state.jobs.get(job_id) {
        Ok(Some(job)) => {
            if job.status == JobStatus::Queued || job.status == JobStatus::Running {
                return json_error_with_code(
                    StatusCode::CONFLICT,
                    "cannot delete an active job (status is queued or running)",
                    Some("job_active"),
                );
            }
        }
        Ok(None) => {
            return json_error_with_code(
                StatusCode::NOT_FOUND,
                format!("job not found: {job_id}"),
                Some("job_not_found"),
            )
        }
        Err(e) => {
            tracing::error!(job_id = %job_id, "delete preflight get failed: {e}");
            return json_error_with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "job store error",
                Some("internal_error"),
            );
        }
    }

    match state.jobs.delete(job_id) {
        Ok(true) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "job_id": job_id,
        })),
        Ok(false) => json_error_with_code(
            StatusCode::NOT_FOUND,
            format!("job not found: {job_id}"),
            Some("job_not_found"),
        ),
        Err(e) => {
            tracing::error!(job_id = %job_id, "delete_job failed: {e}");
            json_error_with_code(
                StatusCode::INTERNAL_SERVER_ERROR,
                "job store error",
                Some("internal_error"),
            )
        }
    }
}

pub(crate) async fn unauthorized() -> impl Responder {
    json_error_with_code(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        Some("unauthorized"),
    )
}
