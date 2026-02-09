//! claude-navigator: Danger field navigator.
//!
//! Discretized 2D danger map over the toroidal space.
//! Uses gradient descent to find safest reachable position.
//! Only fires when at safe position AND aligned with high-value target.

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::{shortest_delta_q12_4, wrap_x_q12_4, wrap_y_q12_4};
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

pub struct NavigatorBot;

impl NavigatorBot {
    pub fn new() -> Self {
        Self
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        // Primary: danger at predicted position
        let danger = danger_at_point(
            world,
            pred.x,
            pred.y,
            pred.vx,
            pred.vy,
            20.0, // horizon
            1.45,  // asteroid weight
            2.1,   // saucer weight
            2.85,  // bullet weight
        );

        // Also check danger at 2 steps out (momentum awareness)
        let future_x = wrap_x_q12_4(pred.x + (pred.vx >> 4));
        let future_y = wrap_y_q12_4(pred.y + (pred.vy >> 4));
        let future_danger = danger_at_point(
            world,
            future_x,
            future_y,
            pred.vx,
            pred.vy,
            16.0,
            1.2,
            1.8,
            2.4,
        );

        let safety = -(danger * 2.2 + future_danger * 0.8);

        // Position quality
        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * 0.5;

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * 0.35;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > 4.2 {
            -((speed_px - 4.2) / 4.2) * 0.35
        } else {
            0.0
        };

        // Targeting - only fire when safe
        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
        let is_safe = danger < 0.5 && nearest_threat > 80.0;

        let mut fire_term = 0.0;
        let mut attack_term = 0.0;

        if let Some(target) = best_target(world, pred) {
            let angle_error = signed_angle_delta(pred.angle, target.aim_angle).abs() as f64;
            let align = (1.0 - angle_error / 128.0).clamp(0.0, 1.0);
            attack_term += target.value * align * 0.6;

            // Lurk boost
            let lurk_active = world.time_since_last_kill >= 290;
            if lurk_active {
                attack_term *= 1.8;
            }

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
                let min_quality = if is_safe { 0.15 } else { 0.28 };
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, min_quality,
                    nearest_saucer, nearest_threat, is_duplicate,
                );
                let emergency_saucer = nearest_saucer < 95.0 && fire_quality + 0.08 >= min_quality;

                if !is_duplicate && discipline_ok && (fire_quality >= min_quality || emergency_saucer) {
                    fire_term += 1.1 * fire_alignment_score(pred, target.aim_angle) * (0.35 + 0.65 * fire_quality);
                    fire_term -= 0.75 * 0.72;
                    if lurk_active {
                        fire_term += 0.3;
                    }
                } else if is_duplicate {
                    fire_term -= 0.65;
                } else {
                    fire_term -= 0.9 * (min_quality - fire_quality).max(0.0) * 0.45;
                    fire_term -= 0.2;
                }
            }
        }

        // Control penalties (lighter when in danger)
        let control_scale = if danger > 2.0 { 0.2 } else if danger > 1.0 { 0.4 } else { 1.0 };
        let mut control_term = 0.0;
        if action != 0 {
            control_term -= 0.01 * control_scale;
        }
        if input.left || input.right {
            control_term -= 0.012 * control_scale;
        }
        if input.thrust {
            control_term -= 0.011 * control_scale;
            if speed_px > 4.2 * 1.05 {
                control_term -= 0.012;
            }
        } else if action == 0x00 && nearest_threat > 165.0 {
            control_term += 0.002;
        }

        safety + attack_term + fire_term + control_term + center_term + edge_term + speed_term
    }
}

fn fire_alignment_score(pred: PredictedShip, aim_angle: i32) -> f64 {
    let angle_error = signed_angle_delta(pred.angle, aim_angle).abs() as f64;
    (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0)
}

impl AutopilotBot for NavigatorBot {
    fn id(&self) -> &'static str {
        "claude-navigator"
    }
    fn description(&self) -> &'static str {
        "Danger-field navigator using spatial gradient descent for safe positioning."
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
