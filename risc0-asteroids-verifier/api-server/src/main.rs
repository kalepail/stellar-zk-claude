mod store;

use std::{
    env,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use actix_cors::Cors;
use actix_web::{
    guard,
    http::{
        header::{HeaderMap, AUTHORIZATION},
        StatusCode,
    },
    middleware, web, App, HttpResponse, HttpServer, Responder,
};
use asteroids_verifier_core::constants::MAX_FRAMES_DEFAULT;
use host::{
    accelerator, prove_tape, ProveOptions, ReceiptKind, TapeProof, SEGMENT_LIMIT_PO2_DEFAULT,
};
use serde::{Deserialize, Serialize};
use store::{EnqueueResult, JobStore};
use tokio::sync::Semaphore;
use uuid::Uuid;

const DEFAULT_MAX_TAPE_BYTES: usize = 2 * 1024 * 1024;
const FIXED_PROVER_CONCURRENCY: usize = 1;
const DEFAULT_JOB_TTL_SECS: u64 = 24 * 60 * 60;
const DEFAULT_JOB_SWEEP_SECS: u64 = 60;
const DEFAULT_MAX_JOBS: usize = 64;
const DEFAULT_RUNNING_JOB_TIMEOUT_SECS: u64 = 30 * 60;
const DEFAULT_MIN_SEGMENT_LIMIT_PO2: u32 = 16;
const DEFAULT_MAX_SEGMENT_LIMIT_PO2: u32 = 21;
const DEFAULT_HTTP_MAX_CONNECTIONS: usize = 25_000;
const DEFAULT_HTTP_KEEP_ALIVE_SECS: u64 = 75;

#[derive(Debug, Clone, Copy)]
struct ServerPolicy {
    max_frames: u32,
    min_segment_limit_po2: u32,
    max_segment_limit_po2: u32,
    allow_dev_mode_requests: bool,
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

        // Default to false: verification happens on-chain, not server-side.
        let verify_receipt = query.verify_receipt.unwrap_or(false);

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
    jobs: Arc<JobStore>,
    prover_semaphore: Arc<Semaphore>,
    max_tape_bytes: usize,
    max_jobs: usize,
    job_ttl_secs: u64,
    running_job_timeout_secs: u64,
    policy: ServerPolicy,
    http_workers: Option<usize>,
    http_max_connections: usize,
    http_keep_alive_secs: u64,
    auth_required: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProofEnvelope {
    proof: TapeProof,
    elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
struct JobCreatedResponse {
    success: bool,
    job_id: Uuid,
    status: JobStatus,
    status_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
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
    accelerator: &'static str,
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

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    accelerator: &'static str,
    dev_mode: bool,
    queued_jobs: usize,
    running_jobs: usize,
    stored_jobs: usize,
    max_jobs: usize,
    prover_concurrency: usize,
    max_tape_bytes: usize,
    max_frames: u32,
    min_segment_limit_po2: u32,
    max_segment_limit_po2: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_workers: Option<usize>,
    http_max_connections: usize,
    http_keep_alive_secs: u64,
    auth_required: bool,
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

fn read_env_optional_usize(name: &str) -> Option<usize> {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
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
        accelerator: accelerator(),
    }
}

fn json_error(status: StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({
        "success": false,
        "error": message.into(),
    }))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let authorization = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = authorization.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

fn is_request_authorized(headers: &HeaderMap, expected_api_key: Option<&str>) -> bool {
    let Some(expected_api_key) = expected_api_key else {
        return true;
    };

    let x_api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    if x_api_key == Some(expected_api_key) {
        return true;
    }

    bearer_token(headers).is_some_and(|token| token == expected_api_key)
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
        .map_err(|err| format!("{err:#}"))?;

    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(ProofEnvelope { proof, elapsed_ms })
}

async fn run_proof_job(state: AppState, job_id: Uuid, tape: Vec<u8>, options: ProveOptions) {
    if let Err(e) = state
        .jobs
        .update_status(job_id, JobStatus::Running, Some(now_unix_s()))
    {
        tracing::error!(job_id = %job_id, "failed to mark job running: {e}");
        return;
    }

    let timeout = Duration::from_secs(state.running_job_timeout_secs);
    let prove_result = match tokio::time::timeout(timeout, run_proof(state.clone(), tape, options))
        .await
    {
        Ok(result) => result,
        Err(_) => {
            tracing::error!(job_id = %job_id, timeout_secs = state.running_job_timeout_secs, "proof generation timed out");
            Err("proof generation timed out".to_string())
        }
    };

    match prove_result {
        Ok(result) => {
            if let Err(e) = state.jobs.complete(job_id, result) {
                tracing::error!(job_id = %job_id, "failed to store proof result: {e}");
            }
        }
        Err(err) => {
            if let Err(e) = state.jobs.fail(job_id, err) {
                tracing::error!(job_id = %job_id, "failed to mark job failed: {e}");
            }
        }
    }
}

fn spawn_job_cleanup_task(state: AppState, sweep_secs: u64) {
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

async fn health(state: web::Data<AppState>) -> impl Responder {
    let (queued_jobs, running_jobs, stored_jobs) = match state.jobs.count_by_status() {
        Ok(counts) => counts,
        Err(e) => {
            tracing::error!("health check failed: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "job store error");
        }
    };

    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "risc0-asteroids-api",
        accelerator: accelerator(),
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
        auth_required: state.auth_required,
    })
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

    match state.jobs.try_enqueue(&job, state.max_jobs) {
        Ok(EnqueueResult::Inserted) => {}
        Ok(EnqueueResult::ProverBusy) => {
            return json_error(
                StatusCode::TOO_MANY_REQUESTS,
                "prover is busy (single-flight mode): retry after the active job finishes",
            );
        }
        Ok(EnqueueResult::AtCapacity(cap)) => {
            return json_error(
                StatusCode::TOO_MANY_REQUESTS,
                format!("job store is at capacity ({cap}) with no finished jobs to evict"),
            );
        }
        Err(e) => {
            tracing::error!("try_enqueue failed: {e}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "job store error");
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

async fn get_job(state: web::Data<AppState>, path: web::Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    match state.jobs.get(job_id) {
        Ok(Some(job)) => HttpResponse::Ok().json(job),
        Ok(None) => json_error(StatusCode::NOT_FOUND, format!("job not found: {job_id}")),
        Err(e) => {
            tracing::error!(job_id = %job_id, "get_job failed: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "job store error")
        }
    }
}

async fn delete_job(state: web::Data<AppState>, path: web::Path<Uuid>) -> impl Responder {
    let job_id = path.into_inner();
    match state.jobs.delete(job_id) {
        Ok(true) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "job_id": job_id,
        })),
        Ok(false) => json_error(StatusCode::NOT_FOUND, format!("job not found: {job_id}")),
        Err(e) => {
            tracing::error!(job_id = %job_id, "delete_job failed: {e}");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "job store error")
        }
    }
}

async fn unauthorized() -> impl Responder {
    json_error(StatusCode::UNAUTHORIZED, "unauthorized")
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
    let max_jobs = read_env_usize("MAX_JOBS", DEFAULT_MAX_JOBS);
    let job_ttl_secs = read_env_u64("JOB_TTL_SECS", DEFAULT_JOB_TTL_SECS);
    let job_sweep_secs = read_env_u64("JOB_SWEEP_SECS", DEFAULT_JOB_SWEEP_SECS);
    let running_job_timeout_secs =
        read_env_u64("RUNNING_JOB_TIMEOUT_SECS", DEFAULT_RUNNING_JOB_TIMEOUT_SECS);
    let http_workers = read_env_optional_usize("HTTP_WORKERS");
    let http_max_connections = read_env_usize("HTTP_MAX_CONNECTIONS", DEFAULT_HTTP_MAX_CONNECTIONS);
    let http_keep_alive_secs = read_env_u64("HTTP_KEEP_ALIVE_SECS", DEFAULT_HTTP_KEEP_ALIVE_SECS);
    let api_key = env::var("API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let auth_required = api_key.is_some();
    let policy = ServerPolicy::from_env();

    let data_dir = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
    let job_store = JobStore::open(&data_dir).expect("failed to open job store");

    tracing::info!(
        "starting risc0 asteroids api: bind_addr={} accelerator={} prover_concurrency={} max_tape_bytes={} max_jobs={} max_frames={} segment_limit_po2=[{}..={}] http_workers={:?} http_max_connections={} http_keep_alive_secs={} auth_required={} data_dir={}",
        bind_addr,
        accelerator(),
        FIXED_PROVER_CONCURRENCY,
        max_tape_bytes,
        max_jobs,
        policy.max_frames,
        policy.min_segment_limit_po2,
        policy.max_segment_limit_po2,
        http_workers,
        http_max_connections,
        http_keep_alive_secs,
        auth_required,
        data_dir.display()
    );

    let state = AppState {
        jobs: Arc::new(job_store),
        prover_semaphore: Arc::new(Semaphore::new(FIXED_PROVER_CONCURRENCY)),
        max_tape_bytes,
        max_jobs,
        job_ttl_secs,
        running_job_timeout_secs,
        policy,
        http_workers,
        http_max_connections,
        http_keep_alive_secs,
        auth_required,
    };
    spawn_job_cleanup_task(state.clone(), job_sweep_secs);

    let state_for_server = state.clone();
    let api_key_for_server = api_key.clone();
    let mut server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_any_header()
            .max_age(3600);
        let required_api_key = api_key_for_server.clone();

        App::new()
            .app_data(web::Data::new(state_for_server.clone()))
            .app_data(web::PayloadConfig::new(max_tape_bytes))
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .route("/health", web::get().to(health))
            .service(
                web::scope("/api")
                    .service(
                        web::scope("")
                            .guard(guard::fn_guard(move |ctx| {
                                is_request_authorized(
                                    ctx.head().headers(),
                                    required_api_key.as_deref(),
                                )
                            }))
                            .route("/jobs/prove-tape/raw", web::post().to(create_prove_job_raw))
                            .route("/jobs/{job_id}", web::get().to(get_job))
                            .route("/jobs/{job_id}", web::delete().to(delete_job)),
                    )
                    .route("/jobs/prove-tape/raw", web::post().to(unauthorized))
                    .route("/jobs/{job_id}", web::get().to(unauthorized))
                    .route("/jobs/{job_id}", web::delete().to(unauthorized)),
            )
    })
    .max_connections(http_max_connections)
    .keep_alive(Duration::from_secs(http_keep_alive_secs));

    if let Some(workers) = http_workers {
        server = server.workers(workers);
    }

    server.bind(bind_addr)?.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::header::{HeaderName, HeaderValue};
    use tempfile::TempDir;

    fn strict_policy() -> ServerPolicy {
        ServerPolicy {
            max_frames: MAX_FRAMES_DEFAULT,
            min_segment_limit_po2: DEFAULT_MIN_SEGMENT_LIMIT_PO2,
            max_segment_limit_po2: DEFAULT_MAX_SEGMENT_LIMIT_PO2,
            allow_dev_mode_requests: false,
        }
    }

    fn test_state() -> (AppState, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = JobStore::open(dir.path()).unwrap();
        let state = AppState {
            jobs: Arc::new(store),
            prover_semaphore: Arc::new(Semaphore::new(FIXED_PROVER_CONCURRENCY)),
            max_tape_bytes: DEFAULT_MAX_TAPE_BYTES,
            max_jobs: DEFAULT_MAX_JOBS,
            job_ttl_secs: DEFAULT_JOB_TTL_SECS,
            running_job_timeout_secs: DEFAULT_RUNNING_JOB_TIMEOUT_SECS,
            policy: strict_policy(),
            http_workers: None,
            http_max_connections: DEFAULT_HTTP_MAX_CONNECTIONS,
            http_keep_alive_secs: DEFAULT_HTTP_KEEP_ALIVE_SECS,
            auth_required: false,
        };
        (state, dir)
    }

    fn sample_options() -> ProveOptions {
        ProveOptions {
            max_frames: MAX_FRAMES_DEFAULT,
            segment_limit_po2: SEGMENT_LIMIT_PO2_DEFAULT,
            receipt_kind: ReceiptKind::default(),
            allow_dev_mode: false,
            verify_receipt: true,
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

    #[test]
    fn auth_allows_requests_when_api_key_not_configured() {
        let headers = HeaderMap::new();
        assert!(is_request_authorized(&headers, None));
    }

    #[test]
    fn auth_accepts_x_api_key_and_bearer_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("secret"),
        );
        assert!(is_request_authorized(&headers, Some("secret")));

        headers.remove("x-api-key");
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        assert!(is_request_authorized(&headers, Some("secret")));
    }

    #[test]
    fn auth_rejects_wrong_or_missing_key() {
        let mut headers = HeaderMap::new();
        assert!(!is_request_authorized(&headers, Some("secret")));

        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("wrong"),
        );
        assert!(!is_request_authorized(&headers, Some("secret")));
    }

    #[actix_web::test]
    async fn enqueue_rejects_when_active_job_exists() {
        let (app_state, _dir) = test_state();
        let state = web::Data::new(app_state);
        let existing_job_id = Uuid::new_v4();

        let existing_job = ProofJob {
            job_id: existing_job_id,
            status: JobStatus::Queued,
            created_at_unix_s: now_unix_s(),
            started_at_unix_s: None,
            finished_at_unix_s: None,
            tape_size_bytes: 1,
            options: options_summary(sample_options()),
            result: None,
            error: None,
        };
        state.jobs.insert(&existing_job).unwrap();
        state
            .jobs
            .update_status(existing_job_id, JobStatus::Running, Some(now_unix_s()))
            .unwrap();

        // The atomic try_enqueue should reject since there's an active job.
        let new_job = ProofJob {
            job_id: Uuid::new_v4(),
            status: JobStatus::Queued,
            created_at_unix_s: now_unix_s(),
            started_at_unix_s: None,
            finished_at_unix_s: None,
            tape_size_bytes: 1,
            options: options_summary(sample_options()),
            result: None,
            error: None,
        };
        assert!(matches!(
            state.jobs.try_enqueue(&new_job, DEFAULT_MAX_JOBS).unwrap(),
            store::EnqueueResult::ProverBusy
        ));

        // Also verify the HTTP handler rejects.
        let response = enqueue_proof_job(state, vec![1_u8], sample_options()).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
