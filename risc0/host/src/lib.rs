//! Asteroids ZK proving library.
//!
//! Provides reusable functions for proving and verifying Asteroids gameplay tapes
//! using RISC Zero zkVM.

use asteroids_core::deserialize_tape;
use methods::{ASTEROIDS_VERIFY_ELF, ASTEROIDS_VERIFY_ID};
use risc0_zkvm::{default_prover, ExecutorEnv, ProverOpts, Receipt, ReceiptKind};
use serde::{Deserialize, Serialize};

/// Proven Asteroids game result with its ZK receipt.
#[derive(Serialize, Deserialize)]
pub struct AsteroidsProof {
    pub seed: u32,
    pub score: u32,
    pub frame_count: u32,
    pub receipt: Receipt,
    pub receipt_kind: String,
}

/// Prove an Asteroids gameplay tape, returning the proof and game results.
///
/// `tape_bytes` must be a valid .tape file (with CRC).
/// `receipt_kind` controls proof compression level.
pub fn prove_tape(tape_bytes: &[u8], receipt_kind: ReceiptKind) -> anyhow::Result<AsteroidsProof> {
    // Validate tape (checks CRC, magic, version, etc.)
    let tape = deserialize_tape(tape_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid tape: {e:?}"))?;

    tracing::info!(
        seed = format!("0x{:08x}", tape.header.seed),
        frames = tape.header.frame_count,
        expected_score = tape.footer.final_score,
        "Tape validated"
    );

    // Build executor environment with raw tape bytes (word-padded)
    let tape_len = tape_bytes.len() as u32;
    let mut padded_tape = tape_bytes.to_vec();
    while padded_tape.len() % 4 != 0 {
        padded_tape.push(0);
    }
    let env = ExecutorEnv::builder()
        .write_slice(&tape_len.to_le_bytes())
        .write_slice(&padded_tape)
        .build()?;

    // Build prover options
    let opts = match receipt_kind {
        ReceiptKind::Composite => ProverOpts::composite(),
        ReceiptKind::Succinct => ProverOpts::succinct(),
        ReceiptKind::Groth16 => ProverOpts::groth16(),
        _ => ProverOpts::succinct(),
    };

    let kind_str = match receipt_kind {
        ReceiptKind::Composite => "composite",
        ReceiptKind::Succinct => "succinct",
        ReceiptKind::Groth16 => "groth16",
        _ => "succinct",
    };

    // Prove
    tracing::info!(receipt_kind = kind_str, "Starting proof generation");
    let prover = default_prover();
    let prove_info = prover.prove_with_opts(env, ASTEROIDS_VERIFY_ELF, &opts)?;
    let receipt = prove_info.receipt;

    // Decode journal: three u32s committed by the guest
    let (proven_seed, proven_score, proven_frames): (u32, u32, u32) =
        receipt.journal.decode()?;

    tracing::info!(
        seed = format!("0x{proven_seed:08x}"),
        score = proven_score,
        frames = proven_frames,
        "Proof generated"
    );

    Ok(AsteroidsProof {
        seed: proven_seed,
        score: proven_score,
        frame_count: proven_frames,
        receipt,
        receipt_kind: kind_str.to_string(),
    })
}

/// Verify an existing Asteroids proof receipt.
pub fn verify_proof(proof: &AsteroidsProof) -> anyhow::Result<()> {
    proof
        .receipt
        .verify(ASTEROIDS_VERIFY_ID)
        .map_err(|e| anyhow::anyhow!("Receipt verification failed: {e}"))?;
    Ok(())
}
