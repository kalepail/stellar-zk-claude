use std::{env, fs, path::PathBuf, process};

use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::{
    constants::MAX_FRAMES_DEFAULT, tape::parse_tape, VerificationJournal,
};
use methods::VERIFY_TAPE_ELF;
use risc0_zkvm::{default_executor, ExecutorEnv};

#[derive(Debug)]
struct Cli {
    tape_path: PathBuf,
    max_frames: u32,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let mut tape_path: Option<PathBuf> = None;
        let mut max_frames = MAX_FRAMES_DEFAULT;

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
                "-h" | "--help" => {
                    println!(
                        "Usage: cargo run --release -p host --bin benchmark -- --tape <file.tape> [--max-frames <n>]"
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
        })
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse()?;
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

    let env = ExecutorEnv::builder()
        .write_slice(&cli.max_frames.to_le_bytes())
        .write_slice(&tape_len.to_le_bytes())
        .write_slice(&padded_tape)
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
    let segments = session.segments.len();
    let cycles_per_frame = if journal.frame_count == 0 {
        0
    } else {
        total_cycles / journal.frame_count as u64
    };

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
