use crate::analysis::{analyze_run, DeathCause, RunAnalysis};
use crate::bot::Bot;
use crate::config::BotConfig;
use crate::runner;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeathAnalysis {
    pub total_deaths: u32,
    pub deaths_per_10k_frames: f64,
    pub cause_breakdown: BTreeMap<String, u32>,
    pub edge_death_ratio: f64,
    pub bullet_death_ratio: f64,
    pub avg_wave_at_death: f64,
    pub insights: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotAnalysis {
    pub total_fired: u32,
    pub hit_rate: f64,
    pub miss_rate: f64,
    pub avg_frames_between_shots: f64,
    pub insights: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionReport {
    pub generated_unix_s: u64,
    pub parent_id: String,
    pub generation: u32,
    pub runs_analyzed: usize,
    pub avg_score: f64,
    pub avg_frames: f64,
    pub max_score: u32,
    pub death_analysis: DeathAnalysis,
    pub shot_analysis: ShotAnalysis,
    pub param_adjustments: Vec<String>,
    pub evolved_config: BotConfig,
}

pub fn evolve(parent: &BotConfig, analyses: &[RunAnalysis]) -> EvolutionReport {
    let generation = parent.generation + 1;
    let total_runs = analyses.len();
    let total_frames: u64 = analyses.iter().map(|a| a.metrics.frame_count as u64).sum();
    let total_score: u64 = analyses.iter().map(|a| a.metrics.final_score as u64).sum();
    let max_score = analyses.iter().map(|a| a.metrics.final_score).max().unwrap_or(0);
    let avg_score = if total_runs == 0 { 0.0 } else { total_score as f64 / total_runs as f64 };
    let avg_frames = if total_runs == 0 { 0.0 } else { total_frames as f64 / total_runs as f64 };

    // Death analysis
    let mut total_deaths = 0u32;
    let mut cause_map = BTreeMap::new();
    let mut edge_deaths = 0u32;
    let mut bullet_deaths = 0u32;
    let mut sum_wave_at_death = 0.0;
    let mut death_count = 0u32;

    for analysis in analyses {
        for death in &analysis.deaths {
            total_deaths += 1;
            death_count += 1;
            sum_wave_at_death += death.wave as f64;

            if death.min_edge_distance_px < 110.0 {
                edge_deaths += 1;
            }

            let cause_key = match death.cause {
                DeathCause::Asteroid => "asteroid",
                DeathCause::Saucer => "saucer",
                DeathCause::SaucerBullet => {
                    bullet_deaths += 1;
                    "saucer_bullet"
                }
                DeathCause::Unknown => "unknown",
            };
            *cause_map.entry(cause_key.to_string()).or_insert(0u32) += 1;
        }
    }

    let deaths_per_10k = if total_frames == 0 {
        0.0
    } else {
        (total_deaths as f64 / total_frames as f64) * 10_000.0
    };
    let edge_death_ratio = if total_deaths == 0 { 0.0 } else { edge_deaths as f64 / total_deaths as f64 };
    let bullet_death_ratio = if total_deaths == 0 { 0.0 } else { bullet_deaths as f64 / total_deaths as f64 };
    let avg_wave_at_death = if death_count == 0 { 0.0 } else { sum_wave_at_death / death_count as f64 };

    let mut death_insights = Vec::new();
    if bullet_death_ratio > 0.35 {
        death_insights.push(format!("High saucer bullet death rate ({:.0}%): increase bullet risk weight.", bullet_death_ratio * 100.0));
    }
    if edge_death_ratio > 0.25 {
        death_insights.push(format!("Edge deaths ({:.0}%): increase edge penalty and center weight.", edge_death_ratio * 100.0));
    }
    if deaths_per_10k > 5.0 {
        death_insights.push(format!("High death rate ({:.1}/10k frames): global risk increase needed.", deaths_per_10k));
    }

    // Shot analysis
    let mut total_fired = 0u32;
    let mut total_hits = 0u32;
    let mut total_misses = 0u32;

    for analysis in analyses {
        total_fired += analysis.shot_summary.total_fired;
        total_hits += analysis.shot_summary.total_hit;
        total_misses += analysis.shot_summary.total_miss;
    }

    let hit_rate = if total_fired == 0 { 0.0 } else { total_hits as f64 / total_fired as f64 };
    let miss_rate = if total_fired == 0 { 0.0 } else { total_misses as f64 / total_fired as f64 };
    let avg_frames_between_shots = if total_fired == 0 { 0.0 } else { total_frames as f64 / total_fired as f64 };

    let mut shot_insights = Vec::new();
    if miss_rate > 0.5 {
        shot_insights.push(format!("High miss rate ({:.0}%): increase fire quality floor.", miss_rate * 100.0));
    }
    if hit_rate > 0.55 {
        shot_insights.push(format!("Good hit rate ({:.0}%): can be more aggressive.", hit_rate * 100.0));
    }

    // Evolve parameters
    let mut cfg = parent.clone();
    cfg.generation = generation;
    cfg.parent_id = parent.id.clone();
    cfg.id = format!("claude-evolved-gen{generation}");
    cfg.description = format!(
        "Generation {generation} evolved from {} (avg={:.0}, deaths/10k={:.1}, hit={:.0}%)",
        parent.id, avg_score, deaths_per_10k, hit_rate * 100.0
    );

    let mut adjustments = Vec::new();

    // Death-driven adjustments
    if deaths_per_10k > 4.0 {
        let scale = (deaths_per_10k / 4.0).min(1.5);
        cfg.risk_weight_asteroid *= 1.0 + 0.08 * scale;
        cfg.risk_weight_saucer *= 1.0 + 0.1 * scale;
        cfg.risk_weight_bullet *= 1.0 + 0.12 * scale;
        cfg.survival_weight *= 1.0 + 0.1 * scale;
        adjustments.push(format!("Increased risk/survival weights by {:.0}% (high death rate)", scale * 10.0));
    } else if deaths_per_10k < 2.0 && avg_score < 30000.0 {
        cfg.aggression *= 1.08;
        cfg.fire_reward *= 1.06;
        cfg.survival_weight *= 0.96;
        adjustments.push("Slightly increased aggression (low death rate, room for scoring)".to_string());
    }

    if bullet_death_ratio > 0.35 {
        cfg.risk_weight_bullet *= 1.15;
        // Gentle saucer urgency tuning — small increments
        if cfg.saucer_kill_urgency < 0.15 {
            cfg.saucer_kill_urgency += 0.03;
        }
        adjustments.push(format!("Increased bullet risk weight (bullet deaths {:.0}%)", bullet_death_ratio * 100.0));
    }

    if edge_death_ratio > 0.25 {
        cfg.center_weight *= 1.12;
        cfg.edge_penalty *= 1.15;
        adjustments.push(format!("Increased center/edge weights (edge deaths {:.0}%)", edge_death_ratio * 100.0));
    }

    // Shot quality adjustments
    if miss_rate > 0.5 {
        cfg.min_fire_quality += 0.03;
        cfg.shot_penalty *= 1.1;
        cfg.miss_fire_penalty *= 1.1;
        adjustments.push(format!("Raised fire quality floor and penalties (miss rate {:.0}%)", miss_rate * 100.0));
    } else if hit_rate > 0.55 {
        cfg.min_fire_quality -= 0.02;
        cfg.fire_reward *= 1.05;
        adjustments.push(format!("Lowered fire quality floor (good hit rate {:.0}%)", hit_rate * 100.0));
    }

    if avg_frames_between_shots < 12.0 {
        cfg.shot_penalty *= 1.15;
        adjustments.push("Increased shot penalty (over-firing)".to_string());
    }

    // Speed management for late-wave deaths
    if avg_wave_at_death > 5.0 {
        cfg.speed_soft_cap *= 0.95;
        cfg.lookahead += 1.0;
        adjustments.push(format!("Reduced speed cap and increased lookahead (late-wave deaths, avg wave {:.1})", avg_wave_at_death));
    }

    cfg.clamp();

    EvolutionReport {
        generated_unix_s: now_unix_s(),
        parent_id: parent.id.clone(),
        generation,
        runs_analyzed: total_runs,
        avg_score,
        avg_frames,
        max_score,
        death_analysis: DeathAnalysis {
            total_deaths,
            deaths_per_10k_frames: deaths_per_10k,
            cause_breakdown: cause_map,
            edge_death_ratio,
            bullet_death_ratio,
            avg_wave_at_death,
            insights: death_insights,
        },
        shot_analysis: ShotAnalysis {
            total_fired,
            hit_rate,
            miss_rate,
            avg_frames_between_shots,
            insights: shot_insights,
        },
        param_adjustments: adjustments,
        evolved_config: cfg,
    }
}

pub fn run_evolution(
    initial_config: &BotConfig,
    seeds: &[u32],
    max_frames: u32,
    generations: u32,
    output_dir: &Path,
) -> Result<Vec<EvolutionReport>> {
    let mut reports = Vec::new();
    let mut current_config = initial_config.clone();

    for gen in 0..generations {
        let gen_dir = output_dir.join(format!("gen-{gen}"));
        fs::create_dir_all(&gen_dir)
            .with_context(|| format!("failed creating {}", gen_dir.display()))?;

        eprintln!(
            "=== Evolution generation {}/{generations}: {} ===",
            gen + 1,
            current_config.id
        );

        // Run on all seeds and collect analyses
        let mut analyses = Vec::new();
        for &seed in seeds {
            let mut bot = Bot::new(current_config.clone());
            let artifact = runner::run(&mut bot, seed, max_frames)
                .with_context(|| format!("failed running seed={seed:#010x}"))?;
            let analysis = analyze_run(artifact.metrics, &artifact.inputs, seed, max_frames)
                .with_context(|| format!("failed analyzing seed={seed:#010x}"))?;
            analyses.push(analysis);
        }

        let report = evolve(&current_config, &analyses);

        eprintln!(
            "  avg_score={:.0}, max_score={}, deaths/10k={:.1}, hit_rate={:.0}%",
            report.avg_score,
            report.max_score,
            report.death_analysis.deaths_per_10k_frames,
            report.shot_analysis.hit_rate * 100.0
        );
        for adj in &report.param_adjustments {
            eprintln!("  adjustment: {adj}");
        }

        // Save config + report
        let config_path = gen_dir.join(format!("{}.json", report.evolved_config.id));
        let report_path = gen_dir.join(format!("evolution-report-gen{}.json", report.generation));

        fs::write(&config_path, serde_json::to_vec_pretty(&report.evolved_config)?)?;
        fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;

        current_config = report.evolved_config.clone();
        reports.push(report);
    }

    Ok(reports)
}

fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
