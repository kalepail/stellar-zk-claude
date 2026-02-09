//! claude-tortoise: Deep survival bot.
//!
//! Ultra-conservative survivalist:
//! - Extremely high risk weights, minimal action penalties
//! - Only shoot when: (a) directly threatened, or (b) guaranteed hit on small saucer
//! - Focus on dodging first, scoring second
//! - Maximize extra lives through occasional safe kills near 10k thresholds

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

pub struct TortoiseBot;

impl TortoiseBot {
    pub fn new() -> Self {
        Self
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        // Very high risk sensitivity
        let mut risk = 0.0;
        let lookahead = 24.0;
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy,
                lookahead,
            );
            let safe = (pred.radius + asteroid.radius + 10) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.2);
            let immediate = (safe / (approach.immediate_px + 1.0)).powf(1.5);
            let closing = if approach.dot < 0.0 { 1.3 } else { 0.9 };
            let time_boost = 1.0 + ((lookahead - approach.t_closest) / lookahead) * 0.5;
            risk += 1.75 * (0.75 * closeness + 0.25 * immediate) * closing * time_boost;
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
            let safe = (pred.radius + saucer.radius + 10) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.2);
            let closing = if approach.dot < 0.0 { 1.3 } else { 0.9 };
            let w = if saucer.small { 2.8 } else { 2.4 };
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
            let safe = (pred.radius + bullet.radius + 10) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.4);
            let closing = if approach.dot < 0.0 { 1.5 } else { 0.9 };
            risk += 3.5 * closeness * closing;
        }

        // Only modest attack - score through survival
        let mut attack_term = 0.0;
        let mut fire_term = 0.0;
        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);

        // Near extra-life threshold? Be slightly more aggressive
        let near_extra_life = world.next_extra_life_score > world.score
            && (world.next_extra_life_score - world.score) <= 2_000;

        // Prevent lurk death
        let near_lurk = world.time_since_last_kill >= 320;
        let in_lurk = world.time_since_last_kill >= LURK_TIME_THRESHOLD_FRAMES;

        if let Some(target) = best_target(world, pred) {
            let angle_error = signed_angle_delta(pred.angle, target.aim_angle).abs() as f64;
            let align = (1.0 - (angle_error / 128.0)).clamp(0.0, 1.0);

            let aggression = if near_lurk || in_lurk {
                0.6
            } else if near_extra_life {
                0.45
            } else {
                0.25
            };

            attack_term += target.value * align * aggression;

            if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
                let fire_quality = estimate_fire_quality(pred, world);
                let (active, shortest) = own_bullet_stats(&world.bullets);
                let nearest_saucer = nearest_saucer_distance(world, pred);
                let is_duplicate = target_already_covered(
                    &world.bullets,
                    target.target_x, target.target_y,
                    target.target_vx, target.target_vy,
                    target.target_radius,
                );

                // Very high quality floor - only take guaranteed shots
                let min_quality = if near_lurk { 0.22 } else { 0.35 };
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, min_quality,
                    nearest_saucer, nearest_threat, is_duplicate,
                );
                let emergency_saucer = nearest_saucer < 80.0 && fire_quality >= 0.2;

                if !is_duplicate && discipline_ok
                    && (fire_quality >= min_quality || emergency_saucer)
                {
                    let fire_align = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
                    fire_term += 0.85 * fire_align * (0.4 + 0.6 * fire_quality);
                    fire_term -= 1.4 * 0.72;
                    if in_lurk {
                        fire_term += 0.4;
                    }
                } else if is_duplicate {
                    fire_term -= 1.2;
                } else {
                    fire_term -= 1.8 * (min_quality - fire_quality).max(0.0) * 0.45;
                    fire_term -= 0.5;
                }
            }
        }

        // Strong position preference
        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * 0.55;

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * 0.42;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > 3.6 {
            -((speed_px - 3.6) / 3.6) * 0.5
        } else {
            0.0
        };

        // Heavy control penalties to stay still when safe
        let control_scale = if risk > 3.0 { 0.15 } else if risk > 1.5 { 0.3 } else { 1.0 };
        let mut control_term = 0.0;
        if action != 0 {
            control_term -= 0.028 * control_scale;
        }
        if input.left || input.right {
            control_term -= 0.035 * control_scale;
            if nearest_threat > 180.0 {
                control_term -= 0.02;
            }
        }
        if input.thrust {
            control_term -= 0.03 * control_scale;
            if nearest_threat > 190.0 && speed_px > 3.6 * 0.72 {
                control_term -= 0.02;
            }
            if speed_px > 3.6 * 1.05 {
                control_term -= 0.035;
            }
        } else if action == 0x00 && nearest_threat > 165.0 {
            control_term += 0.005;
        }

        -risk * 2.8 + attack_term + fire_term + control_term + center_term + edge_term + speed_term
    }
}

impl AutopilotBot for TortoiseBot {
    fn id(&self) -> &'static str {
        "claude-tortoise"
    }
    fn description(&self) -> &'static str {
        "Ultra-conservative deep survival bot prioritizing dodging over scoring."
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
