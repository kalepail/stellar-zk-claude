use std::{env, fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::{GuestInput, VerificationJournal};
use methods::{VERIFY_TAPE_ELF, VERIFY_TAPE_ID};
use risc0_zkvm::{default_prover, ExecutorEnv, Prover, ProverOpts, Receipt};

const SEGMENT_LIMIT_PO2_DEFAULT: u32 = 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiptMode {
    Composite,
    Succinct,
    Groth16,
}

impl ReceiptMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "composite" => Ok(Self::Composite),
            "succinct" => Ok(Self::Succinct),
            "groth16" => Ok(Self::Groth16),
            _ => Err(anyhow!(
                "invalid --receipt-kind value: {value} (expected composite|succinct|groth16)"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Composite => "composite",
            Self::Succinct => "succinct",
            Self::Groth16 => "groth16",
        }
    }

    fn prover_opts(self) -> ProverOpts {
        match self {
            Self::Composite => ProverOpts::composite(),
            Self::Succinct => ProverOpts::succinct(),
            Self::Groth16 => ProverOpts::groth16(),
        }
    }
}

#[derive(Debug)]
struct Cli {
    tape_path: PathBuf,
    max_frames: u32,
    journal_out: Option<PathBuf>,
    segment_limit_po2: u32,
    receipt_mode: ReceiptMode,
    allow_dev_mode: bool,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);

        let mut tape_path: Option<PathBuf> = None;
        let mut max_frames = 18_000u32;
        let mut journal_out: Option<PathBuf> = None;
        let mut segment_limit_po2 = SEGMENT_LIMIT_PO2_DEFAULT;
        let mut receipt_mode = ReceiptMode::Composite;
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
                    segment_limit_po2 =
                        value
                            .parse::<u32>()
                            .with_context(|| {
                                format!("invalid --segment-limit-po2 value: {value}")
                            })?;
                }
                "--receipt-kind" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--receipt-kind requires a value"))?;
                    receipt_mode = ReceiptMode::parse(&value)?;
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
            receipt_mode,
            allow_dev_mode,
        })
    }
}

fn risc0_dev_mode_enabled() -> bool {
    env::var("RISC0_DEV_MODE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn detect_receipt_mode(receipt: &Receipt) -> Result<ReceiptMode> {
    if receipt.inner.groth16().is_ok() {
        return Ok(ReceiptMode::Groth16);
    }
    if receipt.inner.succinct().is_ok() {
        return Ok(ReceiptMode::Succinct);
    }
    if receipt.inner.composite().is_ok() {
        return Ok(ReceiptMode::Composite);
    }
    Err(anyhow!("failed to determine receipt kind"))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse()?;
    let dev_mode_enabled = risc0_dev_mode_enabled();
    if dev_mode_enabled && !cli.allow_dev_mode {
        return Err(anyhow!(
            "RISC0_DEV_MODE is enabled. Refusing to run without --allow-dev-mode because fake receipts are insecure."
        ));
    }

    let tape = fs::read(&cli.tape_path)
        .with_context(|| format!("failed to read tape: {}", cli.tape_path.display()))?;

    let guest_input = GuestInput {
        tape,
        max_frames: cli.max_frames,
    };

    let mut env_builder = ExecutorEnv::builder();
    env_builder
        .write(&guest_input)
        .context("failed to serialize guest input")?;
    env_builder.segment_limit_po2(cli.segment_limit_po2);
    let env = env_builder
        .build()
        .context("failed to build executor env")?;

    let prover = default_prover();
    let prover_opts = cli.receipt_mode.prover_opts();
    let prove_info = prover
        .prove_with_opts(env, VERIFY_TAPE_ELF, &prover_opts)
        .context("failed proving guest execution")?;

    let stats = prove_info.stats;
    let receipt = prove_info.receipt;
    let actual_receipt_mode = detect_receipt_mode(&receipt).ok();
    if !dev_mode_enabled {
        let actual_receipt_mode = actual_receipt_mode
            .ok_or_else(|| anyhow!("failed to determine receipt kind for secure proof"))?;
        if actual_receipt_mode != cli.receipt_mode {
            return Err(anyhow!(
                "requested --receipt-kind {} but prover produced {}",
                cli.receipt_mode.as_str(),
                actual_receipt_mode.as_str()
            ));
        }
    }

    receipt
        .verify(VERIFY_TAPE_ID)
        .context("receipt verification failed for VERIFY_TAPE_ID")?;

    let journal: VerificationJournal = receipt
        .journal
        .decode()
        .context("failed decoding guest journal")?;

    println!("Verification proof generated and validated.");
    let receipt_kind_label = actual_receipt_mode
        .map(ReceiptMode::as_str)
        .unwrap_or("dev-fake");
    println!("  Receipt kind:  {}", receipt_kind_label);
    println!("  Seed:          0x{:08x}", journal.seed);
    println!("  Frames:        {}", journal.frame_count);
    println!("  Final score:   {}", journal.final_score);
    println!("  Final RNG:     0x{:08x}", journal.final_rng_state);
    println!("  Tape checksum: 0x{:08x}", journal.tape_checksum);
    println!("  Rules digest:  0x{:08x}", journal.rules_digest);
    println!("  Segments:      {}", stats.segments);
    println!("  Total cycles:  {}", stats.total_cycles);
    println!("  User cycles:   {}", stats.user_cycles);
    println!("  Paging cycles: {}", stats.paging_cycles);
    println!("  Reserved:      {}", stats.reserved_cycles);

    if let Some(path) = cli.journal_out {
        let json =
            serde_json::to_vec_pretty(&journal).context("failed to serialize journal json")?;
        fs::write(&path, json)
            .with_context(|| format!("failed writing journal output: {}", path.display()))?;
        println!("  Journal JSON:  {}", path.display());
    }

    Ok(())
}
