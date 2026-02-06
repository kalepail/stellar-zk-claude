//! Benchmark binary: runs execute-only (no proof generation) and reports cycle counts.
//!
//! Usage:
//!   cargo run --release --bin benchmark -- <tape-file>

use asteroids_core::deserialize_tape;
use clap::Parser;
use methods::ASTEROIDS_VERIFY_ELF;
use risc0_zkvm::{default_executor, ExecutorEnv};
use std::fs;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "asteroids-benchmark")]
#[command(about = "Benchmark Asteroids ZK guest execution (no proof generation)")]
struct Args {
    /// Path to the .tape file to benchmark
    tape_file: String,
}

fn main() {
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

    let frame_count = tape.header.frame_count;
    println!("  Seed:       0x{:08x}", tape.header.seed);
    println!("  Frames:     {}", frame_count);
    println!("  Exp. Score: {}", tape.footer.final_score);
    println!();

    // Build executor environment
    let env = ExecutorEnv::builder()
        .write(&tape_bytes)
        .unwrap()
        .build()
        .unwrap();

    // Execute (no proof generation)
    println!("Executing guest (no proving)...");
    let start = Instant::now();
    let executor = default_executor();
    let session = executor
        .execute(env, ASTEROIDS_VERIFY_ELF)
        .expect("Execution failed");
    let elapsed = start.elapsed();

    // Verify journal output
    let (proven_seed, proven_score, proven_frames): (u32, u32, u32) = session
        .journal
        .decode()
        .expect("Failed to decode journal");

    assert_eq!(proven_seed, tape.header.seed, "Seed mismatch in journal");
    assert_eq!(proven_score, tape.footer.final_score, "Score mismatch in journal");
    assert_eq!(proven_frames, frame_count, "Frame count mismatch in journal");

    // Report results
    let total_cycles = session.cycles();
    let segment_count = session.segments.len();
    let cycles_per_frame = if frame_count > 0 {
        total_cycles / frame_count as u64
    } else {
        0
    };

    println!();
    println!("=== BENCHMARK RESULTS ===");
    println!("  Wall time:       {:.3}s", elapsed.as_secs_f64());
    println!("  Total cycles:    {}", total_cycles);
    println!("  Segments:        {}", segment_count);
    println!("  Cycles/frame:    {}", cycles_per_frame);
    println!("  Proven score:    {}", proven_score);
    println!("  Frames:          {}", proven_frames);
    println!();

    // Per-segment breakdown
    println!("  Per-segment breakdown:");
    for (i, seg) in session.segments.iter().enumerate() {
        println!("    Segment {:>3}: {:>10} cycles (po2={})", i, seg.cycles, seg.po2);
    }
}
