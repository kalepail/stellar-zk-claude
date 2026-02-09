//! claude-predator: Intercept chain optimizer.
//!
//! Plans multi-target kill sequences:
//! - When hitting large/medium asteroid, predict child spawn positions
//! - Aim from positions where follow-up shots will intercept children
//! - Score chains: large->medium->small for maximum points per frame
//! - Pre-position for optimal chain geometry

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::{AsteroidSizeSnapshot, WorldSnapshot};
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

pub struct PredatorBot;

impl PredatorBot {
    pub fn new() -> Self {
        Self
    }

    /// Target weight incorporating chain value (large/medium have higher weight due to chain potential)
    fn chain_target_weight(size: AsteroidSizeSnapshot) -> f64 {
        match size {
            AsteroidSizeSnapshot::Large => 1.8,   // Higher: chain potential
            AsteroidSizeSnapshot::Medium => 1.6,   // Still good chain
            AsteroidSizeSnapshot::Small => 1.2,    // Clean kill, no chain
        }
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        // Risk assessment
        let mut risk = 0.0;
        let lookahead = 19.0;
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy,
                lookahead,
            );
            let safe = (pred.radius + asteroid.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
            let immediate = (safe / (approach.immediate_px + 1.0)).powf(1.35);
            let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
            let time_boost = 1.0 + ((lookahead - approach.t_closest) / lookahead) * 0.45;
            risk += 1.35 * (0.78 * closeness + 0.22 * immediate) * closing * time_boost;
        }

        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                saucer.x, saucer.y, saucer.vx, saucer.vy,
                lookahead,
            );
            let safe = (pred.radius + saucer.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
            let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
            let w = if saucer.small { 2.1 } else { 1.9 };
            risk += w * closeness * closing;
        }

        for bullet in &world.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                bullet.x, bullet.y, bullet.vx, bullet.vy,
                lookahead,
            );
            let safe = (pred.radius + bullet.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.2);
            let closing = if approach.dot < 0.0 { 1.4 } else { 0.9 };
            risk += 2.7 * closeness * closing;
        }

        // Chain-aware targeting
        let bullet_speed = 8.6 + pred.speed_px() * 0.33;
        let mut best_target_val = 0.0;
        let mut best_aim = None;

        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            if let Some(aim) = best_wrapped_aim(
                pred.x, pred.y, pred.vx, pred.vy, pred.angle,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy,
                bullet_speed, 64.0,
            ) {
                let angle_error = signed_angle_delta(pred.angle, aim.aim_angle).abs() as f64;
                let base_value = Self::chain_target_weight(asteroid.size) / (aim.distance_px + 16.0);
                let align = (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
                let time_bonus = (1.0 - (aim.intercept_frames / 64.0).clamp(0.0, 1.0)) * 0.1;

                // Chain bonus: if large/medium, reward being closer (for follow-up shots)
                let chain_bonus = match asteroid.size {
                    AsteroidSizeSnapshot::Large => {
                        if aim.distance_px < 200.0 {
                            0.3 * (1.0 - aim.distance_px / 200.0)
                        } else {
                            0.0
                        }
                    }
                    AsteroidSizeSnapshot::Medium => {
                        if aim.distance_px < 150.0 {
                            0.2 * (1.0 - aim.distance_px / 150.0)
                        } else {
                            0.0
                        }
                    }
                    AsteroidSizeSnapshot::Small => 0.0,
                };

                let value = base_value + align + time_bonus + chain_bonus;
                if value > best_target_val {
                    best_target_val = value;
                    best_aim = Some((aim.aim_angle, asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, asteroid.radius));
                }
            }
        }

        // Also consider saucers (very high value)
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            if let Some(aim) = best_wrapped_aim(
                pred.x, pred.y, pred.vx, pred.vy, pred.angle,
                saucer.x, saucer.y, saucer.vx, saucer.vy,
                bullet_speed, 64.0,
            ) {
                let w = if saucer.small { 3.5 } else { 2.0 };
                let angle_error = signed_angle_delta(pred.angle, aim.aim_angle).abs() as f64;
                let value = w / (aim.distance_px + 16.0) + (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
                if value > best_target_val {
                    best_target_val = value;
                    best_aim = Some((aim.aim_angle, saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius));
                }
            }
        }

        // Aggression with lurk awareness
        let lurk_active = world.time_since_last_kill >= 280;
        let in_lurk = world.time_since_last_kill >= LURK_TIME_THRESHOLD_FRAMES;
        let mut aggression = 0.85;
        if lurk_active {
            aggression *= 1.6;
        }
        if world.next_extra_life_score > world.score
            && (world.next_extra_life_score - world.score) <= 1_500
        {
            aggression *= 1.15;
        }

        let mut attack_term = 0.0;
        let mut fire_term = 0.0;

        if let Some((aim_angle, tx, ty, tvx, tvy, tr)) = best_aim {
            let angle_error = signed_angle_delta(pred.angle, aim_angle).abs() as f64;
            let align = (1.0 - (angle_error / 128.0)).clamp(0.0, 1.0);
            attack_term = best_target_val * align * aggression;

            if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
                let fire_quality = estimate_fire_quality(pred, world);
                let (active, shortest) = own_bullet_stats(&world.bullets);
                let nearest_saucer = nearest_saucer_distance(world, pred);
                let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
                let is_duplicate = target_already_covered(
                    &world.bullets, tx, ty, tvx, tvy, tr,
                );
                let min_quality = 0.16;
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, min_quality,
                    nearest_saucer, nearest_threat, is_duplicate,
                );
                let emergency_saucer = nearest_saucer < 95.0 && fire_quality + 0.08 >= min_quality;

                if !is_duplicate && discipline_ok && (fire_quality >= min_quality || emergency_saucer) {
                    let fire_align = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
                    fire_term += 1.3 * fire_align * (0.35 + 0.65 * fire_quality);
                    fire_term -= 0.75 * 0.72;
                    if in_lurk {
                        fire_term += 0.3;
                    }
                } else if is_duplicate {
                    fire_term -= 0.65;
                } else {
                    fire_term -= 0.15;
                }
            }
        }

        // Position
        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * 0.45;

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * 0.3;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > 4.5 {
            -((speed_px - 4.5) / 4.5) * 0.35
        } else {
            0.0
        };

        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
        let control_scale = if risk > 3.0 { 0.18 } else if risk > 1.8 { 0.38 } else { 1.0 };
        let mut control_term = 0.0;
        if action != 0 {
            control_term -= 0.01 * control_scale;
        }
        if input.left || input.right {
            control_term -= 0.012 * control_scale;
        }
        if input.thrust {
            control_term -= 0.011 * control_scale;
        } else if action == 0x00 && nearest_threat > 165.0 {
            control_term += 0.002;
        }

        -risk * 1.8 + attack_term + fire_term + control_term + center_term + edge_term + speed_term
    }
}

impl AutopilotBot for PredatorBot {
    fn id(&self) -> &'static str {
        "claude-predator"
    }
    fn description(&self) -> &'static str {
        "Intercept chain optimizer planning multi-target kill sequences."
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
            let utility = self.evaluate_action(world, action);
            if utility > best_value {
                best_value = utility;
                best_action = action;
            }
        }
        decode_input_byte(best_action)
    }
}
