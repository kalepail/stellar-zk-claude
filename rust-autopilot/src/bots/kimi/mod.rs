//! Kimi's Autopilot Bot Collection
//!
//! Multiple autopilot strategies with different approaches:
//! - Hunter: Aggressive score-focused
//! - Survivor: Defensive survival-focused  
//! - Sniper: Precision shot-focused
//! - WrapMaster: Wrap-aware movement specialist
//! - SaucerKiller: Saucer prioritization specialist
//! - SuperShip: Combined best features
//!
//! All bots include learning framework that tracks:
//! - Death patterns and causes
//! - Missed shot analysis
//! - Strategy adjustments based on failures

pub mod configs;
pub mod learning;

use crate::bots::{
    AsteroidSizeSnapshot, AsteroidSnapshot, AutopilotBot, BotManifestEntry, BulletSnapshot,
    MovingTarget, SaucerSnapshot, TargetingPlan,
};
use asteroids_verifier_core::constants::{
    SCORE_LARGE_ASTEROID, SCORE_LARGE_SAUCER, SCORE_MEDIUM_ASTEROID, SCORE_SMALL_ASTEROID,
    SCORE_SMALL_SAUCER, SHIP_BULLET_COOLDOWN_FRAMES, SHIP_BULLET_LIFETIME_FRAMES,
    SHIP_BULLET_LIMIT, SHIP_BULLET_SPEED_Q8_8, SHIP_MAX_SPEED_SQ_Q16_16, SHIP_THRUST_Q8_8,
    SHIP_TURN_SPEED_BAM, WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::{
    apply_drag, atan2_bam, clamp_speed_q8_8, cos_bam, displace_q12_4, shortest_delta_q12_4,
    sin_bam, velocity_q8_8, wrap_x_q12_4, wrap_y_q12_4,
};
use asteroids_verifier_core::rng::SeededRng;
use asteroids_verifier_core::sim::{BulletSnapshot as CoreBulletSnapshot, WorldSnapshot};
use asteroids_verifier_core::tape::{decode_input_byte, encode_input_byte, FrameInput};
use learning::{
    DeathCause, DeathRecord, EntitySnapshot, FrameAction, LearningBot, LearningDatabase,
    MissedShotRecord, StrategyAdjustments, TargetType,
};
use std::collections::VecDeque;

// ============================================================================
// PREDICTED SHIP
// ============================================================================

#[derive(Clone, Copy, Debug)]
struct PredictedShip {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    angle: i32,
    radius: i32,
    fire_cooldown: i32,
}

// ============================================================================
// UTILITY FUNCTIONS (same as main bots)
// ============================================================================

const TORUS_SHIFTS_X_Q12_4: [i32; 3] = [-WORLD_WIDTH_Q12_4, 0, WORLD_WIDTH_Q12_4];
const TORUS_SHIFTS_Y_Q12_4: [i32; 3] = [-WORLD_HEIGHT_Q12_4, 0, WORLD_HEIGHT_Q12_4];

#[derive(Clone, Copy)]
struct TorusApproach {
    immediate_px: f64,
    closest_px: f64,
    t_closest: f64,
    dot: f64,
}

#[derive(Clone, Copy)]
struct WrapAimSolution {
    distance_px: f64,
    aim_angle: i32,
    intercept_frames: f64,
}

fn torus_relative_approach(
    ref_x: i32,
    ref_y: i32,
    ref_vx: i32,
    ref_vy: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    horizon_frames: f64,
) -> TorusApproach {
    let horizon = horizon_frames.max(0.0);
    let rvx = (target_vx - ref_vx) as f64 / 256.0;
    let rvy = (target_vy - ref_vy) as f64 / 256.0;
    let rv_sq = rvx * rvx + rvy * rvy;

    let mut best = TorusApproach {
        immediate_px: f64::MAX,
        closest_px: f64::MAX,
        t_closest: 0.0,
        dot: 0.0,
    };

    for sx in TORUS_SHIFTS_X_Q12_4 {
        for sy in TORUS_SHIFTS_Y_Q12_4 {
            let dx = (target_x + sx - ref_x) as f64 / 16.0;
            let dy = (target_y + sy - ref_y) as f64 / 16.0;
            let immediate = (dx * dx + dy * dy).sqrt();
            let dot = dx * rvx + dy * rvy;

            let mut t = if rv_sq > 1e-6 { -dot / rv_sq } else { 0.0 };
            t = t.clamp(0.0, horizon);
            let cdx = dx + rvx * t;
            let cdy = dy + rvy * t;
            let closest = (cdx * cdx + cdy * cdy).sqrt();

            if closest < best.closest_px - 1e-6
                || ((closest - best.closest_px).abs() <= 1e-6 && immediate < best.immediate_px)
            {
                best = TorusApproach {
                    immediate_px: immediate,
                    closest_px: closest,
                    t_closest: t,
                    dot,
                };
            }
        }
    }

    best
}

fn best_wrapped_aim(
    shooter_x: i32,
    shooter_y: i32,
    shooter_vx: i32,
    shooter_vy: i32,
    shooter_angle: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    bullet_speed_px: f64,
    max_lead_frames: f64,
) -> Option<WrapAimSolution> {
    if bullet_speed_px <= 0.05 {
        return None;
    }

    let rvx = (target_vx - shooter_vx) as f64 / 256.0;
    let rvy = (target_vy - shooter_vy) as f64 / 256.0;
    let lead_cap = max_lead_frames.max(0.0);
    let mut best: Option<(WrapAimSolution, f64)> = None;

    for sx in TORUS_SHIFTS_X_Q12_4 {
        for sy in TORUS_SHIFTS_Y_Q12_4 {
            let base_dx = (target_x + sx - shooter_x) as f64 / 16.0;
            let base_dy = (target_y + sy - shooter_y) as f64 / 16.0;
            let base_dist = (base_dx * base_dx + base_dy * base_dy).sqrt();
            if base_dist < 0.1 {
                continue;
            }

            let a = rvx * rvx + rvy * rvy - bullet_speed_px * bullet_speed_px;
            let b = 2.0 * (base_dx * rvx + base_dy * rvy);
            let c = base_dx * base_dx + base_dy * base_dy;
            let mut candidates = [f64::NAN; 2];
            let mut candidate_count = 0usize;

            if a.abs() <= 1e-8 {
                if b.abs() > 1e-8 {
                    candidates[candidate_count] = -c / b;
                    candidate_count += 1;
                }
            } else {
                let disc = b * b - 4.0 * a * c;
                if disc >= 0.0 {
                    let sqrt_disc = disc.sqrt();
                    candidates[candidate_count] = (-b - sqrt_disc) / (2.0 * a);
                    candidate_count += 1;
                    candidates[candidate_count] = (-b + sqrt_disc) / (2.0 * a);
                    candidate_count += 1;
                }
            }

            let mut best_t: Option<f64> = None;
            for t in candidates.iter().copied().take(candidate_count) {
                if !t.is_finite() {
                    continue;
                }
                if t < 0.0 || t > lead_cap {
                    continue;
                }
                match best_t {
                    None => best_t = Some(t),
                    Some(existing) if t < existing => best_t = Some(t),
                    _ => {}
                }
            }

            let Some(t) = best_t else {
                continue;
            };

            let pdx = base_dx + rvx * t;
            let pdy = base_dy + rvy * t;
            let distance_px = (pdx * pdx + pdy * pdy).sqrt();
            if distance_px < 0.1 {
                continue;
            }

            let aim_angle = atan2_bam((pdy * 16.0) as i32, (pdx * 16.0) as i32);
            let angle_error = signed_angle_delta(shooter_angle, aim_angle).abs() as f64;
            let ranking = t + angle_error * 0.015;
            let candidate = WrapAimSolution {
                distance_px,
                aim_angle,
                intercept_frames: t,
            };

            match best {
                None => best = Some((candidate, ranking)),
                Some((_, best_ranking)) if ranking < best_ranking => {
                    best = Some((candidate, ranking))
                }
                _ => {}
            }
        }
    }

    best.map(|(solution, _)| solution)
}

fn projectile_wrap_closest_approach(
    start_x: i32,
    start_y: i32,
    bullet_vx: i32,
    bullet_vy: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    max_frames: f64,
) -> (f64, f64) {
    let horizon = max_frames.max(0.0);
    let rvx = (target_vx - bullet_vx) as f64 / 256.0;
    let rvy = (target_vy - bullet_vy) as f64 / 256.0;
    let rv_sq = rvx * rvx + rvy * rvy;

    let mut best_closest = f64::MAX;
    let mut best_t = 0.0;

    for sx in TORUS_SHIFTS_X_Q12_4 {
        for sy in TORUS_SHIFTS_Y_Q12_4 {
            let dx = (target_x + sx - start_x) as f64 / 16.0;
            let dy = (target_y + sy - start_y) as f64 / 16.0;
            let dot = dx * rvx + dy * rvy;
            let mut t = if rv_sq > 1e-6 { -dot / rv_sq } else { 0.0 };
            t = t.clamp(0.0, horizon);

            let cdx = dx + rvx * t;
            let cdy = dy + rvy * t;
            let closest = (cdx * cdx + cdy * cdy).sqrt();
            if closest < best_closest {
                best_closest = closest;
                best_t = t;
            }
        }
    }

    (best_closest, best_t)
}

fn own_bullet_in_flight_stats(bullets: &[CoreBulletSnapshot]) -> (usize, i32) {
    let mut active = 0usize;
    let mut shortest_life = i32::MAX;
    for bullet in bullets {
        if !bullet.alive {
            continue;
        }
        active += 1;
        shortest_life = shortest_life.min(bullet.life);
    }
    if shortest_life == i32::MAX {
        shortest_life = 0;
    }
    (active, shortest_life)
}

fn bullet_confidently_tracks_target(bullet: &CoreBulletSnapshot, target: MovingTarget) -> bool {
    if !bullet.alive || bullet.life <= 0 {
        return false;
    }
    let horizon = (bullet.life as f64).min(32.0).max(1.0);
    let (closest, t) = projectile_wrap_closest_approach(
        bullet.x, bullet.y, bullet.vx, bullet.vy, target.x, target.y, target.vx, target.vy, horizon,
    );
    let hit_radius = (bullet.radius + target.radius) as f64;
    closest <= hit_radius * 1.02 && t <= horizon * 0.9
}

fn target_already_covered_by_ship_bullets(
    target: MovingTarget,
    bullets: &[CoreBulletSnapshot],
) -> bool {
    bullets
        .iter()
        .any(|bullet| bullet_confidently_tracks_target(bullet, target))
}

fn disciplined_fire_gate(
    active_bullets: usize,
    shortest_life: i32,
    fire_quality: f64,
    min_fire_quality: f64,
    nearest_saucer_px: f64,
    nearest_threat_px: f64,
    duplicate_target_shot: bool,
) -> bool {
    let strict_quality = (min_fire_quality + 0.1).clamp(0.18, 0.9);
    if active_bullets == 0 {
        return fire_quality >= strict_quality;
    }

    if !duplicate_target_shot {
        let rapid_switch_window = nearest_threat_px < 118.0 || nearest_saucer_px < 136.0;
        let switch_quality = (strict_quality + 0.2).clamp(0.24, 0.95);
        if rapid_switch_window && fire_quality >= switch_quality {
            return true;
        }
    }

    let emergency = nearest_threat_px < 78.0 || nearest_saucer_px < 88.0;
    let life_gate = if emergency { 3 } else { 2 };
    if shortest_life > life_gate {
        return false;
    }

    let stacked_quality = (strict_quality + if emergency { 0.08 } else { 0.18 }).clamp(0.24, 0.94);
    fire_quality >= stacked_quality
}

#[inline]
fn no_input() -> FrameInput {
    FrameInput {
        left: false,
        right: false,
        thrust: false,
        fire: false,
    }
}

#[inline]
fn signed_angle_delta(current: i32, target: i32) -> i32 {
    let mut delta = (target - current) & 0xff;
    if delta > 127 {
        delta -= 256;
    }
    delta
}

// ============================================================================
// KIMI LEARNING BOT - Base implementation with learning framework
// ============================================================================

pub struct KimiLearningBot {
    cfg: configs::KimiSearchConfig,
    rng: SeededRng,
    orbit_sign: i32,
    frame_count: u32,
    last_score: u32,
    current_aggression: f64,
    action_history: VecDeque<FrameAction>,
    learning_db: LearningDatabase,
    last_fire_estimate: Option<f64>,
    last_target: Option<TargetType>,
}

impl KimiLearningBot {
    pub fn new(cfg: configs::KimiSearchConfig) -> Self {
        let db = if cfg.learning_enabled {
            LearningDatabase::load_or_create(cfg.learning_db_path)
        } else {
            LearningDatabase::new()
        };

        Self {
            cfg,
            rng: SeededRng::new(0x5E4C_A100),
            orbit_sign: 1,
            frame_count: 0,
            last_score: 0,
            current_aggression: cfg.aggression_weight,
            action_history: VecDeque::with_capacity(60),
            learning_db: db,
            last_fire_estimate: None,
            last_target: None,
        }
    }

    fn predict_ship(&self, world: &WorldSnapshot, input_byte: u8) -> PredictedShip {
        let ship = &world.ship;
        let input = decode_input_byte(input_byte);

        let mut angle = ship.angle;
        if input.left {
            angle = (angle - SHIP_TURN_SPEED_BAM) & 0xff;
        }
        if input.right {
            angle = (angle + SHIP_TURN_SPEED_BAM) & 0xff;
        }

        let mut vx = ship.vx;
        let mut vy = ship.vy;

        if input.thrust {
            let accel_vx = (cos_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
            let accel_vy = (sin_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
            vx += accel_vx;
            vy += accel_vy;
        }

        vx = apply_drag(vx);
        vy = apply_drag(vy);
        (vx, vy) = clamp_speed_q8_8(vx, vy, SHIP_MAX_SPEED_SQ_Q16_16);

        let x = wrap_x_q12_4(ship.x + (vx >> 4));
        let y = wrap_y_q12_4(ship.y + (vy >> 4));
        let fire_cooldown = if ship.fire_cooldown > 0 {
            ship.fire_cooldown - 1
        } else {
            ship.fire_cooldown
        };

        PredictedShip {
            x,
            y,
            vx,
            vy,
            angle,
            radius: ship.radius,
            fire_cooldown,
        }
    }

    fn update_aggression(&mut self, world: &WorldSnapshot) {
        self.frame_count += 1;
        let score_delta = world.score.saturating_sub(self.last_score);
        self.last_score = world.score;

        if score_delta > 0 {
            self.current_aggression =
                (self.current_aggression * 1.02).min(self.cfg.aggression_weight * 1.5);
        } else if world.time_since_last_kill > self.cfg.lurk_trigger_frames {
            self.current_aggression =
                (self.current_aggression * 0.98).max(self.cfg.aggression_weight * 0.7);
        }

        if world.next_extra_life_score > world.score {
            let to_next = world.next_extra_life_score - world.score;
            if to_next <= 1000 {
                self.current_aggression *= 1.15;
            }
        }

        // Apply learned adjustments
        self.current_aggression *= self.learning_db.strategy_adjustments.base_aggression;
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
            self.cfg.lookahead_frames,
        );
        let closest = approach.closest_px;
        let immediate = approach.immediate_px;
        let dot = approach.dot;
        let t = approach.t_closest;
        let safe = (pred.radius + radius + 8) as f64;

        let closeness = (safe / (closest + 1.0)).powf(2.0);
        let immediate_boost = (safe / (immediate + 1.0)).powf(1.35);
        let closing_boost = if dot < 0.0 { 1.25 } else { 0.92 };
        let time_boost = 1.0 + ((self.cfg.lookahead_frames - t) / self.cfg.lookahead_frames) * 0.45;

        weight * (0.78 * closeness + 0.22 * immediate_boost) * closing_boost * time_boost
    }

    fn target_score(&self, world: &WorldSnapshot, pred: PredictedShip) -> Option<TargetingPlan> {
        let speed_px = ((pred.vx as f64 / 256.0).powi(2) + (pred.vy as f64 / 256.0).powi(2)).sqrt();
        let bullet_speed = 8.6 + speed_px * 0.33;

        let mut best: Option<TargetingPlan> = None;

        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            let w = match asteroid.size {
                AsteroidSizeSnapshot::Large => 0.96,
                AsteroidSizeSnapshot::Medium => 1.22,
                AsteroidSizeSnapshot::Small => 1.44,
            };

            if let Some(intercept) = best_wrapped_aim(
                pred.x,
                pred.y,
                pred.vx,
                pred.vy,
                pred.angle,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                bullet_speed,
                64.0,
            ) {
                let angle_error = signed_angle_delta(pred.angle, intercept.aim_angle).abs() as f64;
                let mut value = w / (intercept.distance_px + 16.0);
                value += (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
                value *= 1.0 + (1.0 - (intercept.intercept_frames / 64.0).clamp(0.0, 1.0)) * 0.1;
                value *= 1.0 + (200.0 - intercept.distance_px.min(200.0)) / 400.0;

                let candidate = TargetingPlan {
                    distance_px: intercept.distance_px,
                    aim_angle: intercept.aim_angle,
                    value,
                    track: MovingTarget {
                        x: asteroid.x,
                        y: asteroid.y,
                        vx: asteroid.vx,
                        vy: asteroid.vy,
                        radius: asteroid.radius,
                    },
                    intercept_frames: intercept.intercept_frames,
                };

                match best {
                    None => best = Some(candidate),
                    Some(existing) if candidate.value > existing.value => best = Some(candidate),
                    _ => {}
                }
            }
        }

        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let mut w = if saucer.small { 3.2 } else { 2.1 };
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 22.0,
            );
            if approach.closest_px < 200.0 {
                let urgency = ((200.0 - approach.closest_px) / 200.0).clamp(0.0, 1.0);
                w *= 1.0 + urgency * 0.85;
            }

            if let Some(intercept) = best_wrapped_aim(
                pred.x,
                pred.y,
                pred.vx,
                pred.vy,
                pred.angle,
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                bullet_speed,
                64.0,
            ) {
                let angle_error = signed_angle_delta(pred.angle, intercept.aim_angle).abs() as f64;
                let mut value = w / (intercept.distance_px + 16.0);
                value += (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
                value *= 1.0 + (1.0 - (intercept.intercept_frames / 64.0).clamp(0.0, 1.0)) * 0.1;

                let candidate = TargetingPlan {
                    distance_px: intercept.distance_px,
                    aim_angle: intercept.aim_angle,
                    value,
                    track: MovingTarget {
                        x: saucer.x,
                        y: saucer.y,
                        vx: saucer.vx,
                        vy: saucer.vy,
                        radius: saucer.radius,
                    },
                    intercept_frames: intercept.intercept_frames,
                };

                match best {
                    None => best = Some(candidate),
                    Some(existing) if candidate.value > existing.value => best = Some(candidate),
                    _ => {}
                }
            }
        }

        best
    }

    fn estimate_fire_quality(&self, world: &WorldSnapshot, pred: PredictedShip) -> f64 {
        let (dx, dy) = displace_q12_4(pred.angle, pred.radius + 6);
        let start_x = wrap_x_q12_4(pred.x + dx);
        let start_y = wrap_y_q12_4(pred.y + dy);
        let ship_speed_approx = ((pred.vx.abs() + pred.vy.abs()) * 3) >> 2;
        let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
        let (bvx, bvy) = velocity_q8_8(pred.angle, bullet_speed_q8_8);
        let bullet_vx = pred.vx + bvx;
        let bullet_vy = pred.vy + bvy;

        let mut best = 0.0;
        let max_t = SHIP_BULLET_LIFETIME_FRAMES as f64;

        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            let w = match asteroid.size {
                AsteroidSizeSnapshot::Large => 0.96,
                AsteroidSizeSnapshot::Medium => 1.22,
                AsteroidSizeSnapshot::Small => 1.44,
            };
            let (closest, t) = projectile_wrap_closest_approach(
                start_x,
                start_y,
                bullet_vx,
                bullet_vy,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                max_t,
            );
            let safe = (asteroid.radius + 2) as f64;
            let hit_score = (safe / (closest + 1.0)).powf(1.75);
            let time_factor = 1.0 - (t / max_t) * 0.42;
            let candidate = w * hit_score * time_factor;
            if candidate > best {
                best = candidate;
            }
        }

        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let mut w = if saucer.small { 3.2 } else { 2.1 };
            let (closest, t) = projectile_wrap_closest_approach(
                start_x, start_y, bullet_vx, bullet_vy, saucer.x, saucer.y, saucer.vx, saucer.vy,
                max_t,
            );
            let safe = (saucer.radius + 2) as f64;
            let hit_score = (safe / (closest + 1.0)).powf(1.75);
            let time_factor = 1.0 - (t / max_t) * 0.42;
            let candidate = w * hit_score * time_factor;
            if candidate > best {
                best = candidate;
            }
        }

        best
    }

    fn action_utility(&self, world: &WorldSnapshot, input_byte: u8) -> f64 {
        let pred = self.predict_ship(world, input_byte);
        let input = decode_input_byte(input_byte);

        // Apply learned risk multipliers
        let asteroid_risk = self.cfg.risk_weight_asteroid
            * self
                .learning_db
                .strategy_adjustments
                .asteroid_risk_multiplier;
        let saucer_risk = self.cfg.risk_weight_saucer
            * self.learning_db.strategy_adjustments.saucer_risk_multiplier;
        let bullet_risk = self.cfg.risk_weight_bullet
            * self.learning_db.strategy_adjustments.bullet_risk_multiplier;

        let mut risk = 0.0;
        for asteroid in &world.asteroids {
            risk += self.entity_risk(
                pred,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                asteroid_risk,
            );
        }
        for saucer in &world.saucers {
            let sr = if saucer.small {
                saucer_risk * 1.35
            } else {
                saucer_risk
            };
            risk += self.entity_risk(
                pred,
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                sr,
            );
        }
        for bullet in &world.saucer_bullets {
            risk += self.entity_risk(
                pred,
                bullet.x,
                bullet.y,
                bullet.vx,
                bullet.vy,
                bullet.radius,
                bullet_risk,
            );
        }

        let mut aggression =
            self.current_aggression * self.learning_db.strategy_adjustments.lurk_aggression;
        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            aggression *= self.cfg.lurk_aggression_boost;
        }
        if world.next_extra_life_score > world.score {
            let to_next = world.next_extra_life_score - world.score;
            if to_next <= 1500 {
                aggression *= 1.12;
            }
            if to_next <= 500 {
                aggression *= 1.2;
            }
        }

        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        let target_plan = self.target_score(world, pred);
        if let Some(plan) = target_plan {
            attack += plan.value;
            let angle_error = signed_angle_delta(pred.angle, plan.aim_angle).abs() as f64;
            fire_alignment =
                (1.0 - (angle_error / self.cfg.fire_tolerance_bam.max(1) as f64)).clamp(0.0, 1.0);
            if plan.distance_px < self.cfg.fire_distance_px {
                attack += 0.18;
            }
            if plan.intercept_frames <= 14.0 {
                attack += 0.06;
            }
        }

        // Position evaluation with learned adjustments
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
        let edge_term = -((self.learning_db.strategy_adjustments.edge_buffer_px - min_edge)
            .max(0.0)
            / self.learning_db.strategy_adjustments.edge_buffer_px)
            * self.cfg.edge_penalty;

        let speed_px = ((pred.vx as f64 / 256.0).powi(2) + (pred.vy as f64 / 256.0).powi(2)).sqrt();
        let max_safe = self
            .cfg
            .speed_soft_cap
            .min(self.learning_db.strategy_adjustments.max_safe_speed);
        let speed_term = if speed_px > max_safe {
            -((speed_px - max_safe) / max_safe.max(0.1)) * 0.35
        } else {
            0.0
        };

        let nearest_threat = self.nearest_threat_distance_px(world, pred);

        // Fire evaluation with learned thresholds
        let mut fire_term = 0.0;
        if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
            let fire_quality = self.estimate_fire_quality(world, pred);
            let min_fire_quality = self.fire_quality_floor(world, pred, aggression);
            let nearest_saucer = self.nearest_saucer_distance_px(world, pred);
            let emergency_saucer = nearest_saucer < 95.0 && fire_quality + 0.08 >= min_fire_quality;
            let duplicate_target_shot = target_plan
                .map(|plan| target_already_covered_by_ship_bullets(plan.track, &world.bullets))
                .unwrap_or(false);
            let (active_ship_bullets, shortest_ship_bullet_life) =
                own_bullet_in_flight_stats(&world.bullets);
            let discipline_ok = disciplined_fire_gate(
                active_ship_bullets,
                shortest_ship_bullet_life,
                fire_quality,
                min_fire_quality,
                nearest_saucer,
                nearest_threat,
                duplicate_target_shot,
            );

            if !duplicate_target_shot
                && discipline_ok
                && (fire_quality >= min_fire_quality || emergency_saucer)
            {
                fire_term += self.cfg.fire_reward * fire_alignment * (0.35 + 0.65 * fire_quality);
                fire_term -= self.cfg.shot_penalty * 0.72;
            } else if duplicate_target_shot {
                fire_term -= self.cfg.shot_penalty * 0.68;
            } else if !discipline_ok {
                fire_term -= self.cfg.shot_penalty * 0.45;
            } else {
                fire_term -= self.cfg.miss_fire_penalty * (min_fire_quality - fire_quality) * 0.45;
                fire_term -= self.cfg.shot_penalty * 0.2;
            }
            if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
                fire_term += self.cfg.fire_reward * 0.25;
            }
        }

        // Control penalties
        let mut control_term = 0.0;
        let control_scale = if risk > 3.0 {
            0.18
        } else if risk > 1.8 {
            0.38
        } else {
            1.0
        };
        if input_byte != 0 {
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
            if nearest_threat > 190.0 && speed_px > max_safe * 0.72 {
                control_term -= self.cfg.thrust_penalty * 0.65;
            }
            if speed_px > max_safe * 1.05 {
                control_term -= self.cfg.thrust_penalty * 1.1;
            }
        } else if input_byte == 0x00 && nearest_threat > 165.0 {
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

    fn nearest_saucer_distance_px(&self, world: &WorldSnapshot, pred: PredictedShip) -> f64 {
        let mut nearest = f64::MAX;
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 16.0,
            );
            nearest = nearest.min(approach.immediate_px);
        }
        nearest
    }

    fn nearest_threat_distance_px(&self, world: &WorldSnapshot, pred: PredictedShip) -> f64 {
        let mut nearest = f64::MAX;
        let mut consider = |x: i32, y: i32| {
            let dx = shortest_delta_q12_4(pred.x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(pred.y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            nearest = nearest.min((dx * dx + dy * dy).sqrt());
        };

        for asteroid in &world.asteroids {
            if asteroid.alive {
                consider(asteroid.x, asteroid.y);
            }
        }
        for saucer in &world.saucers {
            if saucer.alive {
                consider(saucer.x, saucer.y);
            }
        }
        for bullet in &world.saucer_bullets {
            if bullet.alive {
                consider(bullet.x, bullet.y);
            }
        }

        if nearest == f64::MAX {
            9999.0
        } else {
            nearest
        }
    }

    fn fire_quality_floor(
        &self,
        world: &WorldSnapshot,
        pred: PredictedShip,
        aggression: f64,
    ) -> f64 {
        let mut floor = 0.13 + self.cfg.shot_penalty * 0.08 + self.cfg.miss_fire_penalty * 0.06
            - aggression * 0.05;

        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            floor -= 0.02;
        }

        let nearest_saucer = self.nearest_saucer_distance_px(world, pred);
        if nearest_saucer < 260.0 {
            floor -= 0.04;
        }
        if nearest_saucer < 160.0 {
            floor -= 0.05;
        }
        if nearest_saucer < 95.0 {
            floor -= 0.08;
        }
        if world.saucers.iter().any(|s| s.small) {
            floor -= 0.03;
        }

        // Apply learned fire quality threshold
        floor = floor.max(self.learning_db.strategy_adjustments.fire_quality_threshold);

        floor.clamp(0.05, 0.38)
    }

    fn has_uncovered_target(&self, world: &WorldSnapshot) -> bool {
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            if !target_already_covered_by_ship_bullets(
                MovingTarget {
                    x: asteroid.x,
                    y: asteroid.y,
                    vx: asteroid.vx,
                    vy: asteroid.vy,
                    radius: asteroid.radius,
                },
                &world.bullets,
            ) {
                return true;
            }
        }
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            if !target_already_covered_by_ship_bullets(
                MovingTarget {
                    x: saucer.x,
                    y: saucer.y,
                    vx: saucer.vx,
                    vy: saucer.vy,
                    radius: saucer.radius,
                },
                &world.bullets,
            ) {
                return true;
            }
        }
        false
    }

    fn record_action(&mut self, world: &WorldSnapshot, action_byte: u8) {
        let pred = PredictedShip {
            x: world.ship.x,
            y: world.ship.y,
            vx: world.ship.vx,
            vy: world.ship.vy,
            angle: world.ship.angle,
            radius: world.ship.radius,
            fire_cooldown: world.ship.fire_cooldown,
        };
        let threat_dist = self.nearest_threat_distance_px(world, pred);

        let action = FrameAction {
            frame: self.frame_count,
            action_byte,
            score: world.score,
            threat_distance: threat_dist,
            target_count: world.asteroids.iter().filter(|a| a.alive).count()
                + world.saucers.iter().filter(|s| s.alive).count(),
            saucer_count: world.saucers.iter().filter(|s| s.alive).count(),
            bullet_count: world.bullets.len(),
        };

        self.action_history.push_back(action);
        if self.action_history.len() > 60 {
            self.action_history.pop_front();
        }
    }

    fn detect_death_cause(&self, world: &WorldSnapshot) -> DeathCause {
        let ship = &world.ship;
        let mut cause = DeathCause::Unknown;
        let mut min_dist = f64::MAX;

        // Check asteroid collision
        for asteroid in &world.asteroids {
            let dx = shortest_delta_q12_4(ship.x, asteroid.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(ship.y, asteroid.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let hit_dist = (ship.radius + asteroid.radius) as f64;
            if dist < hit_dist && dist < min_dist {
                min_dist = dist;
                cause = DeathCause::AsteroidCollision {
                    size: match asteroid.size {
                        AsteroidSizeSnapshot::Large => "large".to_string(),
                        AsteroidSizeSnapshot::Medium => "medium".to_string(),
                        AsteroidSizeSnapshot::Small => "small".to_string(),
                    },
                };
            }
        }

        // Check saucer collision
        for saucer in &world.saucers {
            let dx = shortest_delta_q12_4(ship.x, saucer.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(ship.y, saucer.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let hit_dist = (ship.radius + saucer.radius) as f64;
            if dist < hit_dist && dist < min_dist {
                min_dist = dist;
                cause = DeathCause::SaucerCollision {
                    small: saucer.small,
                };
            }
        }

        // Check bullet collision
        for bullet in &world.saucer_bullets {
            let dx = shortest_delta_q12_4(ship.x, bullet.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(ship.y, bullet.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let hit_dist = (ship.radius + bullet.radius) as f64;
            if dist < hit_dist && dist < min_dist {
                min_dist = dist;
                cause = DeathCause::SaucerBullet;
            }
        }

        cause
    }
}

impl AutopilotBot for KimiLearningBot {
    fn id(&self) -> &'static str {
        self.cfg.id
    }

    fn description(&self) -> &'static str {
        self.cfg.description
    }

    fn reset(&mut self, seed: u32) {
        let hash = self
            .cfg
            .id
            .bytes()
            .fold(0u32, |acc, b| acc.rotate_left(5) ^ (b as u32));
        self.rng = SeededRng::new(seed ^ hash ^ 0xBADC_0DED);
        self.orbit_sign = if self.rng.next() & 1 == 0 { 1 } else { -1 };
        self.frame_count = 0;
        self.last_score = 0;
        self.current_aggression = self.cfg.aggression_weight;
        self.action_history.clear();
        self.last_fire_estimate = None;
        self.last_target = None;

        if self.cfg.learning_enabled {
            self.learning_db.save(self.cfg.learning_db_path);
        }
        self.learning_db.start_new_game();
    }

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        // Check if ship died
        if !world.ship.alive && world.lives < 3 && self.frame_count > 0 {
            let cause = self.detect_death_cause(world);
            let pred = PredictedShip {
                x: world.ship.x,
                y: world.ship.y,
                vx: world.ship.vx,
                vy: world.ship.vy,
                angle: world.ship.angle,
                radius: world.ship.radius,
                fire_cooldown: world.ship.fire_cooldown,
            };

            // Count nearby threats
            let nearby_asteroids: Vec<_> = world
                .asteroids
                .iter()
                .filter(|a| a.alive)
                .map(|a| {
                    let dx =
                        shortest_delta_q12_4(world.ship.x, a.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
                    let dy =
                        shortest_delta_q12_4(world.ship.y, a.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
                    let dist = (dx * dx + dy * dy).sqrt();
                    EntitySnapshot {
                        x: a.x,
                        y: a.y,
                        vx: a.vx,
                        vy: a.vy,
                        distance: dist,
                        relative_velocity_toward_ship: 0.0,
                    }
                })
                .filter(|e| e.distance < 300.0)
                .collect();

            let nearby_saucers: Vec<_> = world
                .saucers
                .iter()
                .filter(|s| s.alive)
                .map(|s| {
                    let dx =
                        shortest_delta_q12_4(world.ship.x, s.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
                    let dy =
                        shortest_delta_q12_4(world.ship.y, s.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
                    let dist = (dx * dx + dy * dy).sqrt();
                    EntitySnapshot {
                        x: s.x,
                        y: s.y,
                        vx: s.vx,
                        vy: s.vy,
                        distance: dist,
                        relative_velocity_toward_ship: 0.0,
                    }
                })
                .filter(|e| e.distance < 300.0)
                .collect();

            let nearby_bullets: Vec<_> = world
                .saucer_bullets
                .iter()
                .filter(|b| b.alive)
                .map(|b| {
                    let dx =
                        shortest_delta_q12_4(world.ship.x, b.x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
                    let dy =
                        shortest_delta_q12_4(world.ship.y, b.y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
                    let dist = (dx * dx + dy * dy).sqrt();
                    EntitySnapshot {
                        x: b.x,
                        y: b.y,
                        vx: b.vx,
                        vy: b.vy,
                        distance: dist,
                        relative_velocity_toward_ship: 0.0,
                    }
                })
                .filter(|e| e.distance < 200.0)
                .collect();

            let x_px = world.ship.x as f64 / 16.0;
            let y_px = world.ship.y as f64 / 16.0;
            let was_cornered = (x_px < 120.0 || x_px > 480.0) && (y_px < 90.0 || y_px > 360.0);

            let death_record = DeathRecord {
                frame: self.frame_count,
                score: world.score,
                cause,
                ship_x: world.ship.x,
                ship_y: world.ship.y,
                ship_vx: world.ship.vx,
                ship_vy: world.ship.vy,
                ship_angle: world.ship.angle,
                nearby_asteroids,
                nearby_saucers,
                nearby_bullets,
                recent_actions: self.action_history.iter().cloned().collect(),
                threat_count_30_frames_ago: self
                    .action_history
                    .iter()
                    .rev()
                    .nth(30)
                    .map(|a| a.target_count)
                    .unwrap_or(0),
                was_cornered,
                speed_at_death: ((world.ship.vx as f64 / 256.0).powi(2)
                    + (world.ship.vy as f64 / 256.0).powi(2))
                .sqrt(),
                frames_since_last_kill: world.time_since_last_kill,
                bullets_in_flight: world.bullets.len(),
            };

            self.learning_db.record_death(death_record);

            if self.cfg.learning_enabled {
                self.learning_db.save(self.cfg.learning_db_path);
            }
        }

        // Track successful hits by monitoring score changes
        if world.score > self.last_score {
            let score_delta = world.score - self.last_score;
            match score_delta {
                20 | 50 | 100 | 200 | 1000 => {
                    self.learning_db.record_hit();
                }
                _ => {}
            }
        }

        self.update_aggression(world);

        if world.is_game_over || !world.ship.can_control {
            return no_input();
        }

        // Record this action in history
        let pred_now = PredictedShip {
            x: world.ship.x,
            y: world.ship.y,
            vx: world.ship.vx,
            vy: world.ship.vy,
            angle: world.ship.angle,
            radius: world.ship.radius,
            fire_cooldown: world.ship.fire_cooldown,
        };
        let nearest_threat_now = self.nearest_threat_distance_px(world, pred_now);
        let (active_ship_bullets, shortest_ship_bullet_life) =
            own_bullet_in_flight_stats(&world.bullets);
        let fire_locked_base =
            active_ship_bullets > 0 && shortest_ship_bullet_life > 2 && nearest_threat_now > 92.0;

        let mut best_action = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;

        for action in 0x00u8..=0x0F {
            if fire_locked_base && (action & 0x08) != 0 {
                let pred_fire = self.predict_ship(world, action);
                let covered = self
                    .target_score(world, pred_fire)
                    .map(|plan| target_already_covered_by_ship_bullets(plan.track, &world.bullets))
                    .unwrap_or(!self.has_uncovered_target(world));
                if covered {
                    continue;
                }
            }

            let utility = self.action_utility(world, action);
            if utility > best_value {
                best_value = utility;
                best_action = action;
            }
        }

        // Record missed shots
        let input = decode_input_byte(best_action);
        if input.fire {
            let pred_fire = self.predict_ship(world, best_action);
            let fire_quality = self.estimate_fire_quality(world, pred_fire);

            // Check if this fire hit anything
            let target = self.target_score(world, pred_fire);
            let target_type = target
                .as_ref()
                .map(|t| {
                    // Determine target type
                    TargetType::None // Simplified for now
                })
                .unwrap_or(TargetType::None);

            self.last_fire_estimate = Some(fire_quality);
            self.last_target = Some(target_type);
        }

        // Check previous fire for misses
        if let Some(last_estimate) = self.last_fire_estimate {
            if world.time_since_last_kill > 5 {
                // Previous shot missed
                let pred = PredictedShip {
                    x: world.ship.x,
                    y: world.ship.y,
                    vx: world.ship.vx,
                    vy: world.ship.vy,
                    angle: world.ship.angle,
                    radius: world.ship.radius,
                    fire_cooldown: world.ship.fire_cooldown,
                };
                let nearest_threat = self.nearest_threat_distance_px(world, pred);

                let miss_record = MissedShotRecord {
                    frame: self.frame_count.saturating_sub(10),
                    intended_target: self.last_target.clone().unwrap_or(TargetType::None),
                    bullet_start_x: world.ship.x,
                    bullet_start_y: world.ship.y,
                    bullet_angle: world.ship.angle,
                    ship_vx: world.ship.vx,
                    ship_vy: world.ship.vy,
                    target_count_at_fire: world.asteroids.iter().filter(|a| a.alive).count()
                        + world.saucers.iter().filter(|s| s.alive).count(),
                    nearest_threat_distance: nearest_threat,
                    fire_quality_estimate: last_estimate,
                    was_under_pressure: nearest_threat < 150.0,
                };

                self.learning_db.record_missed_shot(miss_record);
                self.last_fire_estimate = None;
            }
        }

        self.record_action(world, best_action);
        self.last_score = world.score;

        decode_input_byte(best_action)
    }
}

// ============================================================================
// BOT CONSTRUCTOR FUNCTIONS
// ============================================================================

pub fn create_kimi_hunter(version: u32) -> Option<Box<dyn AutopilotBot>> {
    let configs = configs::kimi_hunter_configs();
    let id = format!("kimi-hunter-v{}", version);
    configs
        .iter()
        .find(|c| c.id == id)
        .map(|cfg| Box::new(KimiLearningBot::new(*cfg)) as Box<dyn AutopilotBot>)
}

pub fn create_kimi_survivor(version: u32) -> Option<Box<dyn AutopilotBot>> {
    let configs = configs::kimi_survivor_configs();
    let id = format!("kimi-survivor-v{}", version);
    configs
        .iter()
        .find(|c| c.id == id)
        .map(|cfg| Box::new(KimiLearningBot::new(*cfg)) as Box<dyn AutopilotBot>)
}

pub fn create_kimi_sniper(version: u32) -> Option<Box<dyn AutopilotBot>> {
    // Sniper uses precision config which needs different implementation
    // For now, use search-based with sniper-like params
    let configs = configs::kimi_sniper_configs();
    let id = format!("kimi-sniper-v{}", version);
    configs.iter().find(|c| c.id == id).map(|_| {
        // Map precision config to search config
        let search_cfg = configs::KimiSearchConfig {
            id: "kimi-sniper-v1",
            description: "High-precision bot with strict fire discipline",
            lookahead_frames: 22.0,
            risk_weight_asteroid: 1.45,
            risk_weight_saucer: 2.15,
            risk_weight_bullet: 2.85,
            survival_weight: 2.25,
            aggression_weight: 0.42,
            fire_reward: 0.82,
            shot_penalty: 1.25,
            miss_fire_penalty: 1.95,
            action_penalty: 0.019,
            turn_penalty: 0.026,
            thrust_penalty: 0.023,
            center_weight: 0.52,
            edge_penalty: 0.38,
            speed_soft_cap: 4.0,
            fire_tolerance_bam: 5,
            fire_distance_px: 200.0,
            lurk_trigger_frames: 290,
            lurk_aggression_boost: 1.45,
            learning_enabled: true,
            learning_db_path: "kimi-sniper-v1-learning.json",
        };
        Box::new(KimiLearningBot::new(search_cfg)) as Box<dyn AutopilotBot>
    })
}

pub fn create_kimi_wrap_master(version: u32) -> Option<Box<dyn AutopilotBot>> {
    let configs = configs::kimi_wrap_master_configs();
    let id = format!("kimi-wrap-master-v{}", version);
    configs
        .iter()
        .find(|c| c.id == id)
        .map(|cfg| Box::new(KimiLearningBot::new(*cfg)) as Box<dyn AutopilotBot>)
}

pub fn create_kimi_saucer_killer(version: u32) -> Option<Box<dyn AutopilotBot>> {
    let configs = configs::kimi_saucer_killer_configs();
    let id = format!("kimi-saucer-killer-v{}", version);
    configs
        .iter()
        .find(|c| c.id == id)
        .map(|cfg| Box::new(KimiLearningBot::new(*cfg)) as Box<dyn AutopilotBot>)
}

pub fn create_kimi_super_ship(version: u32) -> Option<Box<dyn AutopilotBot>> {
    let configs = configs::kimi_super_ship_configs();
    let id = format!("kimi-super-ship-v{}", version);
    configs
        .iter()
        .find(|c| c.id == id)
        .map(|cfg| Box::new(KimiLearningBot::new(*cfg)) as Box<dyn AutopilotBot>)
}

/// Create any Kimi bot by ID
pub fn create_kimi_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    if let Some(cfg) = configs::find_search_config(id) {
        return Some(Box::new(KimiLearningBot::new(*cfg)));
    }
    if let Some(_) = configs::find_precision_config(id) {
        // Map precision to search for now
        return create_kimi_sniper(1);
    }
    None
}

/// Get all Kimi bot IDs
pub fn all_kimi_bot_ids() -> Vec<&'static str> {
    configs::all_kimi_bot_ids()
}

/// Get learning report for a bot
pub fn get_learning_report(bot_id: &str) -> Option<String> {
    if let Some(cfg) = configs::find_search_config(bot_id) {
        if cfg.learning_enabled {
            let db = LearningDatabase::load_or_create(cfg.learning_db_path);
            return Some(db.generate_learning_report());
        }
    }
    None
}
