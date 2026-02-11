use anyhow::{anyhow, Result};
use asteroids_verifier_core::tape::parse_tape;
use asteroids_verifier_core::verify_tape;
use rust_autopilot::bots::{bot_fingerprint, create_bot};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct RecordLock {
    bot_id: String,
    bot_fingerprint: String,
    source_tape: String,
}

#[derive(Debug, Deserialize)]
struct ChampionRun {
    checkpoint_base: String,
    checkpoint_tape: String,
    checkpoint_meta: String,
    bot_id: String,
    bot_fingerprint: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct ChampionRegistry {
    #[serde(default)]
    record_lock_bot: Option<RecordLock>,
    #[serde(default)]
    runs: Vec<ChampionRun>,
}

fn read_keep_set(path: &str) -> Result<BTreeSet<String>> {
    let raw = fs::read_to_string(repo_path(path))?;
    let mut out = BTreeSet::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.insert(trimmed.to_string());
    }
    Ok(out)
}

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn strict_artifacts_enabled() -> bool {
    matches!(
        env::var("AUTOPILOT_STRICT_ARTIFACTS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

fn strict_validate_tape(path: &str) -> Result<()> {
    let bytes = fs::read(repo_path(path))?;
    // Use a generous bound so this check validates format+rules, not the current bench cap.
    let max_frames = 1_000_000;
    parse_tape(&bytes, max_frames).map_err(|err| anyhow!("invalid tape {path}: {err}"))?;
    verify_tape(&bytes, max_frames).map_err(|err| anyhow!("unverifiable tape {path}: {err}"))?;
    Ok(())
}

#[test]
fn champion_registry_matches_bot_roster_and_files() -> Result<()> {
    let raw = fs::read(repo_path("records/champions.json"))?;
    let registry: ChampionRegistry = serde_json::from_slice(&raw)?;

    if let Some(lock) = &registry.record_lock_bot {
        if create_bot(&lock.bot_id).is_none() {
            return Err(anyhow!("record lock bot missing from roster: {}", lock.bot_id));
        }
        let lock_fp = bot_fingerprint(&lock.bot_id)
            .ok_or_else(|| anyhow!("missing fingerprint for {}", lock.bot_id))?;
        if lock_fp != lock.bot_fingerprint {
            return Err(anyhow!(
                "record lock bot fingerprint mismatch: expected={} actual={}",
                lock.bot_fingerprint,
                lock_fp
            ));
        }
        if strict_artifacts_enabled() {
            if !repo_path(&lock.source_tape).exists() {
                return Err(anyhow!("record lock source tape missing: {}", lock.source_tape));
            }
            strict_validate_tape(&lock.source_tape)?;
        }
    }

    let keep_checkpoints = read_keep_set("records/keep-checkpoints.txt")?;
    let mut run_bases = BTreeSet::new();

    for run in &registry.runs {
        if create_bot(&run.bot_id).is_none() {
            return Err(anyhow!("champion references unknown bot: {}", run.bot_id));
        }
        let actual_fp = bot_fingerprint(&run.bot_id)
            .ok_or_else(|| anyhow!("missing fingerprint for {}", run.bot_id))?;
        if actual_fp != run.bot_fingerprint {
            return Err(anyhow!(
                "fingerprint mismatch for {}: expected={} actual={}",
                run.bot_id,
                run.bot_fingerprint,
                actual_fp
            ));
        }

        if strict_artifacts_enabled() {
            if !repo_path(&run.checkpoint_tape).exists() {
                return Err(anyhow!("missing tape: {}", run.checkpoint_tape));
            }
            if !repo_path(&run.checkpoint_meta).exists() {
                return Err(anyhow!("missing metadata: {}", run.checkpoint_meta));
            }
            strict_validate_tape(&run.checkpoint_tape)?;

            if run.source.starts_with("benchmarks/") && !repo_path(&run.source).exists() {
                return Err(anyhow!("missing benchmark source path: {}", run.source));
            }
        }

        run_bases.insert(run.checkpoint_base.clone());
    }

    if run_bases != keep_checkpoints {
        return Err(anyhow!(
            "registry run set and keep-checkpoints.txt differ\nregistry={:?}\nkeep={:?}",
            run_bases,
            keep_checkpoints
        ));
    }

    Ok(())
}

#[test]
fn keep_benchmark_dirs_exist() -> Result<()> {
    if !strict_artifacts_enabled() {
        return Ok(());
    }
    let keep_benchmarks = read_keep_set("records/keep-benchmarks.txt")?;
    for bench in keep_benchmarks {
        let path = repo_path(&format!("benchmarks/{bench}"));
        if !Path::new(&path).is_dir() {
            return Err(anyhow!(
                "missing benchmark dir from keep list: {}",
                path.display()
            ));
        }
    }
    Ok(())
}
