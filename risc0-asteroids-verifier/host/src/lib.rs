use std::{env, str::FromStr};

use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::VerificationJournal;
use methods::{VERIFY_TAPE_ELF, VERIFY_TAPE_ID};
use risc0_zkvm::{default_prover, ExecutorEnv, Prover, ProverOpts, Receipt};
use serde::{Deserialize, Serialize};

pub const SEGMENT_LIMIT_PO2_DEFAULT: u32 = 21;

/// Return the current VERIFY_TAPE_ID as a hex string (32 bytes, 64 hex chars).
/// Each u32 word is encoded as little-endian, matching RISC Zero's Digest byte order.
pub fn image_id_hex() -> String {
    VERIFY_TAPE_ID
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn accelerator() -> &'static str {
    if cfg!(feature = "cuda") {
        "cuda"
    } else {
        "cpu"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptKind {
    #[default]
    Composite,
    Succinct,
    Groth16,
}

impl ReceiptKind {
    pub fn as_str(self) -> &'static str {
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

impl FromStr for ReceiptKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "composite" => Ok(Self::Composite),
            "succinct" => Ok(Self::Succinct),
            "groth16" => Ok(Self::Groth16),
            _ => Err(anyhow!(
                "invalid receipt kind: {value} (expected composite|succinct|groth16)"
            )),
        }
    }
}

impl std::fmt::Display for ReceiptKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProofStats {
    pub segments: u64,
    pub total_cycles: u64,
    pub user_cycles: u64,
    pub paging_cycles: u64,
    pub reserved_cycles: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapeProof {
    pub journal: VerificationJournal,
    pub receipt: Receipt,
    pub requested_receipt_kind: ReceiptKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_receipt_kind: Option<ReceiptKind>,
    pub stats: ProofStats,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProveOptions {
    pub max_frames: u32,
    pub segment_limit_po2: u32,
    pub receipt_kind: ReceiptKind,
    pub allow_dev_mode: bool,
    pub verify_receipt: bool,
}

impl Default for ProveOptions {
    fn default() -> Self {
        Self {
            max_frames: 18_000,
            segment_limit_po2: SEGMENT_LIMIT_PO2_DEFAULT,
            receipt_kind: ReceiptKind::Composite,
            allow_dev_mode: false,
            verify_receipt: true,
        }
    }
}

pub fn risc0_dev_mode_enabled() -> bool {
    env::var("RISC0_DEV_MODE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn detect_receipt_kind(receipt: &Receipt) -> Result<ReceiptKind> {
    if receipt.inner.groth16().is_ok() {
        return Ok(ReceiptKind::Groth16);
    }
    if receipt.inner.succinct().is_ok() {
        return Ok(ReceiptKind::Succinct);
    }
    if receipt.inner.composite().is_ok() {
        return Ok(ReceiptKind::Composite);
    }
    Err(anyhow!("failed to determine receipt kind"))
}

pub fn verify_tape_receipt(receipt: &Receipt) -> Result<()> {
    receipt
        .verify(VERIFY_TAPE_ID)
        .context("receipt verification failed for VERIFY_TAPE_ID")
}

pub fn prove_tape(tape: Vec<u8>, options: ProveOptions) -> Result<TapeProof> {
    let dev_mode_enabled = risc0_dev_mode_enabled();
    if dev_mode_enabled && !options.allow_dev_mode {
        return Err(anyhow!(
            "RISC0_DEV_MODE is enabled. Refusing to run without allow_dev_mode=true because fake receipts are insecure."
        ));
    }

    let tape_len = tape.len() as u32;
    let mut padded_tape = tape;
    while padded_tape.len() % 4 != 0 {
        padded_tape.push(0);
    }

    let mut env_builder = ExecutorEnv::builder();
    env_builder.write_slice(&options.max_frames.to_le_bytes());
    env_builder.write_slice(&tape_len.to_le_bytes());
    env_builder.write_slice(&padded_tape);
    env_builder.segment_limit_po2(options.segment_limit_po2);
    let env = env_builder
        .build()
        .context("failed to build executor env")?;

    let prover = default_prover();
    let prover_opts = options.receipt_kind.prover_opts();
    let prove_info = prover
        .prove_with_opts(env, VERIFY_TAPE_ELF, &prover_opts)
        .context("failed proving guest execution")?;

    let stats = ProofStats {
        segments: prove_info.stats.segments as u64,
        total_cycles: prove_info.stats.total_cycles as u64,
        user_cycles: prove_info.stats.user_cycles as u64,
        paging_cycles: prove_info.stats.paging_cycles as u64,
        reserved_cycles: prove_info.stats.reserved_cycles as u64,
    };

    let receipt = prove_info.receipt;
    let produced_receipt_kind = detect_receipt_kind(&receipt).ok();
    if !dev_mode_enabled {
        let actual = produced_receipt_kind
            .ok_or_else(|| anyhow!("failed to determine receipt kind for secure proof"))?;
        if actual != options.receipt_kind {
            return Err(anyhow!(
                "requested receipt kind {} but prover produced {}",
                options.receipt_kind,
                actual
            ));
        }
    }

    if options.verify_receipt {
        verify_tape_receipt(&receipt)?;
    }

    let journal: VerificationJournal = receipt
        .journal
        .decode()
        .context("failed decoding guest journal")?;

    Ok(TapeProof {
        journal,
        receipt,
        requested_receipt_kind: options.receipt_kind,
        produced_receipt_kind,
        stats,
    })
}
