use anyhow::{anyhow, Result};
use rust_autopilot::bots::{bot_fingerprint, create_bot};
use serde::Deserialize;
use std::collections::BTreeSet;
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
    record_lock_bot: RecordLock,
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

#[test]
fn champion_registry_matches_bot_roster_and_files() -> Result<()> {
    let raw = fs::read(repo_path("records/champions.json"))?;
    let registry: ChampionRegistry = serde_json::from_slice(&raw)?;

    if create_bot(&registry.record_lock_bot.bot_id).is_none() {
        return Err(anyhow!(
            "record lock bot missing from roster: {}",
            registry.record_lock_bot.bot_id
        ));
    }
    let lock_fp = bot_fingerprint(&registry.record_lock_bot.bot_id).ok_or_else(|| {
        anyhow!(
            "missing fingerprint for {}",
            registry.record_lock_bot.bot_id
        )
    })?;
    if lock_fp != registry.record_lock_bot.bot_fingerprint {
        return Err(anyhow!(
            "record lock bot fingerprint mismatch: expected={} actual={}",
            registry.record_lock_bot.bot_fingerprint,
            lock_fp
        ));
    }
    if !repo_path(&registry.record_lock_bot.source_tape).exists() {
        return Err(anyhow!(
            "record lock source tape missing: {}",
            registry.record_lock_bot.source_tape
        ));
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

        if !repo_path(&run.checkpoint_tape).exists() {
            return Err(anyhow!("missing tape: {}", run.checkpoint_tape));
        }
        if !repo_path(&run.checkpoint_meta).exists() {
            return Err(anyhow!("missing metadata: {}", run.checkpoint_meta));
        }

        if run.source.starts_with("benchmarks/") && !repo_path(&run.source).exists() {
            return Err(anyhow!("missing benchmark source path: {}", run.source));
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
