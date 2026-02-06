//! ZK Host CLI: reads a .tape file, proves it inside the RISC Zero zkVM,
//! and verifies the resulting receipt.
//!
//! Usage:
//!   cargo run --release -p host -- <tape-file>
//!   RISC0_DEV_MODE=1 cargo run --release -p host -- <tape-file>

use clap::Parser;
use risc0_zkvm::ReceiptKind;
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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    println!("Loading tape: {}", args.tape_file);
    let tape_bytes = fs::read(&args.tape_file)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", args.tape_file));

    // Prove
    println!("Proving execution in zkVM...");
    let prove_start = Instant::now();
    let proof = host::prove_tape(&tape_bytes, ReceiptKind::Succinct)
        .expect("Proving failed");
    let prove_elapsed = prove_start.elapsed();

    println!();
    println!("=== PROVEN RESULTS ===");
    println!("  Seed:       0x{:08x}", proof.seed);
    println!("  Score:      {}", proof.score);
    println!("  Frames:     {}", proof.frame_count);
    println!("  Receipt:    {}", proof.receipt_kind);

    // Verify
    println!();
    println!("Verifying receipt...");
    host::verify_proof(&proof).expect("Receipt verification failed");
    println!("  Receipt verification PASSED");

    println!();
    println!(
        "SUCCESS: Score of {} proven for game seed 0x{:08x}",
        proof.score, proof.seed
    );
    println!(
        "         ({} frames, {:.1}s proving time)",
        proof.frame_count,
        prove_elapsed.as_secs_f64()
    );
}
