use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;

pub fn parse_seed(seed: &str) -> Result<u32> {
    let s = seed.trim();
    if s.is_empty() {
        return Err(anyhow!("empty seed"));
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).with_context(|| format!("invalid hex seed: {s}"))
    } else {
        s.parse::<u32>()
            .with_context(|| format!("invalid decimal seed: {s}"))
    }
}

pub fn seed_to_hex(seed: u32) -> String {
    format!("0x{seed:08x}")
}

pub fn parse_seed_csv(input: &str) -> Result<Vec<u32>> {
    let mut seeds = Vec::new();
    for token in input.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        seeds.push(parse_seed(token)?);
    }
    if seeds.is_empty() {
        return Err(anyhow!("no seeds parsed from --seeds"));
    }
    Ok(seeds)
}

pub fn parse_seed_file(path: &Path) -> Result<Vec<u32>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("failed reading seed file {}", path.display()))?;
    let mut seeds = Vec::new();
    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        seeds.push(parse_seed(trimmed)?);
    }
    if seeds.is_empty() {
        return Err(anyhow!("seed file {} had no seeds", path.display()));
    }
    Ok(seeds)
}
