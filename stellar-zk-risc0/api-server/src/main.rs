//! Asteroids ZK Proof API Server.
//!
//! Endpoints:
//! POST /api/prove-tape — Accept base64-encoded tape, return ZK proof
//! POST /api/verify-proof — Verify an existing proof
//! GET /health — Health check

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use host::{AsteroidsProof, prove_tape, verify_proof};
use risc0_zkvm::ReceiptKind;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ProveRequest {
    tape: String,
    #[serde(default = "default_receipt_kind")]
    receipt_kind: String,
}

fn default_receipt_kind() -> String {
    "succinct".to_string()
}

#[derive(Serialize)]
struct ProveResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof: Option<AsteroidsProof>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
struct VerifyRequest {
    proof: AsteroidsProof,
}

#[derive(Serialize)]
struct VerifyResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn handle_prove_tape(body: web::Json<ProveRequest>) -> HttpResponse {
    let tape_bytes = match BASE64.decode(&body.tape) {
        Ok(b) => b,
        Err(e) => {
            return HttpResponse::BadRequest().json(ProveResponse {
                success: false,
                proof: None,
                error: Some(format!("Invalid base64: {e}")),
            });
        }
    };

    let receipt_kind = match body.receipt_kind.as_str() {
        "composite" => ReceiptKind::Composite,
        "succinct" => ReceiptKind::Succinct,
        "groth16" => ReceiptKind::Groth16,
        other => {
            return HttpResponse::BadRequest().json(ProveResponse {
                success: false,
                proof: None,
                error: Some(format!("Unknown receipt_kind: {other}. Use composite, succinct, or groth16")),
            });
        }
    };

    // Run proving in a blocking thread (it's CPU-intensive)
    let result = web::block(move || prove_tape(&tape_bytes, receipt_kind)).await;

    match result {
        Ok(Ok(proof)) => HttpResponse::Ok().json(ProveResponse {
            success: true,
            proof: Some(proof),
            error: None,
        }),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(ProveResponse {
            success: false,
            proof: None,
            error: Some(format!("Proving failed: {e}")),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ProveResponse {
            success: false,
            proof: None,
            error: Some(format!("Internal error: {e}")),
        }),
    }
}

async fn handle_verify_proof(body: web::Json<VerifyRequest>) -> HttpResponse {
    let proof = body.into_inner().proof;

    let seed = proof.seed;
    let score = proof.score;
    let frame_count = proof.frame_count;

    let result = web::block(move || verify_proof(&proof)).await;

    match result {
        Ok(Ok(())) => HttpResponse::Ok().json(VerifyResponse {
            success: true,
            seed: Some(seed),
            score: Some(score),
            frame_count: Some(frame_count),
            error: None,
        }),
        Ok(Err(e)) => HttpResponse::BadRequest().json(VerifyResponse {
            success: false,
            seed: None,
            score: None,
            frame_count: None,
            error: Some(format!("Verification failed: {e}")),
        }),
        Err(e) => HttpResponse::InternalServerError().json(VerifyResponse {
            success: false,
            seed: None,
            score: None,
            frame_count: None,
            error: Some(format!("Internal error: {e}")),
        }),
    }
}

async fn handle_health() -> HttpResponse {
    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "asteroids-zk-api",
    })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Asteroids ZK API server on 0.0.0.0:8080");

    HttpServer::new(|| {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(web::JsonConfig::default().limit(10 * 1024 * 1024)) // 10MB
            .route("/health", web::get().to(handle_health))
            .route("/api/prove-tape", web::post().to(handle_prove_tape))
            .route("/api/verify-proof", web::post().to(handle_verify_proof))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
