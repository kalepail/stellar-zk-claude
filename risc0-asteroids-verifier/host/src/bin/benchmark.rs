use std::{env, fs, path::PathBuf, process};

use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::{
    constants::MAX_FRAMES_DEFAULT, tape::parse_tape, VerificationJournal,
};
use host::SEGMENT_LIMIT_PO2_DEFAULT;
use methods::VERIFY_TAPE_ELF;
use risc0_zkvm::{default_executor, ExecutorEnv};
use serde::Serialize;

#[derive(Debug)]
struct Cli {
    tape_path: PathBuf,
    max_frames: u32,
    segment_limit_po2: u32,
    json_out: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BenchmarkJson {
    seed: u32,
    frame_count: u32,
    final_score: u32,
    final_rng_state: u32,
    tape_checksum: u32,
    rules_digest: u32,
    segments: u64,
    total_cycles: u64,
    cycles_per_frame: u64,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let mut tape_path: Option<PathBuf> = None;
        let mut max_frames = MAX_FRAMES_DEFAULT;
        let mut segment_limit_po2 = SEGMENT_LIMIT_PO2_DEFAULT;
        let mut json_out: Option<PathBuf> = None;

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
                "--segment-limit-po2" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--segment-limit-po2 requires a number"))?;
                    segment_limit_po2 = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid --segment-limit-po2 value: {value}"))?;
                }
                "--json-out" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--json-out requires a file path"))?;
                    json_out = Some(PathBuf::from(value));
                }
                "-h" | "--help" => {
                    println!(
                        "Usage: cargo run --release -p host --bin benchmark -- --tape <file.tape> [--max-frames <n>] [--segment-limit-po2 <n>] [--json-out <file.json>]\n\nNote: This benchmark is intended to run in dev mode only (RISC0_DEV_MODE=1)."
                    );
                    process::exit(0);
                }
                other => return Err(anyhow!("unknown argument: {other}. Use --help for usage.")),
            }
        }

        let tape_path = tape_path.ok_or_else(|| anyhow!("--tape is required"))?;
        Ok(Self {
            tape_path,
            max_frames,
            segment_limit_po2,
            json_out,
        })
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse()?;
    if !host::risc0_dev_mode_enabled() {
        return Err(anyhow!(
            "RISC0_DEV_MODE is not enabled. This benchmark is dev-mode only. Re-run with: RISC0_DEV_MODE=1 cargo run -p host --release --no-default-features --bin benchmark -- ..."
        ));
    }

    let tape_bytes = fs::read(&cli.tape_path)
        .with_context(|| format!("failed to read tape: {}", cli.tape_path.display()))?;
    let (expected_seed, expected_frame_count, expected_score, expected_rng_state) = {
        let tape = parse_tape(&tape_bytes, cli.max_frames).context("failed to parse tape")?;
        (
            tape.header.seed,
            tape.header.frame_count,
            tape.footer.final_score,
            tape.footer.final_rng_state,
        )
    };

    let tape_len = tape_bytes.len() as u32;
    let mut padded_tape = tape_bytes;
    while padded_tape.len() % 4 != 0 {
        padded_tape.push(0);
    }

    let mut env_builder = ExecutorEnv::builder();
    env_builder.write_slice(&cli.max_frames.to_le_bytes());
    env_builder.write_slice(&tape_len.to_le_bytes());
    env_builder.write_slice(&padded_tape);
    env_builder.segment_limit_po2(cli.segment_limit_po2);
    let env = env_builder
        .build()
        .context("failed to build executor env")?;

    let session = default_executor()
        .execute(env, VERIFY_TAPE_ELF)
        .context("failed executing guest")?;
    let journal: VerificationJournal = session
        .journal
        .decode()
        .context("failed decoding journal")?;

    if journal.seed != expected_seed
        || journal.frame_count != expected_frame_count
        || journal.final_score != expected_score
        || journal.final_rng_state != expected_rng_state
    {
        return Err(anyhow!(
            "journal output mismatch: seed={:#x}/{:#x} frames={}/{} score={}/{} rng={:#x}/{:#x}",
            journal.seed,
            expected_seed,
            journal.frame_count,
            expected_frame_count,
            journal.final_score,
            expected_score,
            journal.final_rng_state,
            expected_rng_state,
        ));
    }

    let total_cycles = session.cycles();
    let segments = session.segments.len() as u64;
    let cycles_per_frame = if journal.frame_count == 0 {
        0
    } else {
        total_cycles / journal.frame_count as u64
    };

    if let Some(path) = cli.json_out.as_ref() {
        let summary = BenchmarkJson {
            seed: journal.seed,
            frame_count: journal.frame_count,
            final_score: journal.final_score,
            final_rng_state: journal.final_rng_state,
            tape_checksum: journal.tape_checksum,
            rules_digest: journal.rules_digest,
            segments,
            total_cycles,
            cycles_per_frame,
        };
        let json =
            serde_json::to_vec_pretty(&summary).context("failed serializing benchmark summary")?;
        fs::write(path, json)
            .with_context(|| format!("failed writing benchmark summary to {}", path.display()))?;
    }

    println!("Benchmark complete.");
    println!("  Seed:          0x{:08x}", journal.seed);
    println!("  Frames:        {}", journal.frame_count);
    println!("  Final score:   {}", journal.final_score);
    println!("  Final RNG:     0x{:08x}", journal.final_rng_state);
    println!("  Tape checksum: 0x{:08x}", journal.tape_checksum);
    println!("  Rules digest:  0x{:08x}", journal.rules_digest);
    println!("  Segments:      {}", segments);
    println!("  Total cycles:  {}", total_cycles);
    println!("  Cycles/frame:  {}", cycles_per_frame);

    Ok(())
}
