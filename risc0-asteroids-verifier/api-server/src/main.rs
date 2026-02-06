use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use actix_cors::Cors;
use actix_web::{http::StatusCode, middleware, web, App, HttpResponse, HttpServer, Responder};
use asteroids_verifier_core::constants::MAX_FRAMES_DEFAULT;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use host::{
    prove_tape, verify_tape_receipt, ProveOptions, ReceiptKind, TapeProof,
    SEGMENT_LIMIT_PO2_DEFAULT,
};
use risc0_zkvm::Receipt;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

const DEFAULT_MAX_TAPE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_PROVER_CONCURRENCY: usize = 1;
const DEFAULT_JOB_TTL_SECS: u64 = 24 * 60 * 60;
const DEFAULT_JOB_SWEEP_SECS: u64 = 60;
const DEFAULT_MAX_JOBS: usize = 64;
const DEFAULT_MIN_SEGMENT_LIMIT_PO2: u32 = 16;
const DEFAULT_MAX_SEGMENT_LIMIT_PO2: u32 = 22;

#[derive(Debug, Clone, Copy)]
struct ServerPolicy {
    max_frames: u32,
    min_segment_limit_po2: u32,
    max_segment_limit_po2: u32,
    allow_dev_mode_requests: bool,
    allow_unverified_receipts: bool,
}

impl ServerPolicy {
    fn from_env() -> Self {
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
            allow_dev_mode_requests: read_env_bool("ALLOW_DEV_MODE_REQUESTS", false),
            allow_unverified_receipts: read_env_bool("ALLOW_UNVERIFIED_RECEIPTS", false),
        }
    }

    fn to_options(&self, query: &ProveTapeQuery) -> Result<ProveOptions, String> {
        let max_frames = query.max_frames.unwrap_or(self.max_frames);
        if max_frames == 0 || max_frames > self.max_frames {
            return Err(format!(
                "max_frames must be between 1 and {}",
                self.max_frames
            ));
        }

        let segment_limit_po2 = query.segment_limit_po2.unwrap_or(SEGMENT_LIMIT_PO2_DEFAULT);
        if segment_limit_po2 < self.min_segment_limit_po2
            || segment_limit_po2 > self.max_segment_limit_po2
        {
            return Err(format!(
                "segment_limit_po2 must be in [{}..={}]",
                self.min_segment_limit_po2, self.max_segment_limit_po2
            ));
        }

        let allow_dev_mode = query.allow_dev_mode.unwrap_or(false);
        if allow_dev_mode && !self.allow_dev_mode_requests {
            return Err("allow_dev_mode is disabled by server policy".to_string());
        }

        let verify_receipt = query.verify_receipt.unwrap_or(true);
        if !verify_receipt && !self.allow_unverified_receipts {
            return Err("verify_receipt=false is disabled by server policy".to_string());
        }

        Ok(ProveOptions {
            max_frames,
            segment_limit_po2,
            receipt_kind: query.receipt_kind.unwrap_or_default(),
            allow_dev_mode,
            verify_receipt,
        })
    }
}

#[derive(Clone)]
struct AppState {
    jobs: Arc<RwLock<HashMap<Uuid, ProofJob>>>,
    prover_semaphore: Arc<Semaphore>,
    max_tape_bytes: usize,
    max_jobs: usize,
    job_ttl_secs: u64,
    policy: ServerPolicy,
}

#[derive(Debug, Clone, Deserialize)]
struct ProveTapeRequest {
    tape_b64: String,
    #[serde(default)]
    max_frames: Option<u32>,
    #[serde(default)]
    receipt_kind: Option<ReceiptKind>,
    #[serde(default)]
    segment_limit_po2: Option<u32>,
    #[serde(default)]
    allow_dev_mode: Option<bool>,
    #[serde(default)]
    verify_receipt: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ProveTapeQuery {
    #[serde(default)]
    max_frames: Option<u32>,
    #[serde(default)]
    receipt_kind: Option<ReceiptKind>,
    #[serde(default)]
    segment_limit_po2: Option<u32>,
    #[serde(default)]
    allow_dev_mode: Option<bool>,
    #[serde(default)]
    verify_receipt: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct ProofEnvelope {
    proof: TapeProof,
    elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
struct ProveTapeResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof: Option<ProofEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct JobCreatedResponse {
    success: bool,
    job_id: Uuid,
    status: JobStatus,
    status_url: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct ProveOptionsSummary {
    max_frames: u32,
    receipt_kind: ReceiptKind,
    segment_limit_po2: u32,
    allow_dev_mode: bool,
    verify_receipt: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ProofJob {
    job_id: Uuid,
    status: JobStatus,
    created_at_unix_s: u64,
    started_at_unix_s: Option<u64>,
    finished_at_unix_s: Option<u64>,
    tape_size_bytes: usize,
    options: ProveOptionsSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ProofEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifyReceiptRequest {
    receipt: Receipt,
}

#[derive(Debug, Serialize)]
struct VerifyReceiptResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    queued_jobs: usize,
    running_jobs: usize,
    stored_jobs: usize,
    max_jobs: usize,
    prover_concurrency: usize,
    max_tape_bytes: usize,
    max_frames: u32,
    min_segment_limit_po2: u32,
    max_segment_limit_po2: u32,
}

impl From<&ProveTapeRequest> for ProveTapeQuery {
    fn from(value: &ProveTapeRequest) -> Self {
        Self {
            max_frames: value.max_frames,
            receipt_kind: value.receipt_kind,
            segment_limit_po2: value.segment_limit_po2,
            allow_dev_mode: value.allow_dev_mode,
            verify_receipt: value.verify_receipt,
        }
    }
}

impl ProveTapeRequest {
    fn decode_tape(&self, max_tape_bytes: usize) -> Result<Vec<u8>, String> {
        let tape = BASE64_STANDARD
            .decode(self.tape_b64.trim())
            .map_err(|err| format!("invalid tape_b64: {err}"))?;
        validate_tape_size(tape.len(), max_tape_bytes)?;
        Ok(tape)
    }
}

fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn read_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn validate_tape_size(size: usize, max_tape_bytes: usize) -> Result<(), String> {
    if size == 0 {
        return Err("tape payload is empty".to_string());
    }
    if size > max_tape_bytes {
        return Err(format!(
            "tape payload too large: {size} bytes (max {max_tape_bytes})"
        ));
    }
    Ok(())
}

fn options_summary(options: ProveOptions) -> ProveOptionsSummary {
    ProveOptionsSummary {
        max_frames: options.max_frames,
        receipt_kind: options.receipt_kind,
        segment_limit_po2: options.segment_limit_po2,
        allow_dev_mode: options.allow_dev_mode,
        verify_receipt: options.verify_receipt,
    }
}

fn json_error(status: StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({
        "success": false,
        "error": message.into(),
    }))
}

async fn run_proof(
    state: AppState,
    tape: Vec<u8>,
    options: ProveOptions,
) -> Result<ProofEnvelope, String> {
    let _permit = state
        .prover_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|err| format!("failed to acquire prover semaphore: {err}"))?;

    let started = Instant::now();
    let proof = tokio::task::spawn_blocking(move || prove_tape(tape, options))
        .await
        .map_err(|err| format!("prover worker join failure: {err}"))?
        .map_err(|err| err.to_string())?;

    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(ProofEnvelope { proof, elapsed_ms })
}

async fn run_proof_job(state: AppState, job_id: Uuid, tape: Vec<u8>, options: ProveOptions) {
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = JobStatus::Running;
            job.started_at_unix_s = Some(now_unix_s());
        }
    }

    let prove_result = run_proof(state.clone(), tape, options).await;

    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.finished_at_unix_s = Some(now_unix_s());
            match prove_result {
                Ok(result) => {
                    job.status = JobStatus::Succeeded;
                    job.result = Some(result);
                    job.error = None;
                }
                Err(err) => {
                    job.status = JobStatus::Failed;
                    job.result = None;
                    job.error = Some(err);
                }
            }
        }
    }
}

fn spawn_job_cleanup_task(state: AppState, sweep_secs: u64) {
    tokio::spawn(async move {
        let sweep = Duration::from_secs(sweep_secs);
        loop {
            tokio::time::sleep(sweep).await;
            let cutoff = now_unix_s().saturating_sub(state.job_ttl_secs);

            let mut jobs = state.jobs.write().await;
            jobs.retain(|_, job| {
                if !matches!(job.status, JobStatus::Succeeded | JobStatus::Failed) {
                    return true;
                }
                let finished = job.finished_at_unix_s.unwrap_or(job.created_at_unix_s);
                finished >= cutoff
            });
        }
    });
}

async fn health(state: web::Data<AppState>) -> impl Responder {
    let jobs = state.jobs.read().await;
    let queued_jobs = jobs
        .values()
        .filter(|job| matches!(job.status, JobStatus::Queued))
        .count();
    let running_jobs = jobs
        .values()
        .filter(|job| matches!(job.status, JobStatus::Running))
        .count();

    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "risc0-asteroids-api",
        queued_jobs,
        running_jobs,
        stored_jobs: jobs.len(),
        max_jobs: state.max_jobs,
        prover_concurrency: state.prover_semaphore.available_permits() + running_jobs,
        max_tape_bytes: state.max_tape_bytes,
        max_frames: state.policy.max_frames,
        min_segment_limit_po2: state.policy.min_segment_limit_po2,
        max_segment_limit_po2: state.policy.max_segment_limit_po2,
    })
}

async fn prove_tape_sync_json(
    state: web::Data<AppState>,
    req: web::Json<ProveTapeRequest>,
) -> impl Responder {
    let tape = match req.decode_tape(state.max_tape_bytes) {
        Ok(tape) => tape,
        Err(err) => {
            return HttpResponse::BadRequest().json(ProveTapeResponse {
                success: false,
                proof: None,
                error: Some(err),
            })
        }
    };
    let options = match state.policy.to_options(&ProveTapeQuery::from(&*req)) {
        Ok(options) => options,
        Err(err) => {
            return HttpResponse::BadRequest().json(ProveTapeResponse {
                success: false,
                proof: None,
                error: Some(err),
            })
        }
    };

    match run_proof(state.get_ref().clone(), tape, options).await {
        Ok(proof) => HttpResponse::Ok().json(ProveTapeResponse {
            success: true,
            proof: Some(proof),
            error: None,
        }),
        Err(err) => HttpResponse::UnprocessableEntity().json(ProveTapeResponse {
            success: false,
            proof: None,
            error: Some(err),
        }),
    }
}

async fn prove_tape_sync_raw(
    state: web::Data<AppState>,
    query: web::Query<ProveTapeQuery>,
    body: web::Bytes,
) -> impl Responder {
    if let Err(err) = validate_tape_size(body.len(), state.max_tape_bytes) {
        return HttpResponse::BadRequest().json(ProveTapeResponse {
            success: false,
            proof: None,
            error: Some(err),
        });
    }

    let options = match state.policy.to_options(&query) {
        Ok(options) => options,
        Err(err) => {
            return HttpResponse::BadRequest().json(ProveTapeResponse {
                success: false,
                proof: None,
                error: Some(err),
            })
        }
    };

    match run_proof(state.get_ref().clone(), body.to_vec(), options).await {
        Ok(proof) => HttpResponse::Ok().json(ProveTapeResponse {
            success: true,
            proof: Some(proof),
            error: None,
        }),
        Err(err) => HttpResponse::UnprocessableEntity().json(ProveTapeResponse {
            success: false,
            proof: None,
            error: Some(err),
        }),
    }
}

async fn create_prove_job_json(
    state: web::Data<AppState>,
    req: web::Json<ProveTapeRequest>,
) -> impl Responder {
    let tape = match req.decode_tape(state.max_tape_bytes) {
        Ok(tape) => tape,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err),
    };
    let options = match state.policy.to_options(&ProveTapeQuery::from(&*req)) {
        Ok(options) => options,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err),
    };

    enqueue_proof_job(state, tape, options).await
}

async fn create_prove_job_raw(
    state: web::Data<AppState>,
    query: web::Query<ProveTapeQuery>,
    body: web::Bytes,
) -> impl Responder {
    if let Err(err) = validate_tape_size(body.len(), state.max_tape_bytes) {
        return json_error(StatusCode::BAD_REQUEST, err);
    }
    let options = match state.policy.to_options(&query) {
        Ok(options) => options,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err),
    };

    enqueue_proof_job(state, body.to_vec(), options).await
}

async fn enqueue_proof_job(
    state: web::Data<AppState>,
    tape: Vec<u8>,
    options: ProveOptions,
) -> HttpResponse {
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
    };

    {
        let mut jobs = state.jobs.write().await;
        if jobs.len() >= state.max_jobs {
            return json_error(
                StatusCode::TOO_MANY_REQUESTS,
                format!(
                    "job queue is at capacity ({}). delete finished jobs or wait for TTL cleanup",
                    state.max_jobs
                ),
            );
        }
        jobs.insert(job_id, job);
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

async fn get_job(state: web::Data<AppState>, path: web::Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    let jobs = state.jobs.read().await;
    match jobs.get(&job_id) {
        Some(job) => HttpResponse::Ok().json(job),
        None => json_error(StatusCode::NOT_FOUND, format!("job not found: {job_id}")),
    }
}

async fn delete_job(state: web::Data<AppState>, path: web::Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    let removed = state.jobs.write().await.remove(&job_id);
    if removed.is_some() {
        HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "job_id": job_id,
        }))
    } else {
        json_error(StatusCode::NOT_FOUND, format!("job not found: {job_id}"))
    }
}

async fn verify_receipt_endpoint(req: web::Json<VerifyReceiptRequest>) -> impl Responder {
    let receipt = req.into_inner().receipt;
    let verify_result = tokio::task::spawn_blocking(move || verify_tape_receipt(&receipt)).await;

    match verify_result {
        Ok(Ok(())) => HttpResponse::Ok().json(VerifyReceiptResponse {
            success: true,
            error: None,
        }),
        Ok(Err(err)) => HttpResponse::UnprocessableEntity().json(VerifyReceiptResponse {
            success: false,
            error: Some(err.to_string()),
        }),
        Err(err) => HttpResponse::InternalServerError().json(VerifyReceiptResponse {
            success: false,
            error: Some(format!("receipt verify worker failure: {err}")),
        }),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let bind_addr = env::var("API_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let max_tape_bytes = read_env_usize("MAX_TAPE_BYTES", DEFAULT_MAX_TAPE_BYTES);
    let prover_concurrency = read_env_usize("PROVER_CONCURRENCY", DEFAULT_PROVER_CONCURRENCY);
    let max_jobs = read_env_usize("MAX_JOBS", DEFAULT_MAX_JOBS);
    let job_ttl_secs = read_env_u64("JOB_TTL_SECS", DEFAULT_JOB_TTL_SECS);
    let job_sweep_secs = read_env_u64("JOB_SWEEP_SECS", DEFAULT_JOB_SWEEP_SECS);
    let json_limit = read_env_usize("JSON_LIMIT_BYTES", max_tape_bytes.saturating_mul(4));
    let policy = ServerPolicy::from_env();

    tracing::info!(
        "starting risc0 asteroids api: bind_addr={} prover_concurrency={} max_tape_bytes={} max_jobs={} max_frames={} segment_limit_po2=[{}..={}]",
        bind_addr,
        prover_concurrency,
        max_tape_bytes,
        max_jobs,
        policy.max_frames,
        policy.min_segment_limit_po2,
        policy.max_segment_limit_po2
    );

    let state = AppState {
        jobs: Arc::new(RwLock::new(HashMap::new())),
        prover_semaphore: Arc::new(Semaphore::new(prover_concurrency)),
        max_tape_bytes,
        max_jobs,
        job_ttl_secs,
        policy,
    };
    spawn_job_cleanup_task(state.clone(), job_sweep_secs);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_any_header()
            .max_age(3600);

        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::JsonConfig::default().limit(json_limit))
            .app_data(web::PayloadConfig::new(max_tape_bytes))
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .route("/health", web::get().to(health))
            .route("/api/prove-tape", web::post().to(prove_tape_sync_json))
            .route("/api/prove-tape/raw", web::post().to(prove_tape_sync_raw))
            .route(
                "/api/jobs/prove-tape",
                web::post().to(create_prove_job_json),
            )
            .route(
                "/api/jobs/prove-tape/raw",
                web::post().to(create_prove_job_raw),
            )
            .route("/api/jobs/{job_id}", web::get().to(get_job))
            .route("/api/jobs/{job_id}", web::delete().to(delete_job))
            .route(
                "/api/verify-receipt",
                web::post().to(verify_receipt_endpoint),
            )
    })
    .bind(bind_addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test as awtest, App};
    use serde_json::{json, Value};

    fn strict_policy() -> ServerPolicy {
        ServerPolicy {
            max_frames: MAX_FRAMES_DEFAULT,
            min_segment_limit_po2: DEFAULT_MIN_SEGMENT_LIMIT_PO2,
            max_segment_limit_po2: DEFAULT_MAX_SEGMENT_LIMIT_PO2,
            allow_dev_mode_requests: false,
            allow_unverified_receipts: false,
        }
    }

    fn test_state() -> AppState {
        AppState {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            prover_semaphore: Arc::new(Semaphore::new(1)),
            max_tape_bytes: DEFAULT_MAX_TAPE_BYTES,
            max_jobs: DEFAULT_MAX_JOBS,
            job_ttl_secs: DEFAULT_JOB_TTL_SECS,
            policy: strict_policy(),
        }
    }

    #[test]
    fn validate_tape_size_checks_bounds() {
        assert!(validate_tape_size(1, 10).is_ok());
        assert!(validate_tape_size(0, 10).is_err());
        assert!(validate_tape_size(11, 10).is_err());
    }

    #[test]
    fn policy_rejects_allow_dev_mode_when_disabled() {
        let policy = strict_policy();
        let err = policy
            .to_options(&ProveTapeQuery {
                allow_dev_mode: Some(true),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.contains("allow_dev_mode"));
    }

    #[test]
    fn policy_rejects_skip_verify_when_disabled() {
        let policy = strict_policy();
        let err = policy
            .to_options(&ProveTapeQuery {
                verify_receipt: Some(false),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.contains("verify_receipt=false"));
    }

    #[test]
    fn policy_rejects_out_of_range_segment_limit() {
        let policy = strict_policy();
        let err = policy
            .to_options(&ProveTapeQuery {
                segment_limit_po2: Some(DEFAULT_MAX_SEGMENT_LIMIT_PO2 + 1),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.contains("segment_limit_po2"));
    }

    #[test]
    fn policy_rejects_out_of_range_max_frames() {
        let policy = strict_policy();
        let err = policy
            .to_options(&ProveTapeQuery {
                max_frames: Some(MAX_FRAMES_DEFAULT + 1),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.contains("max_frames"));
    }

    #[actix_web::test]
    async fn prove_tape_rejects_invalid_base64_before_proving() {
        let app = awtest::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .route("/api/prove-tape", web::post().to(prove_tape_sync_json)),
        )
        .await;

        let req = awtest::TestRequest::post()
            .uri("/api/prove-tape")
            .set_json(json!({ "tape_b64": "!!!not_base64!!!" }))
            .to_request();
        let resp = awtest::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body: Value = awtest::read_body_json(resp).await;
        assert_eq!(body["success"], Value::Bool(false));
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("invalid tape_b64"));
    }
}
