//! claude-vulture: Saucer farmer.
//!
//! Exploits the anti-lurk mechanic for high scoring:
//! - Track time_since_last_kill relative to 360-frame lurk threshold
//! - When close to threshold, make one safe kill to reset timer
//! - When saucers spawn, aggressively pursue small saucers (1000 pts each)
//! - Deliberately avoid clearing all asteroids to delay wave transitions when farming

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

pub struct VultureBot;

impl VultureBot {
    pub fn new() -> Self {
        Self
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);

        let lurk_timer = world.time_since_last_kill;
        let near_lurk = lurk_timer >= 300;
        let in_lurk = lurk_timer >= LURK_TIME_THRESHOLD_FRAMES;
        let has_saucers = world.saucers.iter().any(|s| s.alive);
        let _has_small_saucer = world.saucers.iter().any(|s| s.alive && s.small);
        let few_asteroids = world.asteroids.iter().filter(|a| a.alive).count() <= 3;

        // Mode: farming = preserve asteroids, hunt saucers
        // Mode: anti-lurk = need a kill soon
        // Mode: clear = normal clearing
        let farming = has_saucers || in_lurk || (near_lurk && few_asteroids);

        // Risk assessment
        let mut risk = 0.0;
        let lookahead = 20.0;
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
            let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
            let time_boost = 1.0 + ((lookahead - approach.t_closest) / lookahead) * 0.45;
            risk += 1.3 * closeness * closing * time_boost;
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
            let w = if saucer.small { 2.2 } else { 1.9 };
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
            risk += 2.8 * closeness * closing;
        }

        // Targeting with saucer priority
        let mut attack_term = 0.0;
        let mut fire_term = 0.0;
        let target_plan = best_target(world, pred);

        // Boost saucer targeting weight significantly
        let mut saucer_target: Option<(i32, f64, i32, i32, i32, i32, i32)> = None;
        let bullet_speed = 8.6 + pred.speed_px() * 0.33;
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            if let Some(aim) = best_wrapped_aim(
                pred.x, pred.y, pred.vx, pred.vy, pred.angle,
                saucer.x, saucer.y, saucer.vx, saucer.vy,
                bullet_speed, 64.0,
            ) {
                let w = if saucer.small { 5.0 } else { 2.5 };
                let angle_error = signed_angle_delta(pred.angle, aim.aim_angle).abs() as f64;
                let value = w / (aim.distance_px + 16.0)
                    + (1.0 - (angle_error / 128.0)).max(0.0) * 0.8;
                match saucer_target {
                    None => {
                        saucer_target = Some((aim.aim_angle, value, saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius));
                    }
                    Some((_, v, ..)) if value > v => {
                        saucer_target = Some((aim.aim_angle, value, saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius));
                    }
                    _ => {}
                }
            }
        }

        // Use saucer target if farming, otherwise best generic target
        let active_target = if farming {
            saucer_target.or_else(|| {
                target_plan.map(|p| (p.aim_angle, p.value, p.target_x, p.target_y, p.target_vx, p.target_vy, p.target_radius))
            })
        } else {
            target_plan.map(|p| (p.aim_angle, p.value, p.target_x, p.target_y, p.target_vx, p.target_vy, p.target_radius))
                .or(saucer_target)
        };

        if let Some((aim_angle, value, tx, ty, tvx, tvy, tr)) = active_target {
            let angle_error = signed_angle_delta(pred.angle, aim_angle).abs() as f64;
            let align = (1.0 - (angle_error / 128.0)).clamp(0.0, 1.0);

            // In farming mode, heavily reward aiming at saucers
            let aggression = if farming && has_saucers {
                1.2
            } else if near_lurk && !has_saucers {
                // Need a kill to prevent lurk - be more aggressive
                1.5
            } else {
                0.7
            };

            attack_term += value * align * aggression;

            if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
                let fire_quality = estimate_fire_quality(pred, world);
                let (active, shortest) = own_bullet_stats(&world.bullets);
                let nearest_saucer = nearest_saucer_distance(world, pred);
                let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);
                let is_duplicate = target_already_covered(
                    &world.bullets, tx, ty, tvx, tvy, tr,
                );
                let min_quality = if farming && has_saucers { 0.12 } else { 0.18 };
                let discipline_ok = disciplined_fire_ok(
                    active, shortest, fire_quality, min_quality,
                    nearest_saucer, nearest_threat, is_duplicate,
                );
                let emergency_saucer = nearest_saucer < 95.0 && fire_quality + 0.08 >= min_quality;

                // In farming mode, avoid shooting asteroids if few remain (preserve them)
                let should_preserve_asteroids = farming && few_asteroids && !has_saucers;

                if !is_duplicate && discipline_ok
                    && (fire_quality >= min_quality || emergency_saucer)
                    && !should_preserve_asteroids
                {
                    let fire_align = (1.0 - (angle_error / 8.0)).clamp(0.0, 1.0);
                    fire_term += 1.2 * fire_align * (0.35 + 0.65 * fire_quality);
                    fire_term -= 0.7 * 0.72;
                    if in_lurk {
                        fire_term += 0.35;
                    }
                } else if is_duplicate {
                    fire_term -= 0.65;
                } else if should_preserve_asteroids {
                    fire_term -= 0.5;
                } else {
                    fire_term -= 0.15;
                }
            }
        }

        // Position scoring
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
        let speed_term = if speed_px > 4.4 {
            -((speed_px - 4.4) / 4.4) * 0.35
        } else {
            0.0
        };

        // Control penalties
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

        -risk * 1.9 + attack_term + fire_term + control_term + center_term + edge_term + speed_term
    }
}

impl AutopilotBot for VultureBot {
    fn id(&self) -> &'static str {
        "claude-vulture"
    }
    fn description(&self) -> &'static str {
        "Saucer farmer exploiting anti-lurk mechanic for high-value kills."
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
