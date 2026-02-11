//! claude-lab: Iterative learning and evolution system.
//!
//! Records death events and missed shots, analyzes patterns, and evolves
//! improved bot configurations. Uses the existing codex_lab telemetry
//! infrastructure but adds claude-specific evolution logic.

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use crate::codex_lab::{analyze_inputs, DeathCause, RunIntel};
use crate::runner::run_bot_instance;
use anyhow::{Context, Result};
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Evolved bot configuration ───────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolvedConfig {
    pub id: String,
    pub generation: u32,
    pub parent_id: String,
    pub description: String,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,
    pub aggression: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub min_fire_quality: f64,
    pub speed_soft_cap: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub lookahead: f64,
    pub lurk_trigger: i32,
    pub lurk_boost: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
}

impl Default for EvolvedConfig {
    fn default() -> Self {
        Self {
            id: "claude-evolved-gen0".to_string(),
            generation: 0,
            parent_id: "claude-chimera".to_string(),
            description: "Base evolved config from chimera template.".to_string(),
            risk_weight_asteroid: 1.35,
            risk_weight_saucer: 2.0,
            risk_weight_bullet: 2.7,
            survival_weight: 2.0,
            aggression: 0.82,
            fire_reward: 1.15,
            shot_penalty: 0.8,
            miss_fire_penalty: 1.0,
            min_fire_quality: 0.17,
            speed_soft_cap: 4.3,
            center_weight: 0.48,
            edge_penalty: 0.32,
            lookahead: 20.0,
            lurk_trigger: 290,
            lurk_boost: 1.7,
            action_penalty: 0.012,
            turn_penalty: 0.014,
            thrust_penalty: 0.013,
        }
    }
}

// ── Death analysis report ───────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeathAnalysis {
    pub total_deaths: u32,
    pub deaths_per_10k_frames: f64,
    pub cause_breakdown: BTreeMap<String, u32>,
    pub avg_nearest_threat_at_death: f64,
    pub avg_min_edge_at_death: f64,
    pub edge_death_ratio: f64,
    pub bullet_death_ratio: f64,
    pub saucer_death_ratio: f64,
    pub asteroid_death_ratio: f64,
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
    pub evolved_config: EvolvedConfig,
}

// ── Evolution engine ────────────────────────────────────────────────

pub fn evolve_from_intel(parent_config: &EvolvedConfig, runs: &[RunIntel]) -> EvolutionReport {
    let generation = parent_config.generation + 1;

    // Aggregate stats
    let total_runs = runs.len();
    let total_frames: u64 = runs.iter().map(|r| r.frame_count as u64).sum();
    let total_score: u64 = runs.iter().map(|r| r.final_score as u64).sum();
    let max_score = runs.iter().map(|r| r.final_score).max().unwrap_or(0);
    let avg_score = if total_runs == 0 {
        0.0
    } else {
        total_score as f64 / total_runs as f64
    };
    let avg_frames = if total_runs == 0 {
        0.0
    } else {
        total_frames as f64 / total_runs as f64
    };

    // Death analysis
    let mut total_deaths = 0u32;
    let mut cause_map = BTreeMap::new();
    let mut sum_nearest_threat = 0.0;
    let mut sum_min_edge = 0.0;
    let mut edge_deaths = 0u32;
    let mut bullet_deaths = 0u32;
    let mut saucer_deaths = 0u32;
    let mut asteroid_deaths = 0u32;
    let mut sum_wave_at_death = 0.0;
    let mut death_count_for_avg = 0u32;

    for run in runs {
        for death in &run.deaths {
            total_deaths += 1;
            death_count_for_avg += 1;
            sum_nearest_threat += death.nearest_threat_px;
            sum_min_edge += death.min_edge_distance_px;
            sum_wave_at_death += death.wave as f64;

            if death.min_edge_distance_px < 110.0 {
                edge_deaths += 1;
            }

            let cause_key = match death.cause {
                DeathCause::Asteroid => {
                    asteroid_deaths += 1;
                    "asteroid"
                }
                DeathCause::Saucer => {
                    saucer_deaths += 1;
                    "saucer"
                }
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
    let avg_nearest_threat = if death_count_for_avg == 0 {
        0.0
    } else {
        sum_nearest_threat / death_count_for_avg as f64
    };
    let avg_min_edge = if death_count_for_avg == 0 {
        0.0
    } else {
        sum_min_edge / death_count_for_avg as f64
    };
    let avg_wave_at_death = if death_count_for_avg == 0 {
        0.0
    } else {
        sum_wave_at_death / death_count_for_avg as f64
    };
    let edge_death_ratio = if total_deaths == 0 {
        0.0
    } else {
        edge_deaths as f64 / total_deaths as f64
    };
    let bullet_death_ratio = if total_deaths == 0 {
        0.0
    } else {
        bullet_deaths as f64 / total_deaths as f64
    };
    let saucer_death_ratio = if total_deaths == 0 {
        0.0
    } else {
        saucer_deaths as f64 / total_deaths as f64
    };
    let asteroid_death_ratio = if total_deaths == 0 {
        0.0
    } else {
        asteroid_deaths as f64 / total_deaths as f64
    };

    let mut death_insights = Vec::new();
    if bullet_death_ratio > 0.35 {
        death_insights.push(format!("High saucer bullet death rate ({:.0}%): increase bullet risk weight and dodge priority.", bullet_death_ratio * 100.0));
    }
    if edge_death_ratio > 0.25 {
        death_insights.push(format!(
            "Edge deaths ({:.0}%): increase edge penalty and center weight.",
            edge_death_ratio * 100.0
        ));
    }
    if avg_nearest_threat < 50.0 {
        death_insights.push(format!(
            "Deaths at very close range ({:.0}px avg): need faster evasion response.",
            avg_nearest_threat
        ));
    }
    if avg_wave_at_death > 6.0 {
        death_insights.push(format!(
            "Deaths concentrated in late waves ({:.1} avg): increase late-game survival weight.",
            avg_wave_at_death
        ));
    }
    if deaths_per_10k > 5.0 {
        death_insights.push(format!(
            "High death rate ({:.1}/10k frames): global risk sensitivity increase needed.",
            deaths_per_10k
        ));
    }

    // Shot analysis
    let mut total_fired = 0u32;
    let mut total_hits = 0u32;
    let mut total_misses = 0u32;

    for run in runs {
        total_fired += run.shot_summary.total_fired;
        total_hits += run.shot_summary.total_hit;
        total_misses += run.shot_summary.total_miss;
    }

    let hit_rate = if total_fired == 0 {
        0.0
    } else {
        total_hits as f64 / total_fired as f64
    };
    let miss_rate = if total_fired == 0 {
        0.0
    } else {
        total_misses as f64 / total_fired as f64
    };
    let avg_frames_between_shots = if total_fired == 0 {
        0.0
    } else {
        total_frames as f64 / total_fired as f64
    };

    let mut shot_insights = Vec::new();
    if miss_rate > 0.55 {
        shot_insights.push(format!(
            "Very high miss rate ({:.0}%): increase min fire quality and shot penalty.",
            miss_rate * 100.0
        ));
    }
    if miss_rate > 0.45 {
        shot_insights.push(format!(
            "High miss rate ({:.0}%): increase fire quality floor.",
            miss_rate * 100.0
        ));
    }
    if avg_frames_between_shots < 15.0 {
        shot_insights.push(format!(
            "Shooting too frequently ({:.1} frames between shots): increase shot discipline.",
            avg_frames_between_shots
        ));
    }
    if hit_rate > 0.55 {
        shot_insights.push(format!(
            "Good hit rate ({:.0}%): can afford to be slightly more aggressive.",
            hit_rate * 100.0
        ));
    }

    // Evolve parameters
    let mut cfg = parent_config.clone();
    cfg.generation = generation;
    cfg.parent_id = parent_config.id.clone();
    cfg.id = format!("claude-evolved-gen{generation}");
    cfg.description = format!(
        "Generation {generation} evolved from {} (avg_score={:.0}, deaths/10k={:.1}, hit_rate={:.0}%)",
        parent_config.id, avg_score, deaths_per_10k, hit_rate * 100.0
    );

    let mut adjustments = Vec::new();

    // Death-driven adjustments
    if deaths_per_10k > 4.0 {
        let scale = (deaths_per_10k / 4.0).min(1.5);
        cfg.risk_weight_asteroid *= 1.0 + 0.08 * scale;
        cfg.risk_weight_saucer *= 1.0 + 0.1 * scale;
        cfg.risk_weight_bullet *= 1.0 + 0.12 * scale;
        cfg.survival_weight *= 1.0 + 0.1 * scale;
        adjustments.push(format!(
            "Increased risk/survival weights by {:.0}% (high death rate)",
            scale * 10.0
        ));
    } else if deaths_per_10k < 2.0 && avg_score < 30000.0 {
        cfg.aggression *= 1.08;
        cfg.fire_reward *= 1.06;
        cfg.survival_weight *= 0.96;
        adjustments
            .push("Slightly increased aggression (low death rate, room for scoring)".to_string());
    }

    if bullet_death_ratio > 0.35 {
        cfg.risk_weight_bullet *= 1.15;
        adjustments.push(format!(
            "Increased bullet risk weight (bullet deaths {:.0}%)",
            bullet_death_ratio * 100.0
        ));
    }

    if edge_death_ratio > 0.25 {
        cfg.center_weight *= 1.12;
        cfg.edge_penalty *= 1.15;
        adjustments.push(format!(
            "Increased center/edge weights (edge deaths {:.0}%)",
            edge_death_ratio * 100.0
        ));
    }

    // Shot quality adjustments
    if miss_rate > 0.5 {
        cfg.min_fire_quality += 0.03;
        cfg.shot_penalty *= 1.1;
        cfg.miss_fire_penalty *= 1.1;
        adjustments.push(format!(
            "Raised fire quality floor and penalties (miss rate {:.0}%)",
            miss_rate * 100.0
        ));
    } else if hit_rate > 0.55 {
        cfg.min_fire_quality -= 0.02;
        cfg.fire_reward *= 1.05;
        adjustments.push(format!(
            "Lowered fire quality floor (good hit rate {:.0}%)",
            hit_rate * 100.0
        ));
    }

    if avg_frames_between_shots < 12.0 {
        cfg.shot_penalty *= 1.15;
        adjustments.push("Increased shot penalty (over-firing)".to_string());
    }

    // Speed management
    if avg_wave_at_death > 5.0 {
        cfg.speed_soft_cap *= 0.95;
        cfg.lookahead += 1.0;
        adjustments.push(format!(
            "Reduced speed cap and increased lookahead (late-wave deaths, avg wave {:.1})",
            avg_wave_at_death
        ));
    }

    // Clamp all values to reasonable ranges
    cfg.risk_weight_asteroid = cfg.risk_weight_asteroid.clamp(0.8, 2.5);
    cfg.risk_weight_saucer = cfg.risk_weight_saucer.clamp(1.0, 3.5);
    cfg.risk_weight_bullet = cfg.risk_weight_bullet.clamp(1.5, 4.5);
    cfg.survival_weight = cfg.survival_weight.clamp(1.0, 3.5);
    cfg.aggression = cfg.aggression.clamp(0.3, 1.5);
    cfg.fire_reward = cfg.fire_reward.clamp(0.5, 2.0);
    cfg.shot_penalty = cfg.shot_penalty.clamp(0.3, 2.0);
    cfg.miss_fire_penalty = cfg.miss_fire_penalty.clamp(0.4, 2.5);
    cfg.min_fire_quality = cfg.min_fire_quality.clamp(0.08, 0.4);
    cfg.speed_soft_cap = cfg.speed_soft_cap.clamp(3.0, 5.5);
    cfg.center_weight = cfg.center_weight.clamp(0.2, 0.8);
    cfg.edge_penalty = cfg.edge_penalty.clamp(0.15, 0.65);
    cfg.lookahead = cfg.lookahead.clamp(14.0, 28.0);
    cfg.lurk_trigger = cfg.lurk_trigger.clamp(200, 350);
    cfg.lurk_boost = cfg.lurk_boost.clamp(1.0, 2.5);
    cfg.action_penalty = cfg.action_penalty.clamp(0.005, 0.04);
    cfg.turn_penalty = cfg.turn_penalty.clamp(0.005, 0.05);
    cfg.thrust_penalty = cfg.thrust_penalty.clamp(0.005, 0.045);

    EvolutionReport {
        generated_unix_s: now_unix_s(),
        parent_id: parent_config.id.clone(),
        generation,
        runs_analyzed: total_runs,
        avg_score,
        avg_frames,
        max_score,
        death_analysis: DeathAnalysis {
            total_deaths,
            deaths_per_10k_frames: deaths_per_10k,
            cause_breakdown: cause_map,
            avg_nearest_threat_at_death: avg_nearest_threat,
            avg_min_edge_at_death: avg_min_edge,
            edge_death_ratio,
            bullet_death_ratio,
            saucer_death_ratio,
            asteroid_death_ratio,
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

// ── Evolved bot implementation ──────────────────────────────────────

pub struct EvolvedBot {
    cfg: EvolvedConfig,
}

impl EvolvedBot {
    pub fn new(cfg: EvolvedConfig) -> Self {
        Self { cfg }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let data = fs::read(path).with_context(|| format!("failed reading {}", path.display()))?;
        let cfg: EvolvedConfig = serde_json::from_slice(&data)
            .with_context(|| format!("invalid evolved config {}", path.display()))?;
        Ok(Self { cfg })
    }

    fn entity_risk(
        &self,
        pred: PredictedShip,
        ex: i32,
        ey: i32,
        evx: i32,
        evy: i32,
        radius: i32,
        weight: f64,
    ) -> f64 {
        let approach = torus_relative_approach(
            pred.x,
            pred.y,
            pred.vx,
            pred.vy,
            ex,
            ey,
            evx,
            evy,
            self.cfg.lookahead,
        );
        let safe = (pred.radius + radius + 8) as f64;
        let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
        let immediate = (safe / (approach.immediate_px + 1.0)).powf(1.35);
        let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
        let time_boost =
            1.0 + ((self.cfg.lookahead - approach.t_closest) / self.cfg.lookahead) * 0.45;
        weight * (0.78 * closeness + 0.22 * immediate) * closing * time_boost
    }

    fn action_utility(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        // Risk from entities
        let mut risk = 0.0;
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            risk += self.entity_risk(
                pred,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                self.cfg.risk_weight_asteroid,
            );
        }
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let w = if saucer.small {
                self.cfg.risk_weight_saucer * 1.28
            } else {
                self.cfg.risk_weight_saucer
            };
            risk += self.entity_risk(
                pred,
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                w,
            );
        }
        for bullet in &world.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            risk += self.entity_risk(
                pred,
                bullet.x,
                bullet.y,
                bullet.vx,
                bullet.vy,
                bullet.radius,
                self.cfg.risk_weight_bullet,
            );
        }

        // Aggression
        let mut aggression = self.cfg.aggression;
        if world.time_since_last_kill >= self.cfg.lurk_trigger {
            aggression *= self.cfg.lurk_boost;
        }
        if world.next_extra_life_score > world.score {
            let to_next = world.next_extra_life_score - world.score;
            if to_next <= 1_500 {
                aggression *= 1.12;
            }
            if to_next <= 500 {
                aggression *= 1.2;
            }
        }

        // Targeting
        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        let target_plan = best_target(world, pred);
        if let Some(plan) = target_plan {
            attack += plan.value;
            let angle_error = signed_angle_delta(pred.angle, plan.aim_angle).abs() as f64;
            fire_alignment = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
            if plan.distance_px < 280.0 {
                attack += 0.16;
            }
            if plan.intercept_frames <= 14.0 {
                attack += 0.05;
            }
        }

        // Position
        use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * self.cfg.center_weight;

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * self.cfg.edge_penalty;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > self.cfg.speed_soft_cap {
            -((speed_px - self.cfg.speed_soft_cap) / self.cfg.speed_soft_cap.max(0.1)) * 0.35
        } else {
            0.0
        };

        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);

        // Fire evaluation
        let mut fire_term = 0.0;
        if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
            let fire_quality = estimate_fire_quality(pred, world);
            let (active, shortest) = own_bullet_stats(&world.bullets);
            let nearest_saucer = nearest_saucer_distance(world, pred);
            let is_duplicate = target_plan
                .map(|p| {
                    target_already_covered(
                        &world.bullets,
                        p.target_x,
                        p.target_y,
                        p.target_vx,
                        p.target_vy,
                        p.target_radius,
                    )
                })
                .unwrap_or(false);
            let discipline_ok = disciplined_fire_ok(
                active,
                shortest,
                fire_quality,
                self.cfg.min_fire_quality,
                nearest_saucer,
                nearest_threat,
                is_duplicate,
            );
            let emergency =
                nearest_saucer < 95.0 && fire_quality + 0.08 >= self.cfg.min_fire_quality;

            if !is_duplicate
                && discipline_ok
                && (fire_quality >= self.cfg.min_fire_quality || emergency)
            {
                fire_term += self.cfg.fire_reward * fire_alignment * (0.35 + 0.65 * fire_quality);
                fire_term -= self.cfg.shot_penalty * 0.72;
            } else if is_duplicate {
                fire_term -= self.cfg.shot_penalty * 0.68;
            } else if !discipline_ok {
                fire_term -= self.cfg.shot_penalty * 0.45;
            } else {
                fire_term -= self.cfg.miss_fire_penalty
                    * (self.cfg.min_fire_quality - fire_quality).max(0.0)
                    * 0.45;
                fire_term -= self.cfg.shot_penalty * 0.2;
            }
            if world.time_since_last_kill >= self.cfg.lurk_trigger {
                fire_term += self.cfg.fire_reward * 0.25;
            }
        }

        // Control penalties
        let control_scale = if risk > 3.0 {
            0.18
        } else if risk > 1.8 {
            0.38
        } else {
            1.0
        };
        let mut control_term = 0.0;
        if action != 0 {
            control_term -= self.cfg.action_penalty * control_scale;
        }
        if input.left || input.right {
            control_term -= self.cfg.turn_penalty * control_scale;
            if nearest_threat > 180.0 {
                control_term -= self.cfg.turn_penalty * 0.45;
            }
        }
        if input.thrust {
            control_term -= self.cfg.thrust_penalty * control_scale;
            if nearest_threat > 190.0 && speed_px > self.cfg.speed_soft_cap * 0.72 {
                control_term -= self.cfg.thrust_penalty * 0.65;
            }
            if speed_px > self.cfg.speed_soft_cap * 1.05 {
                control_term -= self.cfg.thrust_penalty * 1.1;
            }
        } else if action == 0x00 && nearest_threat > 165.0 {
            control_term += self.cfg.action_penalty * 0.18;
        }

        -risk * self.cfg.survival_weight
            + attack * aggression
            + fire_term
            + control_term
            + center_term
            + edge_term
            + speed_term
    }
}

impl AutopilotBot for EvolvedBot {
    fn id(&self) -> &'static str {
        // Leak the string to get 'static (safe because configs persist for program lifetime)
        Box::leak(self.cfg.id.clone().into_boxed_str())
    }
    fn description(&self) -> &'static str {
        Box::leak(self.cfg.description.clone().into_boxed_str())
    }
    fn reset(&mut self, _seed: u32) {}
    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: false,
            };
        }
        let mut best_action = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        for action in 0x00u8..=0x0F {
            let utility = self.action_utility(world, action);
            if utility > best_value {
                best_value = utility;
                best_action = action;
            }
        }
        decode_input_byte(best_action)
    }
}

// ── Evolution cycle runner ──────────────────────────────────────────

/// Run a full evolution cycle:
/// 1. Benchmark parent bot on given seeds
/// 2. Collect telemetry (deaths, shots)
/// 3. Analyze and evolve new config
/// 4. Benchmark new config
/// 5. Save report and config
pub fn run_evolution_cycle(
    parent_config: &EvolvedConfig,
    seeds: &[u32],
    max_frames: u32,
    output_dir: &Path,
) -> Result<EvolutionReport> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed creating {}", output_dir.display()))?;

    // 1. Run parent and collect intel (directly instantiate EvolvedBot, bypassing roster)
    let mut runs = Vec::new();
    for &seed in seeds {
        let mut bot = EvolvedBot::new(parent_config.clone());
        let artifact = run_bot_instance(&mut bot, seed, max_frames)
            .with_context(|| format!("failed running {} seed={:#010x}", parent_config.id, seed))?;
        let intel = analyze_inputs(&parent_config.id, seed, max_frames, &artifact.inputs)
            .with_context(|| {
                format!(
                    "failed analyzing intel for {} seed={:#010x}",
                    parent_config.id, seed
                )
            })?;
        runs.push(intel);
    }

    // 2. Evolve
    let report = evolve_from_intel(parent_config, &runs);

    // 3. Save
    let config_path = output_dir.join(format!("{}.json", report.evolved_config.id));
    let report_path = output_dir.join(format!("evolution-report-gen{}.json", report.generation));

    let config_json = serde_json::to_vec_pretty(&report.evolved_config)?;
    fs::write(&config_path, config_json)
        .with_context(|| format!("failed writing {}", config_path.display()))?;

    let report_json = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed writing {}", report_path.display()))?;

    Ok(report)
}

/// Run multiple evolution generations iteratively.
pub fn run_multi_generation(
    initial_config: &EvolvedConfig,
    seeds: &[u32],
    max_frames: u32,
    generations: u32,
    output_dir: &Path,
) -> Result<Vec<EvolutionReport>> {
    let mut reports = Vec::new();
    let mut current_config = initial_config.clone();

    for gen in 0..generations {
        let gen_dir = output_dir.join(format!("gen-{gen}"));
        eprintln!(
            "=== Evolution generation {gen}/{generations}: {} ===",
            current_config.id
        );

        let report = run_evolution_cycle(&current_config, seeds, max_frames, &gen_dir)?;

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
