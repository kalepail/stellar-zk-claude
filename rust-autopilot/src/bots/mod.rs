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
use asteroids_verifier_core::sim::{
    AsteroidSizeSnapshot, AsteroidSnapshot, BulletSnapshot, SaucerSnapshot, WorldSnapshot,
};
use asteroids_verifier_core::tape::{decode_input_byte, encode_input_byte, parse_tape, FrameInput};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

pub trait AutopilotBot {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn reset(&mut self, seed: u32);
    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput;
    fn next_raw_input(&mut self, _world: &WorldSnapshot) -> Option<u8> {
        None
    }
    fn prefers_raw_inputs(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BotManifestEntry {
    pub id: String,
    pub family: String,
    pub description: String,
    pub config_hash: String,
    pub config: serde_json::Value,
}

#[derive(Clone, Copy, Debug)]
struct MovingTarget {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    radius: i32,
}

#[derive(Clone, Copy, Debug)]
struct TargetingPlan {
    distance_px: f64,
    aim_angle: i32,
    value: f64,
    track: MovingTarget,
    intercept_frames: f64,
}

#[derive(Clone, Copy, Serialize)]
struct SearchConfig {
    id: &'static str,
    description: &'static str,
    lookahead_frames: f64,
    risk_weight_asteroid: f64,
    risk_weight_saucer: f64,
    risk_weight_bullet: f64,
    survival_weight: f64,
    aggression_weight: f64,
    fire_reward: f64,
    shot_penalty: f64,
    miss_fire_penalty: f64,
    action_penalty: f64,
    turn_penalty: f64,
    thrust_penalty: f64,
    center_weight: f64,
    edge_penalty: f64,
    speed_soft_cap: f64,
    fire_tolerance_bam: i32,
    fire_distance_px: f64,
    lurk_trigger_frames: i32,
    lurk_aggression_boost: f64,
}

#[derive(Clone, Copy)]
struct PredictedShip {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    angle: i32,
    radius: i32,
    fire_cooldown: i32,
}

struct SearchBot {
    cfg: SearchConfig,
    rng: SeededRng,
    orbit_sign: i32,
}

#[derive(Clone, Copy, Serialize)]
struct PrecisionConfig {
    id: &'static str,
    description: &'static str,
    depth: usize,
    beam_width: usize,
    discount: f64,
    risk_weight_asteroid: f64,
    risk_weight_saucer: f64,
    risk_weight_bullet: f64,
    survival_weight: f64,
    imminent_penalty: f64,
    shot_reward: f64,
    shot_penalty: f64,
    miss_fire_penalty: f64,
    action_penalty: f64,
    turn_penalty: f64,
    thrust_penalty: f64,
    center_weight: f64,
    edge_penalty: f64,
    speed_soft_cap: f64,
    target_weight_large: f64,
    target_weight_medium: f64,
    target_weight_small: f64,
    target_weight_saucer_large: f64,
    target_weight_saucer_small: f64,
    lurk_trigger_frames: i32,
    lurk_shot_boost: f64,
}

#[derive(Clone)]
struct PlannerState {
    ship: PredictedShip,
    score: u32,
    time_since_last_kill: i32,
    asteroids: Vec<AsteroidSnapshot>,
    saucers: Vec<SaucerSnapshot>,
    saucer_bullets: Vec<BulletSnapshot>,
    bullets: Vec<BulletSnapshot>,
}

#[derive(Clone)]
struct PlanNode {
    state: PlannerState,
    value: f64,
    first_action: u8,
    fire_count: u32,
}

struct PrecisionBot {
    cfg: PrecisionConfig,
}

#[derive(Clone, Copy, Serialize)]
struct OfflineConfig {
    id: &'static str,
    description: &'static str,
    planner: PrecisionConfig,
    depth: u8,
    max_actions_per_node: usize,
    upper_step_bound: f64,
    bound_slack: f64,
    guardian_mode: bool,
    action_change_penalty: f64,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct CacheKey {
    depth: u8,
    ship_cell_x: i16,
    ship_cell_y: i16,
    ship_vx: i16,
    ship_vy: i16,
    ship_angle: u8,
    ship_fire_cooldown: i8,
    asteroid_count: u8,
    saucer_count: u8,
    bullet_count: u8,
    nearest_threat_bin: u16,
    time_since_last_kill_bin: u16,
}

struct OfflineControlBot {
    cfg: OfflineConfig,
    engine: PrecisionBot,
    cache: HashMap<CacheKey, f64>,
    guardian: Option<SearchBot>,
    last_action: u8,
}

#[derive(Clone, Copy, Serialize)]
struct ReplayConfig {
    id: &'static str,
    description: &'static str,
    expected_seed: u32,
    tape_path: &'static str,
    max_frames_hint: u32,
}

struct ReplayBot {
    cfg: ReplayConfig,
    replay_inputs: Vec<u8>,
    replay_ready: bool,
    cursor: usize,
}

impl SearchBot {
    fn new(cfg: SearchConfig) -> Self {
        Self {
            cfg,
            rng: SeededRng::new(0x5E4C_A100),
            orbit_sign: 1,
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

        let lurk_boost = if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            self.cfg.lurk_aggression_boost
        } else {
            1.0
        };

        let mut best: Option<TargetingPlan> = None;
        let mut consider = |x: i32, y: i32, vx: i32, vy: i32, radius: i32, base_weight: f64| {
            if base_weight <= 0.0 {
                return;
            }

            let Some(intercept) = best_wrapped_aim(
                pred.x,
                pred.y,
                pred.vx,
                pred.vy,
                pred.angle,
                x,
                y,
                vx,
                vy,
                bullet_speed,
                64.0,
            ) else {
                return;
            };
            let distance_px = intercept.distance_px;
            let aim_angle = intercept.aim_angle;
            let angle_error = signed_angle_delta(pred.angle, aim_angle).abs() as f64;

            let mut value = (base_weight * lurk_boost) / (distance_px + 16.0);
            value += (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
            value *= 1.0 + (1.0 - (intercept.intercept_frames / 64.0).clamp(0.0, 1.0)) * 0.1;

            let candidate = TargetingPlan {
                distance_px,
                aim_angle,
                value,
                track: MovingTarget {
                    x,
                    y,
                    vx,
                    vy,
                    radius,
                },
                intercept_frames: intercept.intercept_frames,
            };
            match best {
                None => best = Some(candidate),
                Some(existing) if candidate.value > existing.value => best = Some(candidate),
                _ => {}
            }
        };

        for asteroid in &world.asteroids {
            let w = match asteroid.size {
                AsteroidSizeSnapshot::Large => 0.96,
                AsteroidSizeSnapshot::Medium => 1.22,
                AsteroidSizeSnapshot::Small => 1.44,
            };
            consider(
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                w,
            );
        }

        for saucer in &world.saucers {
            let mut w = if saucer.small { 2.6 } else { 1.75 };
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 22.0,
            );
            if approach.closest_px < 200.0 {
                let urgency = ((200.0 - approach.closest_px) / 200.0).clamp(0.0, 1.0);
                w *= 1.0 + urgency * 0.72;
            }
            consider(saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius, w);
        }

        best
    }

    fn target_weight_asteroid(size: AsteroidSizeSnapshot) -> f64 {
        match size {
            AsteroidSizeSnapshot::Large => 0.96,
            AsteroidSizeSnapshot::Medium => 1.22,
            AsteroidSizeSnapshot::Small => 1.44,
        }
    }

    fn nearest_saucer_distance_px(&self, world: &WorldSnapshot, pred: PredictedShip) -> f64 {
        let mut nearest = f64::MAX;
        for saucer in &world.saucers {
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

    fn has_uncovered_target(&self, world: &WorldSnapshot, _pred: PredictedShip) -> bool {
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
        if world.saucers.iter().any(|entry| entry.small) {
            floor -= 0.03;
        }

        floor.clamp(0.05, 0.38)
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
        let mut consider = |x: i32, y: i32, vx: i32, vy: i32, radius: i32, weight: f64| {
            if weight <= 0.0 {
                return;
            }
            let (closest, t) = projectile_wrap_closest_approach(
                start_x, start_y, bullet_vx, bullet_vy, x, y, vx, vy, max_t,
            );
            let safe = (radius + 2) as f64;
            let hit_score = (safe / (closest + 1.0)).powf(1.75);
            let time_factor = 1.0 - (t / max_t) * 0.42;
            let candidate = weight * hit_score * time_factor;
            if candidate > best {
                best = candidate;
            }
        };

        for asteroid in &world.asteroids {
            consider(
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                Self::target_weight_asteroid(asteroid.size),
            );
        }

        for saucer in &world.saucers {
            let mut weight = if saucer.small { 2.6 } else { 1.75 };
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 24.0,
            );
            if approach.closest_px < 220.0 {
                let urgency = ((220.0 - approach.closest_px) / 220.0).clamp(0.0, 1.0);
                weight *= 1.0 + urgency * 0.72;
            }
            if saucer.small {
                weight *= 1.12;
            }
            consider(
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                weight,
            );
        }

        best
    }

    fn action_utility(&self, world: &WorldSnapshot, input_byte: u8) -> f64 {
        let pred = self.predict_ship(world, input_byte);
        let input = decode_input_byte(input_byte);

        let mut risk = 0.0;
        for asteroid in &world.asteroids {
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
            let saucer_weight = if saucer.small {
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
                saucer_weight,
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
                self.cfg.risk_weight_bullet,
            );
        }

        let mut aggression = self.cfg.aggression_weight;
        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            aggression *= self.cfg.lurk_aggression_boost;
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

        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        let target_plan = self.target_score(world, pred);
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

        let speed_px = ((pred.vx as f64 / 256.0).powi(2) + (pred.vy as f64 / 256.0).powi(2)).sqrt();
        let speed_term = if speed_px > self.cfg.speed_soft_cap {
            -((speed_px - self.cfg.speed_soft_cap) / self.cfg.speed_soft_cap.max(0.1)) * 0.35
        } else {
            0.0
        };

        let nearest_threat = self.nearest_threat_distance_px(world, pred);

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

impl AutopilotBot for SearchBot {
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
    }

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return no_input();
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
                    .unwrap_or(!self.has_uncovered_target(world, pred_now));
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
}

impl PrecisionBot {
    fn new(cfg: PrecisionConfig) -> Self {
        Self { cfg }
    }

    fn seed_state(&self, world: &WorldSnapshot) -> PlannerState {
        PlannerState {
            ship: PredictedShip {
                x: world.ship.x,
                y: world.ship.y,
                vx: world.ship.vx,
                vy: world.ship.vy,
                angle: world.ship.angle,
                radius: world.ship.radius,
                fire_cooldown: world.ship.fire_cooldown,
            },
            score: world.score,
            time_since_last_kill: world.time_since_last_kill,
            asteroids: world.asteroids.clone(),
            saucers: world.saucers.clone(),
            saucer_bullets: world.saucer_bullets.clone(),
            bullets: world.bullets.clone(),
        }
    }

    fn candidate_actions(&self) -> &'static [u8] {
        const ACTIONS_BASE: [u8; 10] = [0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C];
        const ACTIONS_CONSERVATIVE: [u8; 7] = [0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08];
        const ACTIONS_ECONOMY: [u8; 6] = [0x00, 0x04, 0x01, 0x02, 0x05, 0x08];
        const ACTIONS_AGGRESSIVE: [u8; 12] = [
            0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x0E,
        ];

        if self.cfg.action_penalty >= 0.024 || self.cfg.turn_penalty >= 0.034 {
            &ACTIONS_ECONOMY
        } else if self.cfg.shot_penalty > 0.9 {
            &ACTIONS_CONSERVATIVE
        } else if self.cfg.shot_reward > 1.5 {
            &ACTIONS_AGGRESSIVE
        } else {
            &ACTIONS_BASE
        }
    }

    fn entity_risk(
        &self,
        ship: PredictedShip,
        ex: i32,
        ey: i32,
        evx: i32,
        evy: i32,
        radius: i32,
        weight: f64,
    ) -> (f64, f64) {
        let approach = torus_relative_approach(
            ship.x,
            ship.y,
            ship.vx,
            ship.vy,
            ex,
            ey,
            evx,
            evy,
            (self.cfg.depth as f64) + 5.0,
        );
        let closest = approach.closest_px;
        let immediate = approach.immediate_px;
        let dot = approach.dot;
        let safe = (ship.radius + radius + 8) as f64;

        let closeness = (safe / (closest + 1.0)).powf(2.1);
        let immediate_term = (safe / (immediate + 1.0)).powf(1.35);
        let closing_boost = if dot < 0.0 { 1.3 } else { 0.92 };
        let risk = weight * (0.72 * closeness + 0.28 * immediate_term) * closing_boost;

        (risk, closest - safe)
    }

    fn target_weight_asteroid(&self, size: AsteroidSizeSnapshot) -> f64 {
        match size {
            AsteroidSizeSnapshot::Large => self.cfg.target_weight_large,
            AsteroidSizeSnapshot::Medium => self.cfg.target_weight_medium,
            AsteroidSizeSnapshot::Small => self.cfg.target_weight_small,
        }
    }

    fn score_for_asteroid(size: AsteroidSizeSnapshot) -> u32 {
        match size {
            AsteroidSizeSnapshot::Large => SCORE_LARGE_ASTEROID,
            AsteroidSizeSnapshot::Medium => SCORE_MEDIUM_ASTEROID,
            AsteroidSizeSnapshot::Small => SCORE_SMALL_ASTEROID,
        }
    }

    fn best_target_info(&self, state: &PlannerState) -> Option<TargetingPlan> {
        let ship = state.ship;
        let mut best: Option<TargetingPlan> = None;
        let ship_speed =
            ((ship.vx as f64 / 256.0).powi(2) + (ship.vy as f64 / 256.0).powi(2)).sqrt();
        let bullet_speed = 8.6 + ship_speed * 0.33;

        let mut consider = |x: i32, y: i32, vx: i32, vy: i32, radius: i32, weight: f64| {
            if weight <= 0.0 {
                return;
            }

            let Some(intercept) = best_wrapped_aim(
                ship.x,
                ship.y,
                ship.vx,
                ship.vy,
                ship.angle,
                x,
                y,
                vx,
                vy,
                bullet_speed,
                72.0,
            ) else {
                return;
            };

            let candidate_score = weight / (intercept.distance_px + 24.0);
            let candidate_score =
                candidate_score * (1.0 + (1.0 - (intercept.intercept_frames / 72.0)) * 0.14);
            let candidate = TargetingPlan {
                distance_px: intercept.distance_px,
                aim_angle: intercept.aim_angle,
                value: candidate_score,
                track: MovingTarget {
                    x,
                    y,
                    vx,
                    vy,
                    radius,
                },
                intercept_frames: intercept.intercept_frames,
            };
            match best {
                None => best = Some(candidate),
                Some(existing) if candidate_score > existing.value => best = Some(candidate),
                _ => {}
            }
        };

        for asteroid in &state.asteroids {
            if !asteroid.alive {
                continue;
            }
            consider(
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                self.target_weight_asteroid(asteroid.size),
            );
        }

        for saucer in &state.saucers {
            if !saucer.alive {
                continue;
            }
            let mut weight = if saucer.small {
                self.cfg.target_weight_saucer_small
            } else {
                self.cfg.target_weight_saucer_large
            };
            let approach = torus_relative_approach(
                ship.x, ship.y, ship.vx, ship.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 26.0,
            );
            if approach.closest_px < 210.0 {
                let urgency = ((210.0 - approach.closest_px) / 210.0).clamp(0.0, 1.0);
                weight *= 1.0 + urgency * 0.78;
            }
            if saucer.small {
                weight *= 1.1;
            }
            consider(
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                weight,
            );
        }

        best
    }

    fn nearest_saucer_distance_px(&self, state: &PlannerState) -> f64 {
        let ship = state.ship;
        let mut nearest = f64::MAX;

        for saucer in &state.saucers {
            if !saucer.alive {
                continue;
            }
            let approach = torus_relative_approach(
                ship.x, ship.y, ship.vx, ship.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 16.0,
            );
            nearest = nearest.min(approach.immediate_px);
        }

        nearest
    }

    fn nearest_threat_distance_px(&self, state: &PlannerState) -> f64 {
        let ship = state.ship;
        let mut nearest = f64::MAX;
        let mut consider = |x: i32, y: i32| {
            let dx = shortest_delta_q12_4(ship.x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(ship.y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            nearest = nearest.min((dx * dx + dy * dy).sqrt());
        };

        for asteroid in &state.asteroids {
            if asteroid.alive {
                consider(asteroid.x, asteroid.y);
            }
        }
        for saucer in &state.saucers {
            if saucer.alive {
                consider(saucer.x, saucer.y);
            }
        }
        for bullet in &state.saucer_bullets {
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

    fn allow_fire_actions_for_state(&self, state: &PlannerState) -> bool {
        let (active_ship_bullets, shortest_ship_bullet_life) =
            own_bullet_in_flight_stats(&state.bullets);
        if active_ship_bullets == 0 {
            return true;
        }
        let nearest_threat = self.nearest_threat_distance_px(state);
        if shortest_ship_bullet_life <= 2 || nearest_threat < 92.0 {
            return true;
        }

        if let Some(plan) = self.best_target_info(state) {
            return !target_already_covered_by_ship_bullets(plan.track, &state.bullets);
        }

        self.has_uncovered_target_for_state(state)
    }

    fn has_uncovered_target_for_state(&self, state: &PlannerState) -> bool {
        for asteroid in &state.asteroids {
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
                &state.bullets,
            ) {
                return true;
            }
        }
        for saucer in &state.saucers {
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
                &state.bullets,
            ) {
                return true;
            }
        }
        false
    }

    fn fire_quality_floor(&self, state: &PlannerState) -> f64 {
        let mut floor = 0.22 + self.cfg.shot_penalty * 0.11 + self.cfg.miss_fire_penalty * 0.09
            - self.cfg.shot_reward * 0.05;

        if state.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            floor -= 0.03;
        }

        let nearest_saucer = self.nearest_saucer_distance_px(state);
        if nearest_saucer < 260.0 {
            floor -= 0.04;
        }
        if nearest_saucer < 170.0 {
            floor -= 0.06;
        }
        if nearest_saucer < 105.0 {
            floor -= 0.08;
        }
        if state.saucers.iter().any(|entry| entry.alive && entry.small) {
            floor -= 0.03;
        }

        floor.clamp(0.08, 0.55)
    }

    fn estimate_fire_quality(&self, ship: PredictedShip, state: &PlannerState) -> f64 {
        let (dx, dy) = displace_q12_4(ship.angle, ship.radius + 6);
        let start_x = wrap_x_q12_4(ship.x + dx);
        let start_y = wrap_y_q12_4(ship.y + dy);

        let ship_speed_approx = ((ship.vx.abs() + ship.vy.abs()) * 3) >> 2;
        let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
        let (bvx, bvy) = velocity_q8_8(ship.angle, bullet_speed_q8_8);
        let bullet_vx = ship.vx + bvx;
        let bullet_vy = ship.vy + bvy;

        let mut best = 0.0;
        let max_t = SHIP_BULLET_LIFETIME_FRAMES as f64;
        let mut consider = |x: i32, y: i32, vx: i32, vy: i32, radius: i32, weight: f64| {
            if weight <= 0.0 {
                return;
            }

            let (closest, t) = projectile_wrap_closest_approach(
                start_x, start_y, bullet_vx, bullet_vy, x, y, vx, vy, max_t,
            );
            let safe = (radius + 2) as f64;
            let hit_score = (safe / (closest + 1.0)).powf(1.8);
            let time_factor = 1.0 - (t / max_t) * 0.45;
            let candidate = weight * hit_score * time_factor;
            if candidate > best {
                best = candidate;
            }
        };

        for asteroid in &state.asteroids {
            if !asteroid.alive {
                continue;
            }
            consider(
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                self.target_weight_asteroid(asteroid.size),
            );
        }

        for saucer in &state.saucers {
            if !saucer.alive {
                continue;
            }
            let mut weight = if saucer.small {
                self.cfg.target_weight_saucer_small
            } else {
                self.cfg.target_weight_saucer_large
            };
            let approach = torus_relative_approach(
                ship.x, ship.y, ship.vx, ship.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 28.0,
            );
            if approach.closest_px < 220.0 {
                let urgency = ((220.0 - approach.closest_px) / 220.0).clamp(0.0, 1.0);
                weight *= 1.0 + urgency * 0.72;
            }
            if saucer.small {
                weight *= 1.15;
            }
            consider(
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                weight,
            );
        }

        best
    }

    fn update_projectiles(projectiles: &mut [BulletSnapshot]) {
        for bullet in projectiles {
            if !bullet.alive {
                continue;
            }

            bullet.life -= 1;
            if bullet.life <= 0 {
                bullet.alive = false;
                continue;
            }

            bullet.x = wrap_x_q12_4(bullet.x + (bullet.vx >> 4));
            bullet.y = wrap_y_q12_4(bullet.y + (bullet.vy >> 4));
        }
    }

    fn resolve_hits(&self, state: &mut PlannerState) -> u32 {
        let mut gained = 0u32;

        for bullet in &mut state.bullets {
            if !bullet.alive {
                continue;
            }

            let mut consumed = false;
            for asteroid in &mut state.asteroids {
                if !asteroid.alive {
                    continue;
                }
                let adjusted_radius = (asteroid.radius * 225) >> 8;
                let hit_dist_q12_4 = ((bullet.radius + adjusted_radius) << 4) as i64;
                let dx = shortest_delta_q12_4(bullet.x, asteroid.x, WORLD_WIDTH_Q12_4) as i64;
                let dy = shortest_delta_q12_4(bullet.y, asteroid.y, WORLD_HEIGHT_Q12_4) as i64;
                if dx * dx + dy * dy <= hit_dist_q12_4 * hit_dist_q12_4 {
                    asteroid.alive = false;
                    bullet.alive = false;
                    gained = gained.saturating_add(Self::score_for_asteroid(asteroid.size));
                    consumed = true;
                    break;
                }
            }
            if consumed {
                continue;
            }

            for saucer in &mut state.saucers {
                if !saucer.alive {
                    continue;
                }
                let hit_dist_q12_4 = ((bullet.radius + saucer.radius) << 4) as i64;
                let dx = shortest_delta_q12_4(bullet.x, saucer.x, WORLD_WIDTH_Q12_4) as i64;
                let dy = shortest_delta_q12_4(bullet.y, saucer.y, WORLD_HEIGHT_Q12_4) as i64;
                if dx * dx + dy * dy <= hit_dist_q12_4 * hit_dist_q12_4 {
                    saucer.alive = false;
                    bullet.alive = false;
                    gained = gained.saturating_add(if saucer.small {
                        SCORE_SMALL_SAUCER
                    } else {
                        SCORE_LARGE_SAUCER
                    });
                    break;
                }
            }
        }

        state.bullets.retain(|entry| entry.alive);
        state.asteroids.retain(|entry| entry.alive);
        state.saucers.retain(|entry| entry.alive);
        state.saucer_bullets.retain(|entry| entry.alive);

        gained
    }

    fn evaluate_state(
        &self,
        state: &PlannerState,
        action: u8,
        fired: bool,
        fire_quality: f64,
    ) -> f64 {
        let ship = state.ship;
        let mut risk = 0.0;
        let mut imminent = 0.0;

        for asteroid in &state.asteroids {
            let (r, gap) = self.entity_risk(
                ship,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
                asteroid.radius,
                self.cfg.risk_weight_asteroid,
            );
            risk += r;
            if gap < 0.0 {
                imminent += (-gap + 1.0) / 24.0;
            }
        }

        for saucer in &state.saucers {
            let weight = if saucer.small {
                self.cfg.risk_weight_saucer * 1.3
            } else {
                self.cfg.risk_weight_saucer
            };
            let (r, gap) = self.entity_risk(
                ship,
                saucer.x,
                saucer.y,
                saucer.vx,
                saucer.vy,
                saucer.radius,
                weight,
            );
            risk += r;
            if gap < 0.0 {
                imminent += (-gap + 1.0) / 22.0;
            }
        }

        for bullet in &state.saucer_bullets {
            let (r, gap) = self.entity_risk(
                ship,
                bullet.x,
                bullet.y,
                bullet.vx,
                bullet.vy,
                bullet.radius,
                self.cfg.risk_weight_bullet,
            );
            risk += r;
            if gap < 0.0 {
                imminent += (-gap + 1.0) / 10.0;
            }
        }

        let mut value = -risk * self.cfg.survival_weight - imminent * self.cfg.imminent_penalty;
        let input = decode_input_byte(action);
        let nearest_threat = self.nearest_threat_distance_px(state);

        if action != 0x00 {
            value -= self.cfg.action_penalty;
        }
        if input.left || input.right {
            value -= self.cfg.turn_penalty;
            if nearest_threat > 180.0 {
                value -= self.cfg.turn_penalty * 0.45;
            }
        }
        if input.thrust {
            value -= self.cfg.thrust_penalty;
        }

        let cx =
            shortest_delta_q12_4(ship.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(ship.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt();
        value -= (center_dist / 920.0) * self.cfg.center_weight;

        let left_edge = ship.x as f64 / 16.0;
        let right_edge = (WORLD_WIDTH_Q12_4 - ship.x) as f64 / 16.0;
        let top_edge = ship.y as f64 / 16.0;
        let bottom_edge = (WORLD_HEIGHT_Q12_4 - ship.y) as f64 / 16.0;
        let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
        value -= ((145.0 - min_edge).max(0.0) / 145.0) * self.cfg.edge_penalty;

        let speed_px = ((ship.vx as f64 / 256.0).powi(2) + (ship.vy as f64 / 256.0).powi(2)).sqrt();
        if speed_px > self.cfg.speed_soft_cap {
            value -=
                ((speed_px - self.cfg.speed_soft_cap) / self.cfg.speed_soft_cap.max(0.1)) * 0.45;
        }
        if input.thrust {
            if nearest_threat > 190.0 && speed_px > self.cfg.speed_soft_cap * 0.72 {
                value -= self.cfg.thrust_penalty * 0.65;
            }
            if speed_px > self.cfg.speed_soft_cap * 1.05 {
                value -= self.cfg.thrust_penalty * 1.1;
            }
        } else if action == 0x00 && nearest_threat > 165.0 {
            value += self.cfg.action_penalty * 0.18;
        }

        if let Some(plan) = self.best_target_info(state) {
            let angle_error = signed_angle_delta(ship.angle, plan.aim_angle).abs() as f64;
            let align = (1.0 - angle_error / 128.0).clamp(0.0, 1.0);
            value += align * plan.value * 9.0;
            if plan.distance_px < 210.0 {
                value += 0.08 * plan.value;
            }
            if plan.intercept_frames <= 16.0 {
                value += 0.04 * plan.value;
            }
        }

        if fired {
            let lurk_bonus = if state.time_since_last_kill >= self.cfg.lurk_trigger_frames {
                self.cfg.lurk_shot_boost
            } else {
                1.0
            };
            value += self.cfg.shot_reward * fire_quality * lurk_bonus;
            value -= self.cfg.shot_penalty;
            if fire_quality < 0.32 {
                value -= self.cfg.miss_fire_penalty * (0.32 - fire_quality);
            }
        } else if state.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            value -= 0.03;
        }

        value
    }

    fn step_state(&self, state: &PlannerState, action: u8) -> (PlannerState, f64, bool) {
        let mut next = state.clone();
        let input = decode_input_byte(action);

        if next.ship.fire_cooldown > 0 {
            next.ship.fire_cooldown -= 1;
        }

        if input.left {
            next.ship.angle = (next.ship.angle - SHIP_TURN_SPEED_BAM) & 0xff;
        }
        if input.right {
            next.ship.angle = (next.ship.angle + SHIP_TURN_SPEED_BAM) & 0xff;
        }

        if input.thrust {
            let accel_vx = (cos_bam(next.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            let accel_vy = (sin_bam(next.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            next.ship.vx += accel_vx;
            next.ship.vy += accel_vy;
        }

        next.ship.vx = apply_drag(next.ship.vx);
        next.ship.vy = apply_drag(next.ship.vy);
        (next.ship.vx, next.ship.vy) =
            clamp_speed_q8_8(next.ship.vx, next.ship.vy, SHIP_MAX_SPEED_SQ_Q16_16);

        let mut fired = false;
        let mut fire_quality = 0.0;
        let mut blocked_duplicate_fire = false;
        if input.fire && next.ship.fire_cooldown <= 0 && next.bullets.len() < SHIP_BULLET_LIMIT {
            fire_quality = self.estimate_fire_quality(next.ship, &next);
            let min_quality = self.fire_quality_floor(&next);
            let nearest_saucer = self.nearest_saucer_distance_px(&next);
            let nearest_threat = self.nearest_threat_distance_px(&next);
            let emergency_saucer_fire = nearest_saucer < 96.0 && fire_quality + 0.08 >= min_quality;
            let duplicate_target_shot = self
                .best_target_info(&next)
                .map(|plan| target_already_covered_by_ship_bullets(plan.track, &next.bullets))
                .unwrap_or(false);
            let (active_ship_bullets, shortest_ship_bullet_life) =
                own_bullet_in_flight_stats(&next.bullets);
            let discipline_ok = disciplined_fire_gate(
                active_ship_bullets,
                shortest_ship_bullet_life,
                fire_quality,
                min_quality,
                nearest_saucer,
                nearest_threat,
                duplicate_target_shot,
            );

            if !duplicate_target_shot
                && discipline_ok
                && (fire_quality >= min_quality || emergency_saucer_fire)
            {
                let (dx, dy) = displace_q12_4(next.ship.angle, next.ship.radius + 6);
                let start_x = wrap_x_q12_4(next.ship.x + dx);
                let start_y = wrap_y_q12_4(next.ship.y + dy);
                let ship_speed_approx = ((next.ship.vx.abs() + next.ship.vy.abs()) * 3) >> 2;
                let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
                let (bvx, bvy) = velocity_q8_8(next.ship.angle, bullet_speed_q8_8);

                next.bullets.push(BulletSnapshot {
                    x: start_x,
                    y: start_y,
                    vx: next.ship.vx + bvx,
                    vy: next.ship.vy + bvy,
                    alive: true,
                    radius: 2,
                    life: SHIP_BULLET_LIFETIME_FRAMES,
                });
                next.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
                fired = true;
            } else if duplicate_target_shot {
                blocked_duplicate_fire = true;
            }
        }

        next.ship.x = wrap_x_q12_4(next.ship.x + (next.ship.vx >> 4));
        next.ship.y = wrap_y_q12_4(next.ship.y + (next.ship.vy >> 4));

        for asteroid in &mut next.asteroids {
            if !asteroid.alive {
                continue;
            }
            asteroid.x = wrap_x_q12_4(asteroid.x + (asteroid.vx >> 4));
            asteroid.y = wrap_y_q12_4(asteroid.y + (asteroid.vy >> 4));
            asteroid.angle = (asteroid.angle + asteroid.spin) & 0xff;
        }

        for saucer in &mut next.saucers {
            if !saucer.alive {
                continue;
            }
            saucer.x = wrap_x_q12_4(saucer.x + (saucer.vx >> 4));
            saucer.y = wrap_y_q12_4(saucer.y + (saucer.vy >> 4));
        }

        Self::update_projectiles(&mut next.bullets);
        Self::update_projectiles(&mut next.saucer_bullets);

        let gained = self.resolve_hits(&mut next);
        if gained > 0 {
            next.time_since_last_kill = 0;
        } else {
            next.time_since_last_kill = next.time_since_last_kill.saturating_add(1);
        }
        next.score = next.score.saturating_add(gained);

        let mut value = self.evaluate_state(&next, action, fired, fire_quality);
        if blocked_duplicate_fire {
            value -= self.cfg.shot_penalty * 0.65;
        }
        value += (gained as f64) * 0.05;
        (next, value, fired)
    }

    fn select_action(&self, world: &WorldSnapshot) -> u8 {
        if world.is_game_over || !world.ship.can_control {
            return 0x00;
        }

        let root = self.seed_state(world);
        let actions = self.candidate_actions();
        let mut beam = vec![PlanNode {
            state: root.clone(),
            value: 0.0,
            first_action: 0x00,
            fire_count: 0,
        }];

        for depth in 0..self.cfg.depth {
            let mut expanded = Vec::with_capacity(beam.len() * actions.len());
            for node in &beam {
                let allow_fire_actions = self.allow_fire_actions_for_state(&node.state);
                for action in actions {
                    if !allow_fire_actions && (*action & 0x08) != 0 {
                        continue;
                    }
                    let (next_state, step_value, fired) = self.step_state(&node.state, *action);
                    expanded.push(PlanNode {
                        state: next_state,
                        value: node.value + step_value * self.cfg.discount.powi(depth as i32),
                        first_action: if depth == 0 {
                            *action
                        } else {
                            node.first_action
                        },
                        fire_count: node.fire_count + u32::from(fired),
                    });
                }
            }

            if expanded.is_empty() {
                break;
            }

            expanded.sort_by(|a, b| {
                b.value
                    .total_cmp(&a.value)
                    .then_with(|| a.fire_count.cmp(&b.fire_count))
                    .then_with(|| a.first_action.cmp(&b.first_action))
            });

            beam.clear();
            let max_per_first = (self.cfg.beam_width / 3).max(2);
            let mut per_first = [0usize; 16];
            for node in expanded {
                let idx = node.first_action as usize;
                if per_first[idx] >= max_per_first && beam.len() + 2 < self.cfg.beam_width {
                    continue;
                }
                per_first[idx] += 1;
                beam.push(node);
                if beam.len() >= self.cfg.beam_width {
                    break;
                }
            }
        }

        let mut best_action = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        let mut best_fire_count = u32::MAX;
        for node in &beam {
            if node.value > best_value
                || (node.value == best_value && node.fire_count < best_fire_count)
            {
                best_value = node.value;
                best_action = node.first_action;
                best_fire_count = node.fire_count;
            }
        }

        if best_action == 0x00 {
            let mut fallback_action = 0x00u8;
            let mut fallback_value = f64::NEG_INFINITY;
            let allow_fire_actions = self.allow_fire_actions_for_state(&root);
            for action in actions {
                if !allow_fire_actions && (*action & 0x08) != 0 {
                    continue;
                }
                let (_, value, fired) = self.step_state(&root, *action);
                let fire_penalty = if fired { 0.02 } else { 0.0 };
                let score = value - fire_penalty;
                if score > fallback_value {
                    fallback_value = score;
                    fallback_action = *action;
                }
            }
            return fallback_action;
        }

        best_action
    }
}

impl AutopilotBot for PrecisionBot {
    fn id(&self) -> &'static str {
        self.cfg.id
    }

    fn description(&self) -> &'static str {
        self.cfg.description
    }

    fn reset(&mut self, _seed: u32) {}

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        decode_input_byte(self.select_action(world))
    }
}

impl OfflineControlBot {
    fn new(cfg: OfflineConfig) -> Self {
        let guardian = if cfg.guardian_mode {
            let preferred = if cfg.planner.shot_reward >= 1.85 {
                "omega-alltime-hunter"
            } else if cfg.planner.shot_reward >= 1.55 {
                "omega-supernova"
            } else {
                "omega-marathon"
            };
            search_bot_configs()
                .iter()
                .find(|entry| entry.id == preferred)
                .or_else(|| {
                    search_bot_configs()
                        .iter()
                        .find(|entry| entry.id == "omega-marathon")
                })
                .copied()
                .map(SearchBot::new)
        } else {
            None
        };

        Self {
            cfg,
            engine: PrecisionBot::new(cfg.planner),
            cache: HashMap::new(),
            guardian,
            last_action: 0x00,
        }
    }

    fn cache_key(&self, state: &PlannerState, depth: u8) -> CacheKey {
        let ship = state.ship;
        let nearest = self.nearest_threat_distance_px(state).min(4095.0);
        let nearest_bin = (nearest / 8.0).round() as u16;

        CacheKey {
            depth,
            ship_cell_x: ((ship.x / 16) / 24) as i16,
            ship_cell_y: ((ship.y / 16) / 24) as i16,
            ship_vx: (ship.vx / 96) as i16,
            ship_vy: (ship.vy / 96) as i16,
            ship_angle: ship.angle as u8,
            ship_fire_cooldown: ship.fire_cooldown.clamp(-1, 127) as i8,
            asteroid_count: state.asteroids.len().min(255) as u8,
            saucer_count: state.saucers.len().min(255) as u8,
            bullet_count: state.saucer_bullets.len().min(255) as u8,
            nearest_threat_bin: nearest_bin,
            time_since_last_kill_bin: (state.time_since_last_kill.clamp(0, 4095) as u16) / 8,
        }
    }

    fn nearest_threat_distance_px(&self, state: &PlannerState) -> f64 {
        let ship = state.ship;
        let mut nearest = f64::MAX;

        let mut consider = |x: i32, y: i32| {
            let dx = shortest_delta_q12_4(ship.x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
            let dy = shortest_delta_q12_4(ship.y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
            nearest = nearest.min((dx * dx + dy * dy).sqrt());
        };

        for asteroid in &state.asteroids {
            if asteroid.alive {
                consider(asteroid.x, asteroid.y);
            }
        }
        for saucer in &state.saucers {
            if saucer.alive {
                consider(saucer.x, saucer.y);
            }
        }
        for bullet in &state.saucer_bullets {
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

    fn terminal_value(&self, state: &PlannerState) -> f64 {
        let nearest = self.nearest_threat_distance_px(state);
        let threat_bonus = (nearest / 140.0).min(1.5);
        let target_bonus = if let Some(plan) = self.engine.best_target_info(state) {
            plan.value * (1.0 - (plan.distance_px / 600.0).clamp(0.0, 1.0))
        } else {
            0.0
        };
        threat_bonus + target_bonus
    }

    fn ordered_actions(&self, state: &PlannerState) -> Vec<(u8, PlannerState, f64)> {
        let mut scored = Vec::with_capacity(12);
        let allow_fire_actions = self.engine.allow_fire_actions_for_state(state);
        for action in self.engine.candidate_actions() {
            if !allow_fire_actions && (*action & 0x08) != 0 {
                continue;
            }
            let (next, immediate, _) = self.engine.step_state(state, *action);
            scored.push((*action, next, immediate));
        }
        scored.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        if scored.len() > self.cfg.max_actions_per_node {
            scored.truncate(self.cfg.max_actions_per_node);
        }
        scored
    }

    fn search_value(&mut self, state: &PlannerState, depth: u8, mut alpha: f64) -> f64 {
        if depth == 0 {
            return self.terminal_value(state);
        }

        let key = self.cache_key(state, depth);
        if let Some(cached) = self.cache.get(&key) {
            return *cached;
        }

        let ordered = self.ordered_actions(state);
        if ordered.is_empty() {
            return self.terminal_value(state);
        }

        let mut best = f64::NEG_INFINITY;
        for (_, next_state, immediate) in ordered {
            let optimistic =
                immediate + self.cfg.upper_step_bound * (depth.saturating_sub(1) as f64);
            if optimistic + self.cfg.bound_slack < alpha {
                continue;
            }

            let child = if depth <= 1 {
                self.terminal_value(&next_state)
            } else {
                self.search_value(&next_state, depth - 1, alpha)
            };

            let total = immediate + self.cfg.planner.discount * child;
            if total > best {
                best = total;
            }
            if total > alpha {
                alpha = total;
            }
        }

        if !best.is_finite() {
            best = self.terminal_value(state);
        }
        self.cache.insert(key, best);
        best
    }

    fn select_action(&mut self, world: &WorldSnapshot) -> u8 {
        if world.is_game_over || !world.ship.can_control {
            return 0x00;
        }

        let root = self.engine.seed_state(world);
        let actions = self.ordered_actions(&root);
        if actions.is_empty() {
            return 0x00;
        }

        self.cache.clear();
        let mut best_action = actions[0].0;
        let mut best_value = f64::NEG_INFINITY;

        for (action, next_state, immediate) in actions {
            let future = if self.cfg.depth <= 1 {
                self.terminal_value(&next_state)
            } else {
                self.search_value(&next_state, self.cfg.depth - 1, best_value)
            };
            let mut total = immediate + self.cfg.planner.discount * future;
            if action != self.last_action {
                total -= self.cfg.action_change_penalty;
            }
            if total > best_value {
                best_value = total;
                best_action = action;
            }
        }

        if let Some(guardian) = self.guardian.as_mut() {
            let guardian_action = encode_input_byte(guardian.next_input(world));
            if guardian_action != best_action {
                let probe_depth = self.cfg.depth.min(3);
                let (guard_next, guard_step, _) = self.engine.step_state(&root, guardian_action);
                let guard_future = if probe_depth <= 1 {
                    self.terminal_value(&guard_next)
                } else {
                    self.search_value(&guard_next, probe_depth - 1, best_value)
                };
                let mut guard_total = guard_step + self.cfg.planner.discount * guard_future;
                if guardian_action != self.last_action {
                    guard_total -= self.cfg.action_change_penalty;
                }
                if guard_total > best_value + 0.2 {
                    best_action = guardian_action;
                }
            }
        }

        self.last_action = best_action;
        best_action
    }
}

impl AutopilotBot for OfflineControlBot {
    fn id(&self) -> &'static str {
        self.cfg.id
    }

    fn description(&self) -> &'static str {
        self.cfg.description
    }

    fn reset(&mut self, seed: u32) {
        self.cache.clear();
        self.last_action = 0x00;
        if let Some(guardian) = self.guardian.as_mut() {
            guardian.reset(seed);
        }
    }

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        decode_input_byte(self.select_action(world))
    }
}

impl ReplayBot {
    fn new(cfg: ReplayConfig) -> Self {
        Self {
            cfg,
            replay_inputs: Vec::new(),
            replay_ready: false,
            cursor: 0,
        }
    }
}

impl AutopilotBot for ReplayBot {
    fn id(&self) -> &'static str {
        self.cfg.id
    }

    fn description(&self) -> &'static str {
        self.cfg.description
    }

    fn reset(&mut self, seed: u32) {
        self.cursor = 0;
        self.replay_ready = false;
        self.replay_inputs.clear();

        if seed != self.cfg.expected_seed {
            return;
        }

        let Ok(bytes) = fs::read(self.cfg.tape_path) else {
            return;
        };
        let Ok(view) = parse_tape(&bytes, self.cfg.max_frames_hint) else {
            return;
        };
        if view.header.seed != self.cfg.expected_seed {
            return;
        }

        self.replay_inputs = view.inputs.to_vec();
        self.replay_ready = true;
    }

    fn next_input(&mut self, _world: &WorldSnapshot) -> FrameInput {
        if !self.replay_ready || self.cursor >= self.replay_inputs.len() {
            return no_input();
        }

        decode_input_byte(self.replay_inputs[self.cursor])
    }

    fn next_raw_input(&mut self, _world: &WorldSnapshot) -> Option<u8> {
        if !self.replay_ready || self.cursor >= self.replay_inputs.len() {
            return None;
        }

        let input = self.replay_inputs[self.cursor];
        self.cursor += 1;
        Some(input)
    }

    fn prefers_raw_inputs(&self) -> bool {
        self.replay_ready
    }
}

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

fn own_bullet_in_flight_stats(bullets: &[BulletSnapshot]) -> (usize, i32) {
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

fn bullet_confidently_tracks_target(bullet: &BulletSnapshot, target: MovingTarget) -> bool {
    if !bullet.alive || bullet.life <= 0 {
        return false;
    }

    // Keep confidence local-in-time so we do not over-assume long-horizon hits.
    let horizon = (bullet.life as f64).min(32.0).max(1.0);
    let (closest, t) = projectile_wrap_closest_approach(
        bullet.x, bullet.y, bullet.vx, bullet.vy, target.x, target.y, target.vx, target.vy, horizon,
    );
    let hit_radius = (bullet.radius + target.radius) as f64;
    closest <= hit_radius * 1.02 && t <= horizon * 0.9
}

fn target_already_covered_by_ship_bullets(
    target: MovingTarget,
    bullets: &[BulletSnapshot],
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

#[inline]
fn search_bot_configs() -> &'static [SearchConfig] {
    roster::search_bot_configs()
}

mod codex;
mod roster;

pub use roster::{bot_ids, create_bot, describe_bots};

pub fn bot_fingerprint(id: &str) -> Option<String> {
    roster::bot_fingerprint(id)
}

pub fn bot_manifest_entries() -> Vec<BotManifestEntry> {
    roster::bot_manifest_entries()
}
