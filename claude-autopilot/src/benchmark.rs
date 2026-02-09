use crate::bot::Bot;
use crate::config::BotConfig;
use crate::runner::{self, RunMetrics};
use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRecord {
    pub seed: u32,
    pub seed_hex: String,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_lives: i32,
    pub final_wave: i32,
    pub game_over: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub generated_unix_s: u64,
    pub bot_id: String,
    pub max_frames: u32,
    pub seed_count: usize,
    pub avg_score: f64,
    pub max_score: u32,
    pub avg_frames: f64,
    pub avg_lives: f64,
    pub survival_rate: f64,
    pub runs: Vec<RunRecord>,
    pub saved_tapes: Vec<SavedTapeRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTapeRecord {
    pub rank: usize,
    pub seed: u32,
    pub seed_hex: String,
    pub score: u32,
    pub frames: u32,
    pub path: String,
}

pub struct BenchmarkConfig {
    pub bot_config: BotConfig,
    pub seeds: Vec<u32>,
    pub max_frames: u32,
    pub out_dir: PathBuf,
    pub save_top: usize,
    pub jobs: Option<usize>,
}

struct InternalRun {
    metrics: RunMetrics,
    tape: Vec<u8>,
}

pub fn run_benchmark(config: BenchmarkConfig) -> Result<BenchmarkReport> {
    if config.seeds.is_empty() {
        return Err(anyhow!("benchmark requires at least one seed"));
    }

    fs::create_dir_all(&config.out_dir)
        .with_context(|| format!("failed creating {}", config.out_dir.display()))?;

    let run_one = |seed: &u32| -> Result<InternalRun> {
        let mut bot = Bot::new(config.bot_config.clone());
        let artifact = runner::run(&mut bot, *seed, config.max_frames)
            .with_context(|| format!("benchmark run failed for seed={seed:#x}"))?;
        Ok(InternalRun {
            metrics: artifact.metrics,
            tape: artifact.tape,
        })
    };

    let run_results: Vec<Result<InternalRun>> = if let Some(jobs) = config.jobs {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .context("failed to build rayon threadpool")?;
        pool.install(|| config.seeds.par_iter().map(run_one).collect())
    } else {
        config.seeds.par_iter().map(run_one).collect()
    };

    let mut runs = Vec::with_capacity(run_results.len());
    for result in run_results {
        runs.push(result?);
    }

    let total_runs = runs.len();
    let sum_score: u64 = runs.iter().map(|r| r.metrics.final_score as u64).sum();
    let max_score = runs
        .iter()
        .map(|r| r.metrics.final_score)
        .max()
        .unwrap_or(0);
    let sum_frames: u64 = runs.iter().map(|r| r.metrics.frame_count as u64).sum();
    let sum_lives: i64 = runs.iter().map(|r| r.metrics.final_lives as i64).sum();
    let survived = runs
        .iter()
        .filter(|r| !r.metrics.game_over && r.metrics.frame_count >= config.max_frames)
        .count();

    let mut run_records: Vec<RunRecord> = runs
        .iter()
        .map(|r| RunRecord {
            seed: r.metrics.seed,
            seed_hex: format!("{:#010x}", r.metrics.seed),
            frame_count: r.metrics.frame_count,
            final_score: r.metrics.final_score,
            final_lives: r.metrics.final_lives,
            final_wave: r.metrics.final_wave,
            game_over: r.metrics.game_over,
        })
        .collect();
    run_records.sort_by(|a, b| b.final_score.cmp(&a.final_score));

    let mut saved_tapes = Vec::new();
    if config.save_top > 0 {
        let mut order: Vec<usize> = (0..runs.len()).collect();
        order.sort_by(|a, b| runs[*b].metrics.final_score.cmp(&runs[*a].metrics.final_score));

        let save_dir = config.out_dir.join("top-tapes");
        fs::create_dir_all(&save_dir)?;

        for (idx, &run_idx) in order.iter().take(config.save_top).enumerate() {
            let r = &runs[run_idx];
            let rank = idx + 1;
            let filename = format!(
                "rank{rank:02}-seed{:08x}-score{}-frames{}.tape",
                r.metrics.seed, r.metrics.final_score, r.metrics.frame_count
            );
            let tape_path = save_dir.join(&filename);
            fs::write(&tape_path, &r.tape)?;
            saved_tapes.push(SavedTapeRecord {
                rank,
                seed: r.metrics.seed,
                seed_hex: format!("{:#010x}", r.metrics.seed),
                score: r.metrics.final_score,
                frames: r.metrics.frame_count,
                path: tape_path.to_string_lossy().into_owned(),
            });
        }
    }

    let report = BenchmarkReport {
        generated_unix_s: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        bot_id: config.bot_config.id,
        max_frames: config.max_frames,
        seed_count: total_runs,
        avg_score: sum_score as f64 / total_runs as f64,
        max_score,
        avg_frames: sum_frames as f64 / total_runs as f64,
        avg_lives: sum_lives as f64 / total_runs as f64,
        survival_rate: survived as f64 / total_runs as f64,
        runs: run_records,
        saved_tapes,
    };

    let report_path = config.out_dir.join("summary.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).context("failed to serialize summary")?,
    )?;

    Ok(report)
}
