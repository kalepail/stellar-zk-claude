use std::{env, fs, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context, Result};
use host::{prove_tape, ProveOptions, ReceiptKind, SEGMENT_LIMIT_PO2_DEFAULT};

#[derive(Debug)]
struct Cli {
    tape_path: PathBuf,
    max_frames: u32,
    journal_out: Option<PathBuf>,
    segment_limit_po2: u32,
    receipt_kind: ReceiptKind,
    allow_dev_mode: bool,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);

        let mut tape_path: Option<PathBuf> = None;
        let mut max_frames = 18_000u32;
        let mut journal_out: Option<PathBuf> = None;
        let mut segment_limit_po2 = SEGMENT_LIMIT_PO2_DEFAULT;
        let mut receipt_kind = ReceiptKind::Composite;
        let mut allow_dev_mode = false;

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
                "--segment-limit-po2" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--segment-limit-po2 requires a number"))?;
                    segment_limit_po2 = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid --segment-limit-po2 value: {value}"))?;
                }
                "--receipt-kind" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--receipt-kind requires a value"))?;
                    receipt_kind = ReceiptKind::from_str(&value)?;
                }
                "--allow-dev-mode" => {
                    allow_dev_mode = true;
                }
                "-h" | "--help" => {
                    println!(
                        "Usage: cargo run --release -- --tape <file.tape> [--max-frames <n>] [--journal-out <file.json>] [--segment-limit-po2 <n>] [--receipt-kind composite|succinct|groth16] [--allow-dev-mode]\nDefault --segment-limit-po2: {SEGMENT_LIMIT_PO2_DEFAULT}"
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
            segment_limit_po2,
            receipt_kind,
            allow_dev_mode,
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

    let proof = prove_tape(
        tape,
        ProveOptions {
            max_frames: cli.max_frames,
            segment_limit_po2: cli.segment_limit_po2,
            receipt_kind: cli.receipt_kind,
            allow_dev_mode: cli.allow_dev_mode,
            verify_receipt: true,
        },
    )?;

    println!("Verification proof generated and validated.");
    println!(
        "  Receipt kind:  {}",
        proof
            .produced_receipt_kind
            .map(|kind| kind.as_str())
            .unwrap_or("dev-fake")
    );
    println!("  Seed:          0x{:08x}", proof.journal.seed);
    println!("  Frames:        {}", proof.journal.frame_count);
    println!("  Final score:   {}", proof.journal.final_score);
    println!("  Final RNG:     0x{:08x}", proof.journal.final_rng_state);
    println!("  Tape checksum: 0x{:08x}", proof.journal.tape_checksum);
    println!("  Rules digest:  0x{:08x}", proof.journal.rules_digest);
    println!("  Segments:      {}", proof.stats.segments);
    println!("  Total cycles:  {}", proof.stats.total_cycles);
    println!("  User cycles:   {}", proof.stats.user_cycles);
    println!("  Paging cycles: {}", proof.stats.paging_cycles);
    println!("  Reserved:      {}", proof.stats.reserved_cycles);

    if let Some(path) = cli.journal_out {
        let json = serde_json::to_vec_pretty(&proof.journal)
            .context("failed to serialize journal json")?;
        fs::write(&path, json)
            .with_context(|| format!("failed writing journal output: {}", path.display()))?;
        println!("  Journal JSON:  {}", path.display());
    }

    Ok(())
}
