//! claude-chimera: Ensemble hybrid bot.
//!
//! Runs 3 internal evaluators per frame and weights votes by threat level:
//! - High threat -> weight navigator/tortoise style (safety first)
//! - Low threat -> weight vulture/predator style (scoring focus)
//! - Medium -> balanced blend

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

pub struct ChimeraBot;

impl ChimeraBot {
    pub fn new() -> Self {
        Self
    }

    /// Safety evaluator (navigator-style): danger field focus
    fn safety_score(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        let danger = danger_at_point(
            world, pred.x, pred.y, pred.vx, pred.vy,
            22.0, 1.55, 2.2, 3.0,
        );

        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);

        let speed_px = pred.speed_px();

        let mut value = -danger * 2.5;
        value -= (center_dist / 900.0) * 0.55;
        value -= ((140.0 - min_edge).max(0.0) / 140.0) * 0.4;
        if speed_px > 3.8 {
            value -= ((speed_px - 3.8) / 3.8) * 0.4;
        }

        // Penalize any action when very safe
        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
        let control_scale = if danger > 2.0 { 0.15 } else { 1.0 };
        if action != 0 {
            value -= 0.025 * control_scale;
        }
        if input.thrust && speed_px > 3.8 * 0.8 {
            value -= 0.02;
        }
        if action == 0x00 && nearest_threat > 165.0 {
            value += 0.004;
        }

        value
    }

    /// Attack evaluator (predator-style): scoring focus
    fn attack_score(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        // Lighter risk
        let mut risk = 0.0;
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy,
                18.0,
            );
            let safe = (pred.radius + asteroid.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
            let closing = if approach.dot < 0.0 { 1.2 } else { 0.92 };
            risk += 1.15 * closeness * closing;
        }
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                saucer.x, saucer.y, saucer.vx, saucer.vy,
                18.0,
            );
            let safe = (pred.radius + saucer.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
            risk += 1.8 * closeness;
        }
        for bullet in &world.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy,
                bullet.x, bullet.y, bullet.vx, bullet.vy,
                18.0,
            );
            let safe = (pred.radius + bullet.radius + 8) as f64;
            let closeness = (safe / (approach.closest_px + 1.0)).powf(2.2);
            risk += 2.5 * closeness;
        }

        // Heavy attack focus
        let mut attack = 0.0;
        let mut fire_term = 0.0;
        let lurk_active = world.time_since_last_kill >= 260;
        let aggression = if lurk_active { 1.4 } else { 1.0 };

        if let Some(target) = best_target(world, pred) {
            let angle_error = signed_angle_delta(pred.angle, target.aim_angle).abs() as f64;
            let align = (1.0 - (angle_error / 128.0)).clamp(0.0, 1.0);
            attack += target.value * align * aggression;

            if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
                let fire_quality = estimate_fire_quality(pred, world);
                let (active, shortest) = own_bullet_stats(&world.bullets);
                let nearest_saucer = nearest_saucer_distance(world, pred);
                let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
                let is_duplicate = target_already_covered(
                    &world.bullets,
                    target.target_x, target.target_y,
                    target.target_vx, target.target_vy,
                    target.target_radius,
                );
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, 0.14,
                    nearest_saucer, nearest_threat, is_duplicate,
                );
                let emergency_saucer = nearest_saucer < 95.0;

                if !is_duplicate && discipline_ok && (fire_quality >= 0.14 || emergency_saucer) {
                    let fire_align = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
                    fire_term += 1.5 * fire_align * (0.35 + 0.65 * fire_quality);
                    fire_term -= 0.6 * 0.72;
                    if lurk_active {
                        fire_term += 0.35;
                    }
                } else if is_duplicate {
                    fire_term -= 0.6;
                } else {
                    fire_term -= 0.12;
                }
            }
        }

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > 4.6 {
            -((speed_px - 4.6) / 4.6) * 0.3
        } else {
            0.0
        };

        let mut control_term = 0.0;
        if action != 0 {
            control_term -= 0.008;
        }

        -risk * 1.5 + attack + fire_term + control_term + speed_term
    }

    /// Balanced evaluator: middle ground
    fn balanced_score(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        let danger = danger_at_point(
            world, pred.x, pred.y, pred.vx, pred.vy,
            20.0, 1.35, 2.0, 2.7,
        );

        let mut attack = 0.0;
        let mut fire_term = 0.0;
        let lurk_active = world.time_since_last_kill >= 290;

        if let Some(target) = best_target(world, pred) {
            let angle_error = signed_angle_delta(pred.angle, target.aim_angle).abs() as f64;
            let align = (1.0 - (angle_error / 128.0)).clamp(0.0, 1.0);
            let aggression = if lurk_active { 1.3 } else { 0.8 };
            attack += target.value * align * aggression;

            if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
                let fire_quality = estimate_fire_quality(pred, world);
                let (active, shortest) = own_bullet_stats(&world.bullets);
                let nearest_saucer = nearest_saucer_distance(world, pred);
                let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
                let is_duplicate = target_already_covered(
                    &world.bullets,
                    target.target_x, target.target_y,
                    target.target_vx, target.target_vy,
                    target.target_radius,
                );
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, 0.18,
                    nearest_saucer, nearest_threat, is_duplicate,
                );

                if !is_duplicate && discipline_ok && fire_quality >= 0.18 {
                    let fire_align = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
                    fire_term += 1.15 * fire_align * (0.35 + 0.65 * fire_quality);
                    fire_term -= 0.8 * 0.72;
                    if lurk_active {
                        fire_term += 0.25;
                    }
                } else if is_duplicate {
                    fire_term -= 0.65;
                } else {
                    fire_term -= 0.15;
                }
            }
        }

        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * 0.48;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > 4.2 {
            -((speed_px - 4.2) / 4.2) * 0.35
        } else {
            0.0
        };

        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
        let mut control_term = 0.0;
        if action != 0 {
            control_term -= 0.012;
        }
        if action == 0x00 && nearest_threat > 165.0 {
            control_term += 0.003;
        }

        -danger * 2.0 + attack + fire_term + control_term + center_term + speed_term
    }
}

impl AutopilotBot for ChimeraBot {
    fn id(&self) -> &'static str {
        "claude-chimera"
    }
    fn description(&self) -> &'static str {
        "Ensemble hybrid weighting sub-strategies by threat level."
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

        // Determine threat level to set weights
        let ship_pred = PredictedShip::from_world(world);
        let nearest_threat = nearest_threat_distance(world, ship_pred.x, ship_pred.y);
        let danger = danger_at_point(
            world, ship_pred.x, ship_pred.y, ship_pred.vx, ship_pred.vy,
            16.0, 1.3, 1.9, 2.5,
        );

        let (w_safety, w_attack, w_balanced) = if danger > 2.5 || nearest_threat < 60.0 {
            // High threat: safety dominates
            (0.6, 0.1, 0.3)
        } else if danger > 1.0 || nearest_threat < 120.0 {
            // Medium threat: balanced
            (0.3, 0.25, 0.45)
        } else {
            // Low threat: attack focus
            (0.15, 0.45, 0.4)
        };

        let mut best_action = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;

        for action in 0x00u8..=0x0F {
            let safety = self.safety_score(world, action);
            let attack = self.attack_score(world, action);
            let balanced = self.balanced_score(world, action);
            let weighted = w_safety * safety + w_attack * attack + w_balanced * balanced;

            if weighted > best_value {
                best_value = weighted;
                best_action = action;
            }
        }

        decode_input_byte(best_action)
    }
}
