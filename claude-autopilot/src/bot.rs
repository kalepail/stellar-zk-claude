use crate::config::BotConfig;
use crate::torus::*;
use asteroids_verifier_core::constants::{
    SHIP_BULLET_LIMIT, WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::WorldSnapshot;
use asteroids_verifier_core::tape::{decode_input_byte, encode_input_byte, FrameInput};

pub struct Bot {
    pub cfg: BotConfig,
    orbit_sign: i32,
}

impl Bot {
    pub fn new(cfg: BotConfig) -> Self {
        Self {
            cfg,
            orbit_sign: 1,
        }
    }

    pub fn reset(&mut self, seed: u32) {
        let hash = self
            .cfg
            .id
            .bytes()
            .fold(0u32, |acc, b| acc.rotate_left(5) ^ (b as u32));
        let mixed = seed ^ hash ^ 0xBADC_0DED;
        self.orbit_sign = if mixed & 1 == 0 { 1 } else { -1 };
    }

    pub fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: false,
            };
        }

        let pred_now = PredictedShip {
            x: world.ship.x,
            y: world.ship.y,
            vx: world.ship.vx,
            vy: world.ship.vy,
            angle: world.ship.angle,
            radius: world.ship.radius,
            fire_cooldown: world.ship.fire_cooldown,
        };
        let nearest_threat_now = nearest_threat_distance(world, pred_now.x, pred_now.y);
        let (active_ship_bullets, shortest_ship_bullet_life) = own_bullet_stats(&world.bullets);
        let fire_locked_base =
            active_ship_bullets > 0 && shortest_ship_bullet_life > 2 && nearest_threat_now > 92.0;

        let mut best_action = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        for action in 0x00u8..=0x0F {
            if fire_locked_base && (action & 0x08) != 0 {
                let pred_fire = predict_ship(world, action);
                let target = best_target(world, pred_fire);
                let covered = target
                    .map(|plan| {
                        target_already_covered(
                            &world.bullets,
                            plan.target_x,
                            plan.target_y,
                            plan.target_vx,
                            plan.target_vy,
                            plan.target_radius,
                        )
                    })
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

        decode_input_byte(best_action)
    }

    pub fn next_input_byte(&mut self, world: &WorldSnapshot) -> u8 {
        encode_input_byte(self.next_input(world))
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
            pred.x, pred.y, pred.vx, pred.vy, ex, ey, evx, evy, self.cfg.lookahead,
        );
        let safe = (pred.radius + radius + 8) as f64;
        let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
        let immediate = (safe / (approach.immediate_px + 1.0)).powf(1.35);
        let closing = if approach.dot < 0.0 { 1.25 } else { 0.92 };
        let time_boost =
            1.0 + ((self.cfg.lookahead - approach.t_closest) / self.cfg.lookahead) * 0.45;
        weight * (0.78 * closeness + 0.22 * immediate) * closing * time_boost
    }

    fn fire_quality_floor(&self, world: &WorldSnapshot, pred: PredictedShip, aggression: f64) -> f64 {
        let mut floor = 0.13 + self.cfg.shot_penalty * 0.08 + self.cfg.miss_fire_penalty * 0.06
            - aggression * 0.05;

        if world.time_since_last_kill >= self.cfg.lurk_trigger {
            floor -= 0.02;
        }

        let nearest_saucer = nearest_saucer_distance(world, pred);
        if nearest_saucer < 260.0 {
            floor -= 0.04;
        }
        if nearest_saucer < 160.0 {
            floor -= 0.05;
        }
        if nearest_saucer < 95.0 {
            floor -= 0.08;
        }
        if world.saucers.iter().any(|entry| entry.small) {
            floor -= 0.03;
        }

        floor.clamp(0.05, 0.38).max(self.cfg.min_fire_quality - 0.05)
    }

    fn has_uncovered_target(&self, world: &WorldSnapshot) -> bool {
        for asteroid in &world.asteroids {
            if !asteroid.alive {
                continue;
            }
            if !target_already_covered(
                &world.bullets,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, asteroid.radius,
            ) {
                return true;
            }
        }
        for saucer in &world.saucers {
            if !saucer.alive {
                continue;
            }
            if !target_already_covered(
                &world.bullets,
                saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius,
            ) {
                return true;
            }
        }
        false
    }

    fn action_utility(&self, world: &WorldSnapshot, input_byte: u8) -> f64 {
        let pred = predict_ship(world, input_byte);
        let input = decode_input_byte(input_byte);

        // Risk from entities
        let mut risk = 0.0;
        for asteroid in &world.asteroids {
            risk += self.entity_risk(
                pred,
                asteroid.x, asteroid.y, asteroid.vx, asteroid.vy,
                asteroid.radius,
                self.cfg.risk_weight_asteroid,
            );
        }
        for saucer in &world.saucers {
            let w = if saucer.small {
                self.cfg.risk_weight_saucer * 1.28
            } else {
                self.cfg.risk_weight_saucer
            };
            risk += self.entity_risk(
                pred,
                saucer.x, saucer.y, saucer.vx, saucer.vy,
                saucer.radius,
                w,
            );
        }
        for bullet in &world.saucer_bullets {
            risk += self.entity_risk(
                pred,
                bullet.x, bullet.y, bullet.vx, bullet.vy,
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
            fire_alignment =
                (1.0 - (angle_error / self.cfg.fire_tolerance_bam.max(1) as f64)).clamp(0.0, 1.0);
            if plan.distance_px < self.cfg.fire_distance_px {
                attack += 0.16;
            }
            if plan.intercept_frames <= 14.0 {
                attack += 0.05;
            }
        } else {
            let tangent = (pred.angle + self.orbit_sign * 64) & 0xff;
            let tangent_delta = signed_angle_delta(pred.angle, tangent).abs() as f64;
            attack += (1.0 - tangent_delta / 128.0).max(0.0) * 0.1;
        }

        // Position
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

        let speed_px = pred.speed_px();
        let speed_term = if speed_px > self.cfg.speed_soft_cap {
            -((speed_px - self.cfg.speed_soft_cap) / self.cfg.speed_soft_cap.max(0.1)) * 0.35
        } else {
            0.0
        };
        let edge_term = -((140.0 - min_edge).max(0.0) / 140.0) * self.cfg.edge_penalty;

        let nearest_threat = nearest_threat_distance(world, pred.x, pred.y);

        // Fire evaluation
        let mut fire_term = 0.0;
        if input.fire && world.bullets.len() < SHIP_BULLET_LIMIT && pred.fire_cooldown <= 0 {
            let fire_quality = estimate_fire_quality(pred, world);
            let min_fire_quality = self.fire_quality_floor(world, pred, aggression);
            let nearest_saucer = nearest_saucer_distance(world, pred);
            let emergency_saucer =
                nearest_saucer < 95.0 && fire_quality + 0.08 >= min_fire_quality;
            let duplicate_target_shot = target_plan
                .map(|plan| {
                    target_already_covered(
                        &world.bullets,
                        plan.target_x, plan.target_y,
                        plan.target_vx, plan.target_vy,
                        plan.target_radius,
                    )
                })
                .unwrap_or(false);
            let (active_ship_bullets, shortest_ship_bullet_life) =
                own_bullet_stats(&world.bullets);
            let discipline_ok = disciplined_fire_ok(
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
                let mut fire_bonus = self.cfg.fire_reward * fire_alignment * (0.35 + 0.65 * fire_quality);
                // Saucer-kill urgency: bonus for targeting saucers (they spawn bullets)
                if let Some(plan) = target_plan {
                    if plan.is_saucer {
                        fire_bonus *= 1.0 + self.cfg.saucer_kill_urgency;
                    }
                }
                fire_term += fire_bonus;
                fire_term -= self.cfg.shot_penalty * 0.72;
            } else if duplicate_target_shot {
                fire_term -= self.cfg.shot_penalty * 0.68;
            } else if !discipline_ok {
                fire_term -= self.cfg.shot_penalty * 0.45;
            } else {
                fire_term -=
                    self.cfg.miss_fire_penalty * (min_fire_quality - fire_quality).max(0.0) * 0.45;
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
            if nearest_threat > 190.0 && speed_px > self.cfg.speed_soft_cap * 0.72 {
                control_term -= self.cfg.thrust_penalty * 0.65;
            }
            if speed_px > self.cfg.speed_soft_cap * 1.05 {
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
}
