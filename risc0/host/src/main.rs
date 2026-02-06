//! ZK Host program: reads a .tape file, proves it inside the RISC Zero zkVM,
//! and verifies the resulting receipt.
//!
//! Usage:
//!   cargo run --release -- <tape-file>
//!   RISC0_DEV_MODE=1 cargo run --release -- <tape-file>   (fast, no real proof)

use asteroids_core::deserialize_tape;
use clap::Parser;
use methods::{ASTEROIDS_VERIFY_ELF, ASTEROIDS_VERIFY_ID};
use risc0_zkvm::{default_prover, ExecutorEnv};
use std::fs;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "asteroids-zk-host")]
#[command(about = "Prove an Asteroids gameplay tape using RISC Zero zkVM")]
struct Args {
    /// Path to the .tape file to prove
    tape_file: String,
}

fn main() {
    // Initialize tracing for RISC0 logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Read and validate the tape file
    println!("Loading tape: {}", args.tape_file);
    let tape_bytes = fs::read(&args.tape_file)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", args.tape_file));

    let tape = deserialize_tape(&tape_bytes)
        .expect("Failed to parse tape");

    println!("  Seed:       0x{:08x}", tape.header.seed);
    println!("  Frames:     {}", tape.header.frame_count);
    println!("  Exp. Score: {}", tape.footer.final_score);
    println!("  Exp. RNG:   0x{:08x}", tape.footer.final_rng_state);
    println!();

    // Quick local verification before proving (optional sanity check)
    println!("Running local verification...");
    let verify_start = Instant::now();
    let (local_score, local_rng) = asteroids_core::replay_tape(tape.header.seed, &tape.inputs);
    let verify_elapsed = verify_start.elapsed();
    println!(
        "  Local replay: score={local_score}, rng=0x{local_rng:08x} ({:.1}ms)",
        verify_elapsed.as_secs_f64() * 1000.0
    );
    assert_eq!(local_score, tape.footer.final_score, "Local score mismatch");
    assert_eq!(local_rng, tape.footer.final_rng_state, "Local RNG mismatch");
    println!("  Local verification PASSED");
    println!();

    // Build the executor environment with the tape bytes as input
    println!("Building executor environment...");
    let env = ExecutorEnv::builder()
        .write(&tape_bytes)
        .unwrap()
        .build()
        .unwrap();

    // Prove execution of the guest program
    println!("Proving execution in zkVM...");
    let prove_start = Instant::now();
    let prover = default_prover();
    let prove_info = prover
        .prove(env, ASTEROIDS_VERIFY_ELF)
        .expect("Proving failed");
    let prove_elapsed = prove_start.elapsed();

    let receipt = prove_info.receipt;
    println!(
        "  Proving complete ({:.1}s)",
        prove_elapsed.as_secs_f64()
    );

    // Extract public outputs from the journal
    let journal_bytes = receipt.journal.bytes.clone();
    println!("  Journal size: {} bytes", journal_bytes.len());

    // Decode journal: three u32s committed by the guest via env::commit()
    // RISC0 serializes u32 as 4 bytes little-endian
    let proven_seed = u32::from_le_bytes(journal_bytes[0..4].try_into().unwrap());
    let proven_score = u32::from_le_bytes(journal_bytes[4..8].try_into().unwrap());
    let proven_frames = u32::from_le_bytes(journal_bytes[8..12].try_into().unwrap());

    println!();
    println!("=== PROVEN RESULTS ===");
    println!("  Seed:       0x{proven_seed:08x}");
    println!("  Score:      {proven_score}");
    println!("  Frames:     {proven_frames}");

    // Verify the receipt cryptographically
    println!();
    println!("Verifying receipt...");
    receipt
        .verify(ASTEROIDS_VERIFY_ID)
        .expect("Receipt verification failed");
    println!("  Receipt verification PASSED");

    println!();
    println!("SUCCESS: Score of {proven_score} proven for game seed 0x{proven_seed:08x}");
    println!("         ({proven_frames} frames, {:.1}s proving time)", prove_elapsed.as_secs_f64());
}
