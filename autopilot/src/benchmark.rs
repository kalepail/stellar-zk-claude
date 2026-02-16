use crate::bots::bot_ids;
use crate::runner::{run_bot, RunMetrics};
use crate::util::seed_to_hex;
use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    Score,
    Survival,
    Hybrid,
}

impl Objective {
    pub fn run_value(self, metrics: &RunMetrics) -> f64 {
        match self {
            Self::Score => {
                (metrics.final_score as f64) * 1.0
                    + (metrics.frame_count as f64) * 0.08
                    + (metrics.final_lives.max(0) as f64) * 120.0
            }
            Self::Survival => {
                (metrics.frame_count as f64) * 1.0
                    + (metrics.final_lives.max(0) as f64) * 850.0
                    + (metrics.final_score as f64) * 0.15
            }
            Self::Hybrid => {
                (metrics.final_score as f64) * 0.75
                    + (metrics.frame_count as f64) * 0.55
                    + (metrics.final_lives.max(0) as f64) * 260.0
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Score => "score",
            Self::Survival => "survival",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BenchmarkConfig {
    pub bots: Vec<String>,
    pub seeds: Vec<u32>,
    pub max_frames: u32,
    pub objective: Objective,
    pub out_dir: PathBuf,
    pub save_top: usize,
    pub jobs: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRecord {
    pub bot_id: String,
    pub bot_fingerprint: String,
    pub seed: u32,
    pub seed_hex: String,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_lives: i32,
    pub final_wave: i32,
    pub game_over: bool,
    pub objective_value: f64,
    pub action_frames: u32,
    pub turn_frames: u32,
    pub thrust_frames: u32,
    pub fire_frames: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotAggregate {
    pub bot_id: String,
    pub bot_fingerprint: String,
    pub runs: usize,
    pub avg_score: f64,
    pub max_score: u32,
    pub avg_frames: f64,
    pub max_frames: u32,
    pub avg_lives: f64,
    pub min_lives: i32,
    pub survival_rate: f64,
    pub objective_value: f64,
    pub avg_action_frames: f64,
    pub avg_turn_frames: f64,
    pub avg_thrust_frames: f64,
    pub avg_fire_frames: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTapeRecord {
    pub rank: usize,
    pub metric: String,
    pub bot_id: String,
    pub bot_fingerprint: String,
    pub seed: u32,
    pub seed_hex: String,
    pub score: u32,
    pub frames: u32,
    pub lives: i32,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub generated_unix_s: u64,
    pub objective: Objective,
    pub max_frames: u32,
    pub jobs: Option<usize>,
    pub bots: Vec<String>,
    pub seeds: Vec<u32>,
    pub run_count: usize,
    pub bot_rankings: Vec<BotAggregate>,
    pub runs: Vec<RunRecord>,
    pub saved_tapes: Vec<SavedTapeRecord>,
}

#[derive(Clone, Debug)]
struct InternalRun {
    metrics: RunMetrics,
    objective_value: f64,
    tape: Vec<u8>,
}

pub fn resolve_bots(input: Option<&str>) -> Result<Vec<String>> {
    match input {
        None => Ok(bot_ids().iter().map(|id| (*id).to_string()).collect()),
        Some(raw) => {
            let mut bots = Vec::new();
            for token in raw.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                bots.push(token.to_string());
            }
            if bots.is_empty() {
                return Err(anyhow!("--bots resolved to empty list"));
            }
            Ok(bots)
        }
    }
}

pub fn run_benchmark(config: BenchmarkConfig) -> Result<BenchmarkReport> {
    if config.seeds.is_empty() {
        return Err(anyhow!("benchmark requires at least one seed"));
    }
    if config.bots.is_empty() {
        return Err(anyhow!("benchmark requires at least one bot"));
    }
    fs::create_dir_all(&config.out_dir)
        .with_context(|| format!("failed creating {}", config.out_dir.display()))?;

    if let Some(jobs) = config.jobs {
        if jobs == 0 {
            return Err(anyhow!("benchmark --jobs must be >= 1 when provided"));
        }
    }

    let run_jobs: Vec<(String, u32)> = config
        .bots
        .iter()
        .flat_map(|bot| config.seeds.iter().map(move |seed| (bot.clone(), *seed)))
        .collect();

    let run_one = |(bot_id, seed): &(String, u32)| -> Result<InternalRun> {
        let artifact = run_bot(bot_id, *seed, config.max_frames)
            .with_context(|| format!("benchmark run failed for bot={bot_id} seed={seed:#x}"))?;
        let objective_value = config.objective.run_value(&artifact.metrics);
        Ok(InternalRun {
            metrics: artifact.metrics,
            objective_value,
            tape: artifact.tape,
        })
    };

    let run_results: Vec<Result<InternalRun>> = if let Some(jobs) = config.jobs {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .context("failed to build rayon threadpool")?;
        pool.install(|| run_jobs.par_iter().map(run_one).collect())
    } else {
        run_jobs.par_iter().map(run_one).collect()
    };

    let mut runs = Vec::with_capacity(run_results.len());
    for result in run_results {
        runs.push(result?);
    }

    let mut grouped: HashMap<String, Vec<&InternalRun>> = HashMap::new();
    for run in &runs {
        grouped
            .entry(run.metrics.bot_id.clone())
            .or_default()
            .push(run);
    }

    let mut rankings = Vec::new();
    for (bot_id, bot_runs) in grouped {
        let runs_count = bot_runs.len();
        let bot_fingerprint = bot_runs
            .first()
            .map(|r| r.metrics.bot_fingerprint.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let sum_score: u64 = bot_runs.iter().map(|r| r.metrics.final_score as u64).sum();
        let max_score: u32 = bot_runs
            .iter()
            .map(|r| r.metrics.final_score)
            .max()
            .unwrap_or_default();
        let sum_frames: u64 = bot_runs.iter().map(|r| r.metrics.frame_count as u64).sum();
        let max_frames: u32 = bot_runs
            .iter()
            .map(|r| r.metrics.frame_count)
            .max()
            .unwrap_or_default();
        let sum_lives: i64 = bot_runs.iter().map(|r| r.metrics.final_lives as i64).sum();
        let sum_action_frames: u64 = bot_runs
            .iter()
            .map(|r| r.metrics.action_frames as u64)
            .sum();
        let sum_turn_frames: u64 = bot_runs.iter().map(|r| r.metrics.turn_frames as u64).sum();
        let sum_thrust_frames: u64 = bot_runs
            .iter()
            .map(|r| r.metrics.thrust_frames as u64)
            .sum();
        let sum_fire_frames: u64 = bot_runs.iter().map(|r| r.metrics.fire_frames as u64).sum();
        let min_lives: i32 = bot_runs
            .iter()
            .map(|r| r.metrics.final_lives)
            .min()
            .unwrap_or_default();
        let survived_count = bot_runs
            .iter()
            .filter(|r| !r.metrics.game_over && r.metrics.frame_count >= config.max_frames)
            .count();
        let objective_value =
            bot_runs.iter().map(|r| r.objective_value).sum::<f64>() / runs_count as f64;

        rankings.push(BotAggregate {
            bot_id,
            bot_fingerprint,
            runs: runs_count,
            avg_score: sum_score as f64 / runs_count as f64,
            max_score,
            avg_frames: sum_frames as f64 / runs_count as f64,
            max_frames,
            avg_lives: sum_lives as f64 / runs_count as f64,
            min_lives,
            survival_rate: survived_count as f64 / runs_count as f64,
            objective_value,
            avg_action_frames: sum_action_frames as f64 / runs_count as f64,
            avg_turn_frames: sum_turn_frames as f64 / runs_count as f64,
            avg_thrust_frames: sum_thrust_frames as f64 / runs_count as f64,
            avg_fire_frames: sum_fire_frames as f64 / runs_count as f64,
        });
    }

    rankings.sort_by(|a, b| {
        b.objective_value
            .total_cmp(&a.objective_value)
            .then_with(|| b.avg_score.total_cmp(&a.avg_score))
            .then_with(|| b.avg_frames.total_cmp(&a.avg_frames))
    });

    let mut run_records: Vec<RunRecord> = runs
        .iter()
        .map(|run| RunRecord {
            bot_id: run.metrics.bot_id.clone(),
            bot_fingerprint: run.metrics.bot_fingerprint.clone(),
            seed: run.metrics.seed,
            seed_hex: seed_to_hex(run.metrics.seed),
            frame_count: run.metrics.frame_count,
            final_score: run.metrics.final_score,
            final_lives: run.metrics.final_lives,
            final_wave: run.metrics.final_wave,
            game_over: run.metrics.game_over,
            objective_value: run.objective_value,
            action_frames: run.metrics.action_frames,
            turn_frames: run.metrics.turn_frames,
            thrust_frames: run.metrics.thrust_frames,
            fire_frames: run.metrics.fire_frames,
        })
        .collect();

    run_records.sort_by(|a, b| {
        b.objective_value
            .total_cmp(&a.objective_value)
            .then_with(|| b.final_score.cmp(&a.final_score))
            .then_with(|| b.frame_count.cmp(&a.frame_count))
    });

    let mut saved_tapes = Vec::new();
    if config.save_top > 0 {
        save_top_tapes(
            &config.out_dir,
            &runs,
            "objective",
            config.save_top,
            |run| run.objective_value,
            &mut saved_tapes,
        )?;
        save_top_tapes(
            &config.out_dir,
            &runs,
            "score",
            config.save_top,
            |run| run.metrics.final_score as f64,
            &mut saved_tapes,
        )?;
        save_top_tapes(
            &config.out_dir,
            &runs,
            "survival",
            config.save_top,
            |run| run.metrics.frame_count as f64,
            &mut saved_tapes,
        )?;
    }

    write_runs_csv(&config.out_dir.join("runs.csv"), &run_records)?;
    write_rankings_csv(&config.out_dir.join("rankings.csv"), &rankings)?;

    let report = BenchmarkReport {
        generated_unix_s: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        objective: config.objective,
        max_frames: config.max_frames,
        jobs: config.jobs,
        bots: config.bots,
        seeds: config.seeds,
        run_count: run_records.len(),
        bot_rankings: rankings,
        runs: run_records,
        saved_tapes,
    };

    let report_path = config.out_dir.join("summary.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).context("failed to serialize summary json")?,
    )
    .with_context(|| format!("failed writing {}", report_path.display()))?;

    Ok(report)
}

fn save_top_tapes<F>(
    out_dir: &Path,
    runs: &[InternalRun],
    metric_name: &str,
    count: usize,
    metric: F,
    saved_tapes: &mut Vec<SavedTapeRecord>,
) -> Result<()>
where
    F: Fn(&InternalRun) -> f64,
{
    let mut order: Vec<&InternalRun> = runs.iter().collect();
    order.sort_by(|a, b| {
        metric(b)
            .total_cmp(&metric(a))
            .then_with(|| b.metrics.final_score.cmp(&a.metrics.final_score))
            .then_with(|| b.metrics.frame_count.cmp(&a.metrics.frame_count))
    });

    let save_dir = out_dir.join(format!("top-{metric_name}"));
    fs::create_dir_all(&save_dir)
        .with_context(|| format!("failed creating {}", save_dir.display()))?;

    for (idx, run) in order.into_iter().take(count).enumerate() {
        let rank = idx + 1;
        let safe_bot = run.metrics.bot_id.replace('_', "-");
        let base = format!(
            "rank{rank:02}-{safe_bot}-seed{:08x}-score{}-frames{}",
            run.metrics.seed, run.metrics.final_score, run.metrics.frame_count
        );
        let tape_path = save_dir.join(format!("{base}.tape"));
        fs::write(&tape_path, &run.tape)
            .with_context(|| format!("failed writing {}", tape_path.display()))?;

        let meta = serde_json::json!({
            "rank": rank,
            "metric": metric_name,
            "bot_id": run.metrics.bot_id,
            "bot_fingerprint": run.metrics.bot_fingerprint,
            "seed": run.metrics.seed,
            "seed_hex": seed_to_hex(run.metrics.seed),
            "max_frames": run.metrics.max_frames,
            "frame_count": run.metrics.frame_count,
            "final_score": run.metrics.final_score,
            "final_lives": run.metrics.final_lives,
            "final_wave": run.metrics.final_wave,
            "final_rng_state": run.metrics.final_rng_state,
            "game_over": run.metrics.game_over,
            "objective_value": run.objective_value,
            "action_frames": run.metrics.action_frames,
            "turn_frames": run.metrics.turn_frames,
            "thrust_frames": run.metrics.thrust_frames,
            "fire_frames": run.metrics.fire_frames,
        });
        let meta_path = save_dir.join(format!("{base}.json"));
        fs::write(
            &meta_path,
            serde_json::to_vec_pretty(&meta).context("failed to serialize top tape metadata")?,
        )
        .with_context(|| format!("failed writing {}", meta_path.display()))?;

        saved_tapes.push(SavedTapeRecord {
            rank,
            metric: metric_name.to_string(),
            bot_id: run.metrics.bot_id.clone(),
            bot_fingerprint: run.metrics.bot_fingerprint.clone(),
            seed: run.metrics.seed,
            seed_hex: seed_to_hex(run.metrics.seed),
            score: run.metrics.final_score,
            frames: run.metrics.frame_count,
            lives: run.metrics.final_lives,
            path: tape_path.to_string_lossy().into_owned(),
        });
    }

    Ok(())
}

fn write_runs_csv(path: &Path, rows: &[RunRecord]) -> Result<()> {
    let mut csv = String::from(
        "bot_id,bot_fingerprint,seed_hex,seed,frame_count,final_score,final_lives,final_wave,game_over,objective_value,action_frames,turn_frames,thrust_frames,fire_frames\n",
    );
    for row in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            row.bot_id,
            row.bot_fingerprint,
            row.seed_hex,
            row.seed,
            row.frame_count,
            row.final_score,
            row.final_lives,
            row.final_wave,
            row.game_over,
            row.objective_value,
            row.action_frames,
            row.turn_frames,
            row.thrust_frames,
            row.fire_frames
        ));
    }
    fs::write(path, csv).with_context(|| format!("failed writing {}", path.display()))
}

fn write_rankings_csv(path: &Path, rows: &[BotAggregate]) -> Result<()> {
    let mut csv = String::from(
        "rank,bot_id,bot_fingerprint,runs,avg_score,max_score,avg_frames,max_frames,avg_lives,min_lives,survival_rate,objective_value,avg_action_frames,avg_turn_frames,avg_thrust_frames,avg_fire_frames\n",
    );
    for (idx, row) in rows.iter().enumerate() {
        csv.push_str(&format!(
            "{},{},{},{},{:.2},{},{:.2},{},{:.2},{},{:.4},{:.4},{:.2},{:.2},{:.2},{:.2}\n",
            idx + 1,
            row.bot_id,
            row.bot_fingerprint,
            row.runs,
            row.avg_score,
            row.max_score,
            row.avg_frames,
            row.max_frames,
            row.avg_lives,
            row.min_lives,
            row.survival_rate,
            row.objective_value,
            row.avg_action_frames,
            row.avg_turn_frames,
            row.avg_thrust_frames,
            row.avg_fire_frames
        ));
    }
    fs::write(path, csv).with_context(|| format!("failed writing {}", path.display()))
}
