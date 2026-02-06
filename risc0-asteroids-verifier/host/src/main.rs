use std::{env, fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::{GuestInput, VerificationJournal};
use methods::{VERIFY_TAPE_ELF, VERIFY_TAPE_ID};
use risc0_zkvm::{default_prover, ExecutorEnv};

#[derive(Debug)]
struct Cli {
    tape_path: PathBuf,
    max_frames: u32,
    journal_out: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);

        let mut tape_path: Option<PathBuf> = None;
        let mut max_frames = 18_000u32;
        let mut journal_out: Option<PathBuf> = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--tape" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--tape requires a file path"))?;
                    tape_path = Some(PathBuf::from(value));
                }
                "--max-frames" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--max-frames requires a number"))?;
                    max_frames = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid --max-frames value: {value}"))?;
                }
                "--journal-out" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--journal-out requires a file path"))?;
                    journal_out = Some(PathBuf::from(value));
                }
                "-h" | "--help" => {
                    println!(
                        "Usage: cargo run --release -- --tape <file.tape> [--max-frames <n>] [--journal-out <file.json>]"
                    );
                    std::process::exit(0);
                }
                other => return Err(anyhow!("unknown argument: {other}. Use --help for usage.")),
            }
        }

        let tape_path = tape_path.ok_or_else(|| anyhow!("--tape is required"))?;

        Ok(Self {
            tape_path,
            max_frames,
            journal_out,
        })
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse()?;
    let tape = fs::read(&cli.tape_path)
        .with_context(|| format!("failed to read tape: {}", cli.tape_path.display()))?;

    let guest_input = GuestInput {
        tape,
        max_frames: cli.max_frames,
    };

    let env = ExecutorEnv::builder()
        .write(&guest_input)
        .context("failed to serialize guest input")?
        .build()
        .context("failed to build executor env")?;

    let prover = default_prover();
    let prove_info = prover
        .prove(env, VERIFY_TAPE_ELF)
        .context("failed proving guest execution")?;

    let receipt = prove_info.receipt;

    receipt
        .verify(VERIFY_TAPE_ID)
        .context("receipt verification failed for VERIFY_TAPE_ID")?;

    let journal: VerificationJournal = receipt
        .journal
        .decode()
        .context("failed decoding guest journal")?;

    println!("Verification proof generated and validated.");
    println!("  Seed:          0x{:08x}", journal.seed);
    println!("  Frames:        {}", journal.frame_count);
    println!("  Final score:   {}", journal.final_score);
    println!("  Final RNG:     0x{:08x}", journal.final_rng_state);
    println!("  Tape checksum: 0x{:08x}", journal.tape_checksum);
    println!("  Rules digest:  0x{:08x}", journal.rules_digest);

    if let Some(path) = cli.journal_out {
        let json =
            serde_json::to_vec_pretty(&journal).context("failed to serialize journal json")?;
        fs::write(&path, json)
            .with_context(|| format!("failed writing journal output: {}", path.display()))?;
        println!("  Journal JSON:  {}", path.display());
    }

    Ok(())
}
