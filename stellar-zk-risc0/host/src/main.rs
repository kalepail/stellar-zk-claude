use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use asteroids_core::{PublicOutput, Tape};
use clap::Parser;
use methods::{ASTEROIDS_VERIFY_ELF, ASTEROIDS_VERIFY_ID};
use risc0_zkvm::{default_prover, ExecutorEnv};

#[derive(Parser)]
#[command(name = "asteroids-verify")]
#[command(about = "Verify Asteroids game tapes using RISC0 ZK proofs")]
struct Args {
    /// Path to the tape file to verify
    #[arg(short, long)]
    tape: PathBuf,

    /// Output path for the receipt (optional)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Verify only (don't generate proof)
    #[arg(long)]
    verify_only: bool,

    /// Print detailed information
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Load and parse the tape
    let tape_data = match fs::read(&args.tape) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error reading tape file: {}", e);
            std::process::exit(1);
        }
    };

    if args.verbose {
        println!("Loaded tape: {} bytes", tape_data.len());
    }

    // Parse the tape
    let tape = match Tape::from_bytes(&tape_data) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error parsing tape: {}", e);
            std::process::exit(1);
        }
    };

    if args.verbose {
        println!("Tape validated successfully:");
        println!("  Seed: 0x{:08x}", tape.header.seed);
        println!("  Frames: {}", tape.header.frame_count);
        println!("  Expected score: {}", tape.footer.final_score);
        println!(
            "  Expected RNG state: 0x{:08x}",
            tape.footer.final_rng_state
        );
    }

    if args.verify_only {
        // Just verify the tape parsing, don't generate proof
        println!("Tape is valid!");
        std::process::exit(0);
    }

    // Generate the proof
    println!("Generating ZK proof...");
    let start = Instant::now();

    let env = match ExecutorEnv::builder().write(&tape).build() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error building executor environment: {}", e);
            std::process::exit(1);
        }
    };

    let prover = default_prover();

    let prove_info = match prover.prove(env, ASTEROIDS_VERIFY_ELF) {
        Ok(info) => info,
        Err(e) => {
            eprintln!("Error generating proof: {}", e);
            std::process::exit(1);
        }
    };

    let elapsed = start.elapsed();
    println!("Proof generated in {:.2}s", elapsed.as_secs_f64());

    // Extract and decode the output
    let output: PublicOutput = match prove_info.receipt.journal.decode() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error decoding output: {}", e);
            std::process::exit(1);
        }
    };

    println!("\nVerification result:");
    println!("  Verified: {}", output.verified);
    println!("  Seed: 0x{:08x}", output.seed);
    println!("  Frames: {}", output.frame_count);
    println!("  Final score: {}", output.final_score);
    println!("  Final RNG state: 0x{:08x}", output.final_rng_state);
    println!("  Tape CRC: 0x{:08x}", output.tape_crc);

    // Verify the receipt
    println!("\nVerifying receipt...");
    match prove_info.receipt.verify(ASTEROIDS_VERIFY_ID) {
        Ok(_) => println!("Receipt verified successfully!"),
        Err(e) => {
            eprintln!("Receipt verification failed: {}", e);
            std::process::exit(1);
        }
    }

    // Save receipt if requested
    if let Some(output_path) = args.output {
        match prove_info.receipt.save(&output_path) {
            Ok(_) => println!("Receipt saved to: {}", output_path.display()),
            Err(e) => eprintln!("Warning: Failed to save receipt: {}", e),
        }
    }

    // Exit with appropriate code
    if output.verified {
        println!("\n✓ TAPE VERIFIED - Game was played fairly!");
        std::process::exit(0);
    } else {
        println!("\n✗ TAPE REJECTED - Verification failed!");
        std::process::exit(1);
    }
}
