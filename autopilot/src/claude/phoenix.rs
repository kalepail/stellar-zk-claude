//! claude-phoenix: Adaptive phase bot.
//!
//! Changes behavior based on game state:
//! - Early (waves 1-3): Aggressive clearing, build extra lives via 10k thresholds
//! - Mid (waves 4-7): Balanced, prioritize small saucers (1000pts) when available
//! - Late (wave 8+): Survival focus, only take guaranteed shots

use crate::bots::AutopilotBot;
use crate::claude::common::*;
use asteroids_verifier_core::constants::*;
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Early,
    Mid,
    Late,
}

pub struct PhoenixBot {
    phase: Phase,
}

impl PhoenixBot {
    pub fn new() -> Self {
        Self {
            phase: Phase::Early,
        }
    }

    fn update_phase(&mut self, world: &WorldSnapshot) {
        self.phase = if world.wave <= 3 {
            Phase::Early
        } else if world.wave <= 7 {
            Phase::Mid
        } else {
            Phase::Late
        };
    }

    fn phase_params(&self) -> PhaseParams {
        match self.phase {
            Phase::Early => PhaseParams {
                risk_weight_asteroid: 1.15,
                risk_weight_saucer: 1.8,
                risk_weight_bullet: 2.5,
                survival_weight: 1.4,
                aggression: 0.95,
                fire_reward: 1.4,
                shot_penalty: 0.65,
                miss_fire_penalty: 0.85,
                min_fire_quality: 0.15,
                speed_soft_cap: 4.8,
                center_weight: 0.4,
                edge_penalty: 0.25,
                lookahead: 18.0,
                lurk_trigger: 280,
                lurk_boost: 2.0,
            },
            Phase::Mid => PhaseParams {
                risk_weight_asteroid: 1.35,
                risk_weight_saucer: 2.0,
                risk_weight_bullet: 2.7,
                survival_weight: 1.75,
                aggression: 0.72,
                fire_reward: 1.2,
                shot_penalty: 0.85,
                miss_fire_penalty: 1.1,
                min_fire_quality: 0.2,
                speed_soft_cap: 4.3,
                center_weight: 0.48,
                edge_penalty: 0.32,
                lookahead: 20.0,
                lurk_trigger: 300,
                lurk_boost: 1.7,
            },
            Phase::Late => PhaseParams {
                risk_weight_asteroid: 1.65,
                risk_weight_saucer: 2.4,
                risk_weight_bullet: 3.2,
                survival_weight: 2.5,
                aggression: 0.4,
                fire_reward: 0.9,
                shot_penalty: 1.3,
                miss_fire_penalty: 1.7,
                min_fire_quality: 0.3,
                speed_soft_cap: 3.8,
                center_weight: 0.55,
                edge_penalty: 0.4,
                lookahead: 22.0,
                lurk_trigger: 330,
                lurk_boost: 1.4,
            },
        }
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
        lookahead: f64,
    ) -> f64 {
        let approach = torus_relative_approach(
            pred.x, pred.y, pred.vx, pred.vy, ex, ey, evx, evy, lookahead,
        );
        let safe = (pred.radius + radius + 8) as f64;
        let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
        let immediate_boost = (safe / (approach.immediate_px + 1.0)).powf(1.35);
        let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
        let time_boost = 1.0 + ((lookahead - approach.t_closest) / lookahead) * 0.45;
        weight * (0.78 * closeness + 0.22 * immediate_boost) * closing * time_boost
    }

    fn action_utility(&self, world: &WorldSnapshot, input_byte: u8) -> f64 {
        let params = self.phase_params();
        let pred = predict_ship(world, input_byte);
        let input = decode_input_byte(input_byte);

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
                params.risk_weight_asteroid,
                params.lookahead,
            );
        }
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            let w = if saucer.small {
                params.risk_weight_saucer * 1.28
            } else {
                params.risk_weight_saucer
            };
            risk += self.entity_risk(
                pred,
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                w,
                params.lookahead,
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
                params.risk_weight_bullet,
                params.lookahead,
            );
        }

        // Aggression with lurk boost
        let mut aggression = params.aggression;
        if world.time_since_last_kill >= params.lurk_trigger {
            aggression *= params.lurk_boost;
        }
        // Extra-life proximity boost
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
            fire_alignment = (1.0 - (angle_error / 8.0_f64.max(1.0))).clamp(0.0, 1.0);
            if plan.distance_px < 280.0 {
                attack += 0.16;
            }
            if plan.intercept_frames <= 14.0 {
                attack += 0.05;
            }
        }

        // Position scoring
        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        let center_term = -(center_dist / 900.0) * params.center_weight;

        let left_edge = pred.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - pred.x) as f64 / 16.0;
        let top_edge = pred.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - pred.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * params.edge_penalty;

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > params.speed_soft_cap {
            -((speed_px - params.speed_soft_cap) / params.speed_soft_cap.max(0.1)) * 0.35
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
                params.min_fire_quality,
                nearest_saucer,
                nearest_threat,
                is_duplicate,
            );
            let emergency_saucer =
                nearest_saucer < 95.0 && fire_quality + 0.08 >= params.min_fire_quality;

            if !is_duplicate
                && discipline_ok
                && (fire_quality >= params.min_fire_quality || emergency_saucer)
            {
                fire_term += params.fire_reward * fire_alignment * (0.35 + 0.65 * fire_quality);
                fire_term -= params.shot_penalty * 0.72;
            } else if is_duplicate {
                fire_term -= params.shot_penalty * 0.68;
            } else if !discipline_ok {
                fire_term -= params.shot_penalty * 0.45;
            } else {
                fire_term -= params.miss_fire_penalty
                    * (params.min_fire_quality - fire_quality).max(0.0)
                    * 0.45;
                fire_term -= params.shot_penalty * 0.2;
            }
            if world.time_since_last_kill >= params.lurk_trigger {
                fire_term += params.fire_reward * 0.25;
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
            control_term -= 0.01 * control_scale;
        }
        if input.left || input.right {
            control_term -= 0.012 * control_scale;
            if nearest_threat > 180.0 {
                control_term -= 0.006;
            }
        }
        if input.thrust {
            control_term -= 0.011 * control_scale;
            if nearest_threat > 190.0 && speed_px > params.speed_soft_cap * 0.72 {
                control_term -= 0.007;
            }
            if speed_px > params.speed_soft_cap * 1.05 {
                control_term -= 0.012;
            }
        } else if input_byte == 0x00 && nearest_threat > 165.0 {
            control_term += 0.002;
        }

        -risk * params.survival_weight
            + attack * aggression
            + fire_term
            + control_term
            + center_term
            + edge_term
            + speed_term
    }
}

struct PhaseParams {
    risk_weight_asteroid: f64,
    risk_weight_saucer: f64,
    risk_weight_bullet: f64,
    survival_weight: f64,
    aggression: f64,
    fire_reward: f64,
    shot_penalty: f64,
    miss_fire_penalty: f64,
    min_fire_quality: f64,
    speed_soft_cap: f64,
    center_weight: f64,
    edge_penalty: f64,
    lookahead: f64,
    lurk_trigger: i32,
    lurk_boost: f64,
}

impl AutopilotBot for PhoenixBot {
    fn id(&self) -> &'static str {
        "claude-phoenix"
    }
    fn description(&self) -> &'static str {
        "Adaptive phase bot: aggressive early, balanced mid, survival late."
    }
    fn reset(&mut self, _seed: u32) {
        self.phase = Phase::Early;
    }
    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: false,
            };
        }
        self.update_phase(world);

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
