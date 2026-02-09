use crate::benchmark::BenchmarkReport;
use crate::runner::run_bot;
use anyhow::{anyhow, Context, Result};
use asteroids_verifier_core::constants::{
    SHIP_BULLET_LIFETIME_FRAMES, SHIP_BULLET_LIMIT, WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::{LiveGame, WorldSnapshot};
use asteroids_verifier_core::tape::decode_input_byte;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CONTEXT_WINDOW_FRAMES: usize = 180;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    Asteroid,
    Saucer,
    SaucerBullet,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShotOutcome {
    Hit,
    Miss,
    Unresolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionWindowSummary {
    pub window_frames: usize,
    pub idle_frames: u32,
    pub turn_frames: u32,
    pub thrust_frames: u32,
    pub fire_frames: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotRecord {
    pub shot_id: u32,
    pub fired_frame: u32,
    pub resolved_frame: Option<u32>,
    pub outcome: ShotOutcome,
    pub resolution_reason: String,
    pub score_delta_on_resolution: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeathEvent {
    pub frame: u32,
    pub lives_after: i32,
    pub wave: i32,
    pub score: u32,
    pub cause: DeathCause,
    pub cause_distance_px: f64,
    pub nearest_threat_px: f64,
    pub recent_avg_threat_px: f64,
    pub recent_min_threat_px: f64,
    pub recent_action_window: ActionWindowSummary,
    pub recent_shots_fired: u32,
    pub recent_shots_hit: u32,
    pub recent_shots_missed: u32,
    pub min_edge_distance_px: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotSummary {
    pub total_fired: u32,
    pub total_hit: u32,
    pub total_miss: u32,
    pub total_unresolved: u32,
    pub hit_rate: f64,
    pub miss_rate: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunIntel {
    pub bot_id: String,
    pub seed: u32,
    pub max_frames: u32,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_lives: i32,
    pub final_wave: i32,
    pub game_over: bool,
    pub action_counts: ActionWindowSummary,
    pub shots: Vec<ShotRecord>,
    pub shot_summary: ShotSummary,
    pub deaths: Vec<DeathEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkIntelReport {
    pub generated_unix_s: u64,
    pub source_summary: String,
    pub source_max_frames: u32,
    pub codex_only: bool,
    pub run_count: usize,
    pub runs: Vec<RunIntel>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AdaptiveProfile {
    pub risk_weight_scale: f64,
    pub survival_weight_scale: f64,
    pub aggression_weight_scale: f64,
    pub fire_reward_scale: f64,
    pub shot_penalty_scale: f64,
    pub miss_fire_penalty_scale: f64,
    pub min_fire_quality_delta: f64,
    pub action_penalty_scale: f64,
    pub turn_penalty_scale: f64,
    pub thrust_penalty_scale: f64,
    pub center_weight_scale: f64,
    pub edge_penalty_scale: f64,
    pub lookahead_frames_scale: f64,
    pub flow_weight_scale: f64,
    pub speed_soft_cap_scale: f64,
    pub fire_distance_scale: f64,
    pub lurk_trigger_scale: f64,
    pub lurk_boost_scale: f64,
    pub fire_tolerance_scale: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearningRecommendation {
    pub generated_unix_s: u64,
    pub source_summary: String,
    pub runs_considered: usize,
    pub total_frames: u64,
    pub total_deaths: u32,
    pub deaths_per_10k_frames: f64,
    pub death_causes: BTreeMap<String, u32>,
    pub edge_death_ratio: f64,
    pub avg_recent_min_threat_px_on_death: f64,
    pub avg_turn_ratio_on_death: f64,
    pub avg_thrust_ratio_on_death: f64,
    pub total_shots: u32,
    pub shot_hits: u32,
    pub shot_misses: u32,
    pub shot_unresolved: u32,
    pub hit_rate: f64,
    pub miss_rate: f64,
    pub profile: AdaptiveProfile,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearningCycleResult {
    pub intel_report_path: String,
    pub recommendation_path: String,
    pub adaptive_profile_path: String,
    pub recommendation: LearningRecommendation,
}

#[derive(Clone, Debug)]
struct ThreatSample {
    cause: DeathCause,
    distance_px: f64,
}

pub fn collect_run_intel(bot_id: &str, seed: u32, max_frames: u32) -> Result<RunIntel> {
    let artifact = run_bot(bot_id, seed, max_frames)
        .with_context(|| format!("failed running bot={bot_id} seed={seed:#010x}"))?;
    analyze_inputs(bot_id, seed, max_frames, &artifact.inputs)
}

pub fn analyze_inputs(bot_id: &str, seed: u32, max_frames: u32, inputs: &[u8]) -> Result<RunIntel> {
    let mut game = LiveGame::new(seed);
    game.validate()
        .map_err(|rule| anyhow!("initial invariant failure for intel replay: {rule:?}"))?;

    let mut snapshot = game.snapshot();
    let mut action_history: Vec<u8> = Vec::with_capacity(inputs.len());
    let mut threat_history: Vec<f64> = Vec::with_capacity(inputs.len());

    let mut shots = Vec::<ShotRecord>::new();
    let mut unresolved_shots = VecDeque::<usize>::new();

    let mut next_shot_id: u32 = 1;
    let mut deaths = Vec::<DeathEvent>::new();

    for (idx, input_byte) in inputs.iter().copied().enumerate() {
        let before = snapshot.clone();
        let frame_number = before.frame_count + 1;
        action_history.push(input_byte);

        let nearest_before = nearest_threat_distance_px(&before);
        threat_history.push(nearest_before);

        let decoded = decode_input_byte(input_byte);
        let fired_now = decoded.fire
            && before.ship.can_control
            && !before.is_game_over
            && before.ship.fire_cooldown <= 0
            && before.bullets.len() < SHIP_BULLET_LIMIT;

        if fired_now {
            shots.push(ShotRecord {
                shot_id: next_shot_id,
                fired_frame: frame_number,
                resolved_frame: None,
                outcome: ShotOutcome::Unresolved,
                resolution_reason: "pending".to_string(),
                score_delta_on_resolution: 0,
            });
            unresolved_shots.push_back(shots.len() - 1);
            next_shot_id = next_shot_id.saturating_add(1);
        }

        game.step(input_byte);
        snapshot = game.snapshot();

        let score_delta = snapshot.score.saturating_sub(before.score);
        let mut inferred_hits = estimate_hit_events(score_delta);
        while inferred_hits > 0 {
            if let Some(shot_index) = unresolved_shots.pop_front() {
                let shot = &mut shots[shot_index];
                shot.resolved_frame = Some(frame_number);
                shot.outcome = ShotOutcome::Hit;
                shot.resolution_reason = "inferred_score_event".to_string();
                shot.score_delta_on_resolution = score_delta;
            }
            inferred_hits -= 1;
        }

        // Reconcile unresolved shots with live in-world bullet count.
        while unresolved_shots.len() > snapshot.bullets.len() {
            let Some(shot_index) = unresolved_shots.pop_front() else {
                break;
            };
            let shot = &mut shots[shot_index];
            shot.resolved_frame = Some(frame_number);
            shot.outcome = ShotOutcome::Miss;
            shot.resolution_reason = "bullet_disappeared_without_score".to_string();
        }

        if snapshot.lives < before.lives {
            let killer = probable_killer(&before);
            let recent_actions = summarize_action_window(&action_history, action_history.len(), CONTEXT_WINDOW_FRAMES);
            let (recent_avg_threat, recent_min_threat) = summarize_recent_threats(&threat_history);
            let (recent_shots_fired, recent_shots_hit, recent_shots_missed) =
                summarize_recent_shots(&shots, frame_number, CONTEXT_WINDOW_FRAMES as u32);

            deaths.push(DeathEvent {
                frame: frame_number,
                lives_after: snapshot.lives,
                wave: snapshot.wave,
                score: snapshot.score,
                cause: killer.cause,
                cause_distance_px: killer.distance_px,
                nearest_threat_px: nearest_before,
                recent_avg_threat_px: recent_avg_threat,
                recent_min_threat_px: recent_min_threat,
                recent_action_window: recent_actions,
                recent_shots_fired,
                recent_shots_hit,
                recent_shots_missed,
                min_edge_distance_px: min_edge_distance_px(&before),
            });
        }

        // Time out unresolved shots if they have definitely exceeded lifetime.
        while let Some(front) = unresolved_shots.front().copied() {
            let age = frame_number.saturating_sub(shots[front].fired_frame);
            if age <= SHIP_BULLET_LIFETIME_FRAMES as u32 + 2 {
                break;
            }
            unresolved_shots.pop_front();
            let shot = &mut shots[front];
            shot.resolved_frame = Some(frame_number);
            shot.outcome = ShotOutcome::Miss;
            shot.resolution_reason = "shot_timed_out".to_string();
        }

        if idx + 1 >= max_frames as usize {
            break;
        }
    }

    for shot_index in unresolved_shots {
        let shot = &mut shots[shot_index];
        shot.outcome = ShotOutcome::Unresolved;
        shot.resolution_reason = "run_ended_before_resolution".to_string();
    }

    let final_actions = summarize_action_window(&action_history, action_history.len(), action_history.len());

    let mut total_hit = 0u32;
    let mut total_miss = 0u32;
    let mut total_unresolved = 0u32;
    for shot in &shots {
        match shot.outcome {
            ShotOutcome::Hit => total_hit += 1,
            ShotOutcome::Miss => total_miss += 1,
            ShotOutcome::Unresolved => total_unresolved += 1,
        }
    }
    let total_fired = shots.len() as u32;
    let hit_rate = if total_fired == 0 {
        0.0
    } else {
        total_hit as f64 / total_fired as f64
    };
    let miss_rate = if total_fired == 0 {
        0.0
    } else {
        total_miss as f64 / total_fired as f64
    };

    Ok(RunIntel {
        bot_id: bot_id.to_string(),
        seed,
        max_frames,
        frame_count: snapshot.frame_count,
        final_score: snapshot.score,
        final_lives: snapshot.lives,
        final_wave: snapshot.wave,
        game_over: snapshot.is_game_over,
        action_counts: final_actions,
        shots,
        shot_summary: ShotSummary {
            total_fired,
            total_hit,
            total_miss,
            total_unresolved,
            hit_rate,
            miss_rate,
        },
        deaths,
    })
}

pub fn analyze_benchmark_summary(
    summary_path: &Path,
    codex_only: bool,
) -> Result<BenchmarkIntelReport> {
    let raw = fs::read(summary_path)
        .with_context(|| format!("failed reading summary {}", summary_path.display()))?;
    let report: BenchmarkReport = serde_json::from_slice(&raw)
        .with_context(|| format!("invalid benchmark summary {}", summary_path.display()))?;

    let mut runs = Vec::new();
    for run in &report.runs {
        if codex_only && !run.bot_id.starts_with("codex-") {
            continue;
        }
        let intel = collect_run_intel(&run.bot_id, run.seed, report.max_frames)
            .with_context(|| format!("intel collection failed for bot={} seed={:#010x}", run.bot_id, run.seed))?;
        runs.push(intel);
    }

    Ok(BenchmarkIntelReport {
        generated_unix_s: now_unix_s(),
        source_summary: summary_path.display().to_string(),
        source_max_frames: report.max_frames,
        codex_only,
        run_count: runs.len(),
        runs,
    })
}

pub fn derive_learning_recommendation(intel: &BenchmarkIntelReport) -> LearningRecommendation {
    let mut total_frames: u64 = 0;
    let mut total_deaths: u32 = 0;

    let mut death_causes = BTreeMap::<String, u32>::new();
    death_causes.insert("asteroid".to_string(), 0);
    death_causes.insert("saucer".to_string(), 0);
    death_causes.insert("saucer_bullet".to_string(), 0);
    death_causes.insert("unknown".to_string(), 0);

    let mut edge_deaths = 0u32;
    let mut sum_recent_min_threat = 0.0;
    let mut sum_turn_ratio = 0.0;
    let mut sum_thrust_ratio = 0.0;
    let mut death_samples = 0u32;

    let mut total_shots = 0u32;
    let mut shot_hits = 0u32;
    let mut shot_misses = 0u32;
    let mut shot_unresolved = 0u32;

    for run in &intel.runs {
        total_frames = total_frames.saturating_add(run.frame_count as u64);
        total_deaths = total_deaths.saturating_add(run.deaths.len() as u32);

        total_shots = total_shots.saturating_add(run.shot_summary.total_fired);
        shot_hits = shot_hits.saturating_add(run.shot_summary.total_hit);
        shot_misses = shot_misses.saturating_add(run.shot_summary.total_miss);
        shot_unresolved = shot_unresolved.saturating_add(run.shot_summary.total_unresolved);

        for death in &run.deaths {
            death_samples = death_samples.saturating_add(1);
            sum_recent_min_threat += death.recent_min_threat_px;

            let window = death.recent_action_window.window_frames.max(1) as f64;
            sum_turn_ratio += death.recent_action_window.turn_frames as f64 / window;
            sum_thrust_ratio += death.recent_action_window.thrust_frames as f64 / window;

            if death.min_edge_distance_px < 110.0 {
                edge_deaths = edge_deaths.saturating_add(1);
            }

            let key = match death.cause {
                DeathCause::Asteroid => "asteroid",
                DeathCause::Saucer => "saucer",
                DeathCause::SaucerBullet => "saucer_bullet",
                DeathCause::Unknown => "unknown",
            }
            .to_string();
            *death_causes.entry(key).or_insert(0) += 1;
        }
    }

    let deaths_per_10k = if total_frames == 0 {
        0.0
    } else {
        (total_deaths as f64 / total_frames as f64) * 10_000.0
    };

    let avg_recent_min_threat = if death_samples == 0 {
        0.0
    } else {
        sum_recent_min_threat / death_samples as f64
    };
    let avg_turn_ratio = if death_samples == 0 {
        0.0
    } else {
        sum_turn_ratio / death_samples as f64
    };
    let avg_thrust_ratio = if death_samples == 0 {
        0.0
    } else {
        sum_thrust_ratio / death_samples as f64
    };

    let hit_rate = if total_shots == 0 {
        0.0
    } else {
        shot_hits as f64 / total_shots as f64
    };
    let miss_rate = if total_shots == 0 {
        0.0
    } else {
        shot_misses as f64 / total_shots as f64
    };

    let edge_death_ratio = if total_deaths == 0 {
        0.0
    } else {
        edge_deaths as f64 / total_deaths as f64
    };

    let asteroid_ratio = ratio_from_map(&death_causes, "asteroid", total_deaths);
    let saucer_ratio = ratio_from_map(&death_causes, "saucer", total_deaths);
    let bullet_ratio = ratio_from_map(&death_causes, "saucer_bullet", total_deaths);

    let panic_factor = (deaths_per_10k / 1.8).clamp(0.0, 1.6);
    let risk_weight_scale = (1.0 + 0.22 * panic_factor + 0.12 * bullet_ratio + 0.08 * asteroid_ratio)
        .clamp(0.75, 1.9);
    let survival_weight_scale = (1.0 + 0.28 * panic_factor + 0.1 * bullet_ratio).clamp(0.8, 2.0);
    let aggression_weight_scale =
        (1.0 - 0.2 * panic_factor + 0.16 * (hit_rate - 0.5)).clamp(0.55, 1.4);
    let fire_reward_scale = (1.0 + (hit_rate - miss_rate) * 0.3).clamp(0.7, 1.45);
    let shot_penalty_scale = (1.0 + (miss_rate - 0.45).max(0.0) * 0.85).clamp(0.8, 1.9);
    let miss_fire_penalty_scale = (1.0 + (miss_rate - 0.42).max(0.0)).clamp(0.8, 2.0);
    let min_fire_quality_delta = ((miss_rate - 0.5) * 0.2 - bullet_ratio * 0.03).clamp(-0.08, 0.14);
    let action_penalty_scale = if panic_factor > 0.8 { 0.92 } else { 1.0 };
    let turn_penalty_scale = if avg_turn_ratio < 0.17 { 0.86 } else { 0.97 };
    let thrust_penalty_scale = if avg_thrust_ratio < 0.14 { 0.88 } else { 0.98 };
    let center_weight_scale = (1.0 + edge_death_ratio * 0.4).clamp(0.9, 1.9);
    let edge_penalty_scale = (1.0 + edge_death_ratio * 0.55).clamp(0.9, 2.0);

    let profile = AdaptiveProfile {
        risk_weight_scale,
        survival_weight_scale,
        aggression_weight_scale,
        fire_reward_scale,
        shot_penalty_scale,
        miss_fire_penalty_scale,
        min_fire_quality_delta,
        action_penalty_scale,
        turn_penalty_scale,
        thrust_penalty_scale,
        center_weight_scale,
        edge_penalty_scale,
        lookahead_frames_scale: 1.0,
        flow_weight_scale: 1.0,
        speed_soft_cap_scale: 1.0,
        fire_distance_scale: 1.0,
        lurk_trigger_scale: 1.0,
        lurk_boost_scale: 1.0,
        fire_tolerance_scale: 1.0,
    };

    let mut notes = Vec::new();
    notes.push(format!(
        "death pressure {:.2} per 10k frames, causes asteroid {:.1}%, saucer {:.1}%, saucer_bullet {:.1}%",
        deaths_per_10k,
        asteroid_ratio * 100.0,
        saucer_ratio * 100.0,
        bullet_ratio * 100.0
    ));
    notes.push(format!(
        "shot quality hit {:.1}% miss {:.1}% unresolved {}",
        hit_rate * 100.0,
        miss_rate * 100.0,
        shot_unresolved
    ));
    notes.push(format!(
        "edge death ratio {:.1}% -> center/edge weights scaled to {:.2}/{:.2}",
        edge_death_ratio * 100.0,
        center_weight_scale,
        edge_penalty_scale
    ));
    notes.push(format!(
        "avg turn/thrust ratio at death {:.3}/{:.3} -> penalty scales {:.2}/{:.2}",
        avg_turn_ratio, avg_thrust_ratio, turn_penalty_scale, thrust_penalty_scale
    ));
    if avg_recent_min_threat > 0.0 {
        notes.push(format!(
            "average recent minimum threat distance before death: {:.1}px",
            avg_recent_min_threat
        ));
    }

    LearningRecommendation {
        generated_unix_s: now_unix_s(),
        source_summary: intel.source_summary.clone(),
        runs_considered: intel.runs.len(),
        total_frames,
        total_deaths,
        deaths_per_10k_frames: deaths_per_10k,
        death_causes,
        edge_death_ratio,
        avg_recent_min_threat_px_on_death: avg_recent_min_threat,
        avg_turn_ratio_on_death: avg_turn_ratio,
        avg_thrust_ratio_on_death: avg_thrust_ratio,
        total_shots,
        shot_hits,
        shot_misses,
        shot_unresolved,
        hit_rate,
        miss_rate,
        profile,
        notes,
    }
}

pub fn run_learning_cycle(
    summary_path: &Path,
    output_dir: &Path,
    codex_only: bool,
) -> Result<LearningCycleResult> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed creating {}", output_dir.display()))?;

    let intel = analyze_benchmark_summary(summary_path, codex_only)?;
    let recommendation = derive_learning_recommendation(&intel);

    let intel_report_path = output_dir.join("intel-report.json");
    let recommendation_path = output_dir.join("adaptive-learning.json");
    let adaptive_profile_path = output_dir.join("adaptive-profile.json");

    write_json(&intel_report_path, &intel)?;
    write_json(&recommendation_path, &recommendation)?;
    write_json(&adaptive_profile_path, &recommendation.profile)?;

    Ok(LearningCycleResult {
        intel_report_path: intel_report_path.display().to_string(),
        recommendation_path: recommendation_path.display().to_string(),
        adaptive_profile_path: adaptive_profile_path.display().to_string(),
        recommendation,
    })
}

fn summarize_action_window(actions: &[u8], end_exclusive: usize, window: usize) -> ActionWindowSummary {
    let start = end_exclusive.saturating_sub(window);
    let mut summary = ActionWindowSummary {
        window_frames: end_exclusive.saturating_sub(start),
        idle_frames: 0,
        turn_frames: 0,
        thrust_frames: 0,
        fire_frames: 0,
    };

    for action in &actions[start..end_exclusive] {
        if *action == 0 {
            summary.idle_frames += 1;
        }
        if (action & 0x01) != 0 || (action & 0x02) != 0 {
            summary.turn_frames += 1;
        }
        if (action & 0x04) != 0 {
            summary.thrust_frames += 1;
        }
        if (action & 0x08) != 0 {
            summary.fire_frames += 1;
        }
    }

    summary
}

fn summarize_recent_threats(threat_history: &[f64]) -> (f64, f64) {
    if threat_history.is_empty() {
        return (0.0, 0.0);
    }

    let start = threat_history.len().saturating_sub(CONTEXT_WINDOW_FRAMES);
    let window = &threat_history[start..];
    let sum: f64 = window.iter().sum();
    let min = window
        .iter()
        .copied()
        .fold(f64::MAX, |acc, value| acc.min(value));

    (sum / window.len() as f64, min)
}

fn summarize_recent_shots(shots: &[ShotRecord], frame: u32, window: u32) -> (u32, u32, u32) {
    let start = frame.saturating_sub(window);
    let mut fired = 0u32;
    let mut hit = 0u32;
    let mut miss = 0u32;

    for shot in shots {
        if shot.fired_frame < start || shot.fired_frame > frame {
            continue;
        }
        fired += 1;
        match shot.outcome {
            ShotOutcome::Hit => hit += 1,
            ShotOutcome::Miss => miss += 1,
            ShotOutcome::Unresolved => {}
        }
    }

    (fired, hit, miss)
}

fn probable_killer(world: &WorldSnapshot) -> ThreatSample {
    let ship_x = world.ship.x;
    let ship_y = world.ship.y;
    let ship_radius = world.ship.radius as f64 / 16.0;

    let mut best = ThreatSample {
        cause: DeathCause::Unknown,
        distance_px: f64::MAX,
    };

    let mut consider = |x: i32, y: i32, radius_q12_4: i32, cause: DeathCause| {
        let dx = shortest_delta_q12_4(ship_x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let dy = shortest_delta_q12_4(ship_y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let dist = (dx * dx + dy * dy).sqrt();
        let threshold = ship_radius + radius_q12_4 as f64 / 16.0;
        let gap = dist - threshold;
        if gap < best.distance_px {
            best = ThreatSample {
                cause,
                distance_px: gap,
            };
        }
    };

    for asteroid in &world.asteroids {
        consider(asteroid.x, asteroid.y, asteroid.radius, DeathCause::Asteroid);
    }
    for saucer in &world.saucers {
        consider(saucer.x, saucer.y, saucer.radius, DeathCause::Saucer);
    }
    for bullet in &world.saucer_bullets {
        consider(bullet.x, bullet.y, bullet.radius, DeathCause::SaucerBullet);
    }

    if !best.distance_px.is_finite() {
        ThreatSample {
            cause: DeathCause::Unknown,
            distance_px: 9_999.0,
        }
    } else {
        best
    }
}

fn nearest_threat_distance_px(world: &WorldSnapshot) -> f64 {
    let ship_x = world.ship.x;
    let ship_y = world.ship.y;
    let mut best = f64::MAX;

    let mut consider = |x: i32, y: i32| {
        let dx = shortest_delta_q12_4(ship_x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let dy = shortest_delta_q12_4(ship_y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        best = best.min((dx * dx + dy * dy).sqrt());
    };

    for asteroid in &world.asteroids {
        consider(asteroid.x, asteroid.y);
    }
    for saucer in &world.saucers {
        consider(saucer.x, saucer.y);
    }
    for bullet in &world.saucer_bullets {
        consider(bullet.x, bullet.y);
    }

    if best == f64::MAX {
        9_999.0
    } else {
        best
    }
}

fn min_edge_distance_px(world: &WorldSnapshot) -> f64 {
    let left = world.ship.x as f64 / 16.0;
    let right = (WORLD_WIDTH_Q12_4 - world.ship.x) as f64 / 16.0;
    let top = world.ship.y as f64 / 16.0;
    let bottom = (WORLD_HEIGHT_Q12_4 - world.ship.y) as f64 / 16.0;
    left.min(right).min(top).min(bottom)
}

fn estimate_hit_events(score_delta: u32) -> u32 {
    if score_delta == 0 {
        return 0;
    }

    const EVENTS: [u32; 5] = [20, 50, 100, 200, 1000];
    for count in 1..=4u32 {
        if can_make_score_delta(score_delta, count as usize, &EVENTS) {
            return count;
        }
    }

    1
}

fn can_make_score_delta(delta: u32, depth: usize, events: &[u32]) -> bool {
    if depth == 0 {
        return delta == 0;
    }

    for event in events {
        if *event > delta {
            continue;
        }
        if can_make_score_delta(delta - *event, depth - 1, events) {
            return true;
        }
    }

    false
}

fn ratio_from_map(map: &BTreeMap<String, u32>, key: &str, total: u32) -> f64 {
    if total == 0 {
        return 0.0;
    }
    map.get(key).copied().unwrap_or(0) as f64 / total as f64
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let encoded = serde_json::to_vec_pretty(value)?;
    fs::write(path, encoded).with_context(|| format!("failed writing {}", path.display()))
}

fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn default_codex_output_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/codex-/state"))
}
