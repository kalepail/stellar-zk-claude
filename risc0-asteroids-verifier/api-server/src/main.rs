mod auth;
mod config;
mod handlers;
mod jobs;
mod response;
mod store;
mod types;

use std::{env, path::PathBuf, sync::Arc, time::Duration};

use actix_cors::Cors;
use actix_web::{guard, middleware, web, App, HttpServer};
use host::accelerator;
#[cfg(test)]
use host::{ProveOptions, ReceiptKind, SEGMENT_LIMIT_PO2_DEFAULT};
use tokio::sync::Semaphore;

#[cfg(test)]
use asteroids_verifier_core::constants::MAX_FRAMES_DEFAULT;

pub(crate) use auth::{bearer_token, is_request_authorized};
pub(crate) use config::{
    read_env_optional_usize, read_env_u64, read_env_u64_allow_zero, read_env_usize, AppState,
    ServerPolicy, DEFAULT_HTTP_KEEP_ALIVE_SECS, DEFAULT_HTTP_MAX_CONNECTIONS,
    DEFAULT_JOB_SWEEP_SECS, DEFAULT_JOB_TTL_SECS, DEFAULT_MAX_JOBS, DEFAULT_MAX_SEGMENT_LIMIT_PO2,
    DEFAULT_MAX_TAPE_BYTES, DEFAULT_MIN_SEGMENT_LIMIT_PO2, DEFAULT_RUNNING_JOB_TIMEOUT_SECS,
    DEFAULT_TIMED_OUT_PROOF_KILL_SECS, FIXED_PROVER_CONCURRENCY,
};
pub(crate) use handlers::{
    create_prove_job_raw, delete_job, get_job, health, unauthorized, validate_non_zero_score_tape,
    validate_tape_size,
};
pub(crate) use jobs::{enqueue_proof_job, now_unix_s, options_summary, spawn_job_cleanup_task};
pub(crate) use store::{EnqueueResult, JobStore};
pub(crate) use types::{
    HealthResponse, JobCreatedResponse, JobStatus, ProofEnvelope, ProofJob, ProveOptionsSummary,
    ProveTapeQuery,
};

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
    let timed_out_proof_kill_secs = read_env_u64_allow_zero(
        "TIMED_OUT_PROOF_KILL_SECS",
        DEFAULT_TIMED_OUT_PROOF_KILL_SECS,
    );
    let cors_allowed_origin = env::var("CORS_ALLOWED_ORIGIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let api_key = env::var("API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let auth_required = api_key.is_some();
    let policy = ServerPolicy::from_env();

    let data_dir = PathBuf::from(env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
    let job_store = JobStore::open(&data_dir).expect("failed to open job store");

    tracing::info!(
        "starting risc0 asteroids api: bind_addr={} accelerator={} prover_concurrency={} max_tape_bytes={} max_jobs={} max_frames={} segment_limit_po2=[{}..={}] http_workers={:?} http_max_connections={} http_keep_alive_secs={} timed_out_proof_kill_secs={} cors_allowed_origin={} auth_required={} data_dir={}",
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
        timed_out_proof_kill_secs,
        cors_allowed_origin.as_deref().unwrap_or("disabled"),
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
        timed_out_proof_kill_secs,
        auth_required,
    };
    spawn_job_cleanup_task(state.clone(), job_sweep_secs);

    let state_for_server = state.clone();
    let api_key_for_server = api_key.clone();
    let cors_allowed_origin_for_server = cors_allowed_origin.clone();
    let mut server = HttpServer::new(move || {
        let cors = if let Some(origin) = cors_allowed_origin_for_server.clone() {
            Cors::default()
                .allowed_origin(&origin)
                .allowed_methods(vec!["GET", "POST", "DELETE"])
                .allow_any_header()
                .expose_any_header()
                .max_age(3600)
        } else {
            Cors::default()
        };
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
    use actix_web::http::{
        header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION},
        StatusCode,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

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
            timed_out_proof_kill_secs: DEFAULT_TIMED_OUT_PROOF_KILL_SECS,
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

        let (_, code) = validate_tape_size(0, 10).unwrap_err();
        assert_eq!(code, "tape_empty");

        let (_, code) = validate_tape_size(11, 10).unwrap_err();
        assert_eq!(code, "tape_too_large");
    }

    #[test]
    fn validate_non_zero_score_tape_rejects_zero_score() {
        let zero_score_tape =
            asteroids_verifier_core::tape::serialize_tape(0xDEAD_BEEF, &[0x00], 0, 0xAABB_CCDD);
        let (_, code) = validate_non_zero_score_tape(&zero_score_tape, MAX_FRAMES_DEFAULT)
            .expect_err("zero score should be rejected");
        assert_eq!(code, "zero_score_not_allowed");
    }

    #[test]
    fn validate_non_zero_score_tape_accepts_positive_score() {
        let positive_score_tape =
            asteroids_verifier_core::tape::serialize_tape(0xDEAD_BEEF, &[0x00], 10, 0xAABB_CCDD);
        assert!(validate_non_zero_score_tape(&positive_score_tape, MAX_FRAMES_DEFAULT).is_ok());
    }

    #[test]
    fn policy_rejects_allow_dev_mode_when_disabled() {
        let policy = strict_policy();
        let (msg, code) = policy
            .to_options(&ProveTapeQuery {
                allow_dev_mode: Some(true),
                ..Default::default()
            })
            .unwrap_err();
        assert!(msg.contains("allow_dev_mode"));
        assert_eq!(code, "dev_mode_disabled");
    }

    #[test]
    fn policy_rejects_out_of_range_segment_limit() {
        let policy = strict_policy();
        let (msg, code) = policy
            .to_options(&ProveTapeQuery {
                segment_limit_po2: Some(DEFAULT_MAX_SEGMENT_LIMIT_PO2 + 1),
                ..Default::default()
            })
            .unwrap_err();
        assert!(msg.contains("segment_limit_po2"));
        assert_eq!(code, "invalid_segment_limit");
    }

    #[test]
    fn policy_rejects_out_of_range_max_frames() {
        let policy = strict_policy();
        let (msg, code) = policy
            .to_options(&ProveTapeQuery {
                max_frames: Some(MAX_FRAMES_DEFAULT + 1),
                ..Default::default()
            })
            .unwrap_err();
        assert!(msg.contains("max_frames"));
        assert_eq!(code, "invalid_max_frames");
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
            error_code: None,
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
            error_code: None,
        };
        assert!(matches!(
            state.jobs.try_enqueue(&new_job, DEFAULT_MAX_JOBS).unwrap(),
            store::EnqueueResult::ProverBusy
        ));

        // Also verify the HTTP handler rejects.
        let response = enqueue_proof_job(state, vec![1_u8], sample_options()).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn policy_accepts_valid_defaults() {
        let policy = strict_policy();
        let options = policy.to_options(&ProveTapeQuery::default()).unwrap();
        assert_eq!(options.max_frames, MAX_FRAMES_DEFAULT);
        assert_eq!(options.segment_limit_po2, SEGMENT_LIMIT_PO2_DEFAULT);
        assert!(!options.allow_dev_mode);
        assert!(!options.verify_receipt);
    }

    #[test]
    fn policy_rejects_zero_max_frames() {
        let policy = strict_policy();
        let (_, code) = policy
            .to_options(&ProveTapeQuery {
                max_frames: Some(0),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(code, "invalid_max_frames");
    }

    #[test]
    fn policy_rejects_segment_limit_below_minimum() {
        let policy = strict_policy();
        let (_, code) = policy
            .to_options(&ProveTapeQuery {
                segment_limit_po2: Some(DEFAULT_MIN_SEGMENT_LIMIT_PO2 - 1),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(code, "invalid_segment_limit");
    }

    #[test]
    fn bearer_token_rejects_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic secret"));
        assert!(bearer_token(&headers).is_none());
    }

    #[test]
    fn bearer_token_rejects_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer   "));
        assert!(bearer_token(&headers).is_none());
    }
}
