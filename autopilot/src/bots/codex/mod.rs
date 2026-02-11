use super::AutopilotBot;
use asteroids_verifier_core::constants::{
    SCORE_LARGE_SAUCER, SCORE_SMALL_SAUCER, SHIP_BULLET_COOLDOWN_FRAMES,
    SHIP_BULLET_LIFETIME_FRAMES, SHIP_BULLET_LIMIT, SHIP_BULLET_SPEED_Q8_8,
    SHIP_MAX_SPEED_SQ_Q16_16, SHIP_RESPAWN_FRAMES, SHIP_THRUST_Q8_8, SHIP_TURN_SPEED_BAM,
    WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::{
    apply_drag, atan2_bam, clamp_speed_q8_8, cos_bam, displace_q12_4, shortest_delta_q12_4,
    sin_bam, velocity_q8_8, wrap_x_q12_4, wrap_y_q12_4,
};
use asteroids_verifier_core::rng::SeededRng;
use asteroids_verifier_core::sim::{AsteroidSizeSnapshot, BulletSnapshot, WorldSnapshot};
use asteroids_verifier_core::tape::{decode_input_byte, FrameInput};
use serde::{Deserialize, Serialize};
use std::fs;

const TORUS_SHIFTS_X_Q12_4: [i32; 3] = [-WORLD_WIDTH_Q12_4, 0, WORLD_WIDTH_Q12_4];
const TORUS_SHIFTS_Y_Q12_4: [i32; 3] = [-WORLD_HEIGHT_Q12_4, 0, WORLD_HEIGHT_Q12_4];
const ACTIONS_WIDE: [u8; 12] = [
    0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x0E,
];
const ACTIONS_ROLLOUT: [u8; 10] = [0x00, 0x04, 0x01, 0x02, 0x05, 0x06, 0x08, 0x09, 0x0A, 0x0C];
// This challenge uses a fixed 30-minute cap at 60 FPS (108000 frames).
const ENDGAME_FRAME_CAP: i32 = 108_000;
const ENDGAME_PUSH_START_FRAME: i32 = 72_000;
/// Path relative to the autopilot crate root (CARGO_MANIFEST_DIR).
const ADAPTIVE_PROFILE_REL_PATH: &str = "codex-/state/adaptive-profile.json";

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

#[derive(Clone, Copy, Debug)]
struct RolloutState {
    ship: PredictedShip,
    bullets_in_flight: usize,
}

#[derive(Clone, Copy, Debug)]
struct MovingTarget {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    radius: i32,
    value_hint: f64,
}

#[derive(Clone, Copy, Debug)]
struct TargetPlan {
    aim_angle: i32,
    distance_px: f64,
    intercept_frames: f64,
    value: f64,
    target: MovingTarget,
}

#[derive(Clone, Copy, Debug)]
struct TorusApproach {
    immediate_px: f64,
    closest_px: f64,
    t_closest: f64,
    dot: f64,
}

#[derive(Clone, Copy, Debug)]
struct WrapAimSolution {
    distance_px: f64,
    aim_angle: i32,
    intercept_frames: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct PotentialConfig {
    pub id: &'static str,
    pub description: &'static str,
    pub lookahead_frames: f64,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,
    pub aggression_weight: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub min_fire_quality: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub speed_soft_cap: f64,
    pub fire_tolerance_bam: i32,
    pub fire_distance_px: f64,
    pub lurk_trigger_frames: i32,
    pub lurk_boost: f64,
    pub flow_weight: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct ModeWeights {
    pub survival_weight: f64,
    pub aggression_weight: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub min_fire_quality: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub flow_weight: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct StanceConfig {
    pub id: &'static str,
    pub description: &'static str,
    pub lookahead_frames: f64,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub panic_distance_px: f64,
    pub harvest_distance_px: f64,
    pub lurk_trigger_frames: i32,
    pub mode_hysteresis_frames: i32,
    pub fire_tolerance_bam: i32,
    pub fire_distance_px: f64,
    pub speed_soft_cap: f64,
    pub panic: ModeWeights,
    pub harvest: ModeWeights,
    pub lurk_break: ModeWeights,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct RolloutConfig {
    pub id: &'static str,
    pub description: &'static str,
    pub depth: u8,
    pub discount: f64,
    pub lookahead_frames: f64,
    pub risk_weight_asteroid: f64,
    pub risk_weight_saucer: f64,
    pub risk_weight_bullet: f64,
    pub survival_weight: f64,
    pub aggression_weight: f64,
    pub fire_reward: f64,
    pub shot_penalty: f64,
    pub miss_fire_penalty: f64,
    pub min_fire_quality: f64,
    pub action_penalty: f64,
    pub turn_penalty: f64,
    pub thrust_penalty: f64,
    pub center_weight: f64,
    pub edge_penalty: f64,
    pub speed_soft_cap: f64,
    pub fire_tolerance_bam: i32,
    pub fire_distance_px: f64,
    pub lurk_trigger_frames: i32,
    pub lurk_boost: f64,
}

pub(super) struct PotentialBot {
    cfg: PotentialConfig,
    rng: SeededRng,
    orbit_sign: i32,
}

#[derive(Clone, Copy, Debug)]
struct EndgamePressure {
    endgame_push: f64,
    saucer_push: f64,
    target_value_mult: f64,
    min_fire_quality_relief: f64,
    control_relief: f64,
    risk_asteroid_mult: f64,
    risk_saucer_mult: f64,
    risk_bullet_mult: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StanceMode {
    Panic,
    Harvest,
    LurkBreak,
}

pub(super) struct StanceBot {
    cfg: StanceConfig,
    rng: SeededRng,
    orbit_sign: i32,
    mode: StanceMode,
    mode_hold_frames: i32,
}

pub(super) struct RolloutBot {
    cfg: RolloutConfig,
    rng: SeededRng,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AdaptiveProfile {
    #[serde(default = "default_scale")]
    pub risk_weight_scale: f64,
    #[serde(default = "default_scale")]
    pub survival_weight_scale: f64,
    #[serde(default = "default_scale")]
    pub aggression_weight_scale: f64,
    #[serde(default = "default_scale")]
    pub fire_reward_scale: f64,
    #[serde(default = "default_scale")]
    pub shot_penalty_scale: f64,
    #[serde(default = "default_scale")]
    pub miss_fire_penalty_scale: f64,
    #[serde(default)]
    pub min_fire_quality_delta: f64,
    #[serde(default = "default_scale")]
    pub action_penalty_scale: f64,
    #[serde(default = "default_scale")]
    pub turn_penalty_scale: f64,
    #[serde(default = "default_scale")]
    pub thrust_penalty_scale: f64,
    #[serde(default = "default_scale")]
    pub center_weight_scale: f64,
    #[serde(default = "default_scale")]
    pub edge_penalty_scale: f64,
    #[serde(default = "default_scale")]
    pub lookahead_frames_scale: f64,
    #[serde(default = "default_scale")]
    pub flow_weight_scale: f64,
    #[serde(default = "default_scale")]
    pub speed_soft_cap_scale: f64,
    #[serde(default = "default_scale")]
    pub fire_distance_scale: f64,
    #[serde(default = "default_scale")]
    pub lurk_trigger_scale: f64,
    #[serde(default = "default_scale")]
    pub lurk_boost_scale: f64,
    #[serde(default = "default_scale")]
    pub fire_tolerance_scale: f64,
}

#[inline]
fn default_scale() -> f64 {
    1.0
}

pub(super) fn potential_bot_configs() -> &'static [PotentialConfig] {
    &[
        PotentialConfig {
            id: "codex-potential-navigator",
            description: "Potential-field navigator balancing threat repulsion with efficient wrap intercepts.",
            lookahead_frames: 22.0,
            risk_weight_asteroid: 1.48,
            risk_weight_saucer: 2.16,
            risk_weight_bullet: 2.98,
            survival_weight: 2.12,
            aggression_weight: 0.82,
            fire_reward: 1.05,
            shot_penalty: 0.92,
            miss_fire_penalty: 1.25,
            min_fire_quality: 0.2,
            action_penalty: 0.012,
            turn_penalty: 0.013,
            thrust_penalty: 0.012,
            center_weight: 0.5,
            edge_penalty: 0.34,
            speed_soft_cap: 4.05,
            fire_tolerance_bam: 8,
            fire_distance_px: 300.0,
            lurk_trigger_frames: 320,
            lurk_boost: 1.42,
            flow_weight: 0.8,
        },
        PotentialConfig {
            id: "codex-potential-raider",
            description: "Potential-field raider with stronger kill pressure once lanes are clear.",
            lookahead_frames: 19.0,
            risk_weight_asteroid: 1.24,
            risk_weight_saucer: 1.84,
            risk_weight_bullet: 2.58,
            survival_weight: 1.72,
            aggression_weight: 1.12,
            fire_reward: 1.42,
            shot_penalty: 0.76,
            miss_fire_penalty: 0.98,
            min_fire_quality: 0.16,
            action_penalty: 0.0095,
            turn_penalty: 0.0108,
            thrust_penalty: 0.0102,
            center_weight: 0.4,
            edge_penalty: 0.26,
            speed_soft_cap: 4.55,
            fire_tolerance_bam: 9,
            fire_distance_px: 360.0,
            lurk_trigger_frames: 250,
            lurk_boost: 1.96,
            flow_weight: 0.65,
        },
    ]
}

fn adaptive_base_potential_config() -> PotentialConfig {
    PotentialConfig {
        id: "codex-potential-adaptive",
        description:
            "Adaptive potential-field ship tuned from codex death/miss telemetry profiles.",
        lookahead_frames: 21.0,
        risk_weight_asteroid: 1.46,
        risk_weight_saucer: 2.14,
        risk_weight_bullet: 2.96,
        survival_weight: 2.08,
        aggression_weight: 0.88,
        fire_reward: 1.06,
        shot_penalty: 0.9,
        miss_fire_penalty: 1.2,
        min_fire_quality: 0.2,
        action_penalty: 0.0115,
        turn_penalty: 0.0125,
        thrust_penalty: 0.0118,
        center_weight: 0.5,
        edge_penalty: 0.33,
        speed_soft_cap: 4.1,
        fire_tolerance_bam: 8,
        fire_distance_px: 320.0,
        lurk_trigger_frames: 300,
        lurk_boost: 1.56,
        flow_weight: 0.76,
    }
}

pub(super) fn default_adaptive_profile() -> AdaptiveProfile {
    AdaptiveProfile {
        risk_weight_scale: 1.0,
        survival_weight_scale: 1.0,
        aggression_weight_scale: 1.0,
        fire_reward_scale: 1.0,
        shot_penalty_scale: 1.0,
        miss_fire_penalty_scale: 1.0,
        min_fire_quality_delta: 0.0,
        action_penalty_scale: 1.0,
        turn_penalty_scale: 1.0,
        thrust_penalty_scale: 1.0,
        center_weight_scale: 1.0,
        edge_penalty_scale: 1.0,
        lookahead_frames_scale: 1.0,
        flow_weight_scale: 1.0,
        speed_soft_cap_scale: 1.0,
        fire_distance_scale: 1.0,
        lurk_trigger_scale: 1.0,
        lurk_boost_scale: 1.0,
        fire_tolerance_scale: 1.0,
    }
}

pub(super) fn load_adaptive_profile() -> AdaptiveProfile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(ADAPTIVE_PROFILE_REL_PATH);
    let Ok(raw) = fs::read_to_string(path) else {
        return default_adaptive_profile();
    };
    serde_json::from_str::<AdaptiveProfile>(&raw).unwrap_or_else(|_| default_adaptive_profile())
}

fn apply_adaptive_profile(base: PotentialConfig, profile: AdaptiveProfile) -> PotentialConfig {
    PotentialConfig {
        risk_weight_asteroid: base.risk_weight_asteroid * profile.risk_weight_scale.max(0.1),
        risk_weight_saucer: base.risk_weight_saucer * profile.risk_weight_scale.max(0.1),
        risk_weight_bullet: base.risk_weight_bullet * profile.risk_weight_scale.max(0.1),
        survival_weight: base.survival_weight * profile.survival_weight_scale.max(0.1),
        aggression_weight: base.aggression_weight * profile.aggression_weight_scale.max(0.1),
        fire_reward: base.fire_reward * profile.fire_reward_scale.max(0.1),
        shot_penalty: base.shot_penalty * profile.shot_penalty_scale.max(0.1),
        miss_fire_penalty: base.miss_fire_penalty * profile.miss_fire_penalty_scale.max(0.1),
        min_fire_quality: (base.min_fire_quality + profile.min_fire_quality_delta)
            .clamp(0.05, 0.65),
        action_penalty: base.action_penalty * profile.action_penalty_scale.max(0.1),
        turn_penalty: base.turn_penalty * profile.turn_penalty_scale.max(0.1),
        thrust_penalty: base.thrust_penalty * profile.thrust_penalty_scale.max(0.1),
        center_weight: base.center_weight * profile.center_weight_scale.max(0.1),
        edge_penalty: base.edge_penalty * profile.edge_penalty_scale.max(0.1),
        lookahead_frames: (base.lookahead_frames * profile.lookahead_frames_scale.max(0.1))
            .clamp(12.0, 36.0),
        flow_weight: (base.flow_weight * profile.flow_weight_scale.max(0.1)).clamp(0.18, 2.2),
        speed_soft_cap: (base.speed_soft_cap * profile.speed_soft_cap_scale.max(0.1))
            .clamp(2.6, 7.4),
        fire_distance_px: (base.fire_distance_px * profile.fire_distance_scale.max(0.1))
            .clamp(180.0, 560.0),
        lurk_trigger_frames: ((base.lurk_trigger_frames as f64
            * profile.lurk_trigger_scale.max(0.1))
        .round() as i32)
            .clamp(120, 900),
        lurk_boost: (base.lurk_boost * profile.lurk_boost_scale.max(0.1)).clamp(1.0, 3.4),
        fire_tolerance_bam: ((base.fire_tolerance_bam as f64
            * profile.fire_tolerance_scale.max(0.1))
        .round() as i32)
            .clamp(3, 24),
        ..base
    }
}

pub(super) fn stance_bot_configs() -> &'static [StanceConfig] {
    &[
        StanceConfig {
            id: "codex-stance-warden",
            description: "State-machine pilot that hard-switches between panic evasion, harvest, and lurk-break modes.",
            lookahead_frames: 21.0,
            risk_weight_asteroid: 1.52,
            risk_weight_saucer: 2.2,
            risk_weight_bullet: 3.05,
            panic_distance_px: 108.0,
            harvest_distance_px: 220.0,
            lurk_trigger_frames: 320,
            mode_hysteresis_frames: 12,
            fire_tolerance_bam: 8,
            fire_distance_px: 305.0,
            speed_soft_cap: 3.9,
            panic: ModeWeights {
                survival_weight: 2.45,
                aggression_weight: 0.38,
                fire_reward: 0.62,
                shot_penalty: 1.2,
                miss_fire_penalty: 1.42,
                min_fire_quality: 0.24,
                action_penalty: 0.008,
                turn_penalty: 0.009,
                thrust_penalty: 0.008,
                center_weight: 0.62,
                edge_penalty: 0.44,
                flow_weight: 1.12,
            },
            harvest: ModeWeights {
                survival_weight: 1.74,
                aggression_weight: 1.0,
                fire_reward: 1.18,
                shot_penalty: 0.84,
                miss_fire_penalty: 1.06,
                min_fire_quality: 0.18,
                action_penalty: 0.0105,
                turn_penalty: 0.012,
                thrust_penalty: 0.011,
                center_weight: 0.46,
                edge_penalty: 0.3,
                flow_weight: 0.74,
            },
            lurk_break: ModeWeights {
                survival_weight: 1.62,
                aggression_weight: 1.38,
                fire_reward: 1.48,
                shot_penalty: 0.7,
                miss_fire_penalty: 0.88,
                min_fire_quality: 0.13,
                action_penalty: 0.011,
                turn_penalty: 0.0125,
                thrust_penalty: 0.0118,
                center_weight: 0.35,
                edge_penalty: 0.22,
                flow_weight: 0.68,
            },
        },
        StanceConfig {
            id: "codex-stance-breaker",
            description: "Mode-switching scorer with aggressive lurk-break and opportunistic saucer pressure.",
            lookahead_frames: 18.0,
            risk_weight_asteroid: 1.32,
            risk_weight_saucer: 1.9,
            risk_weight_bullet: 2.62,
            panic_distance_px: 98.0,
            harvest_distance_px: 205.0,
            lurk_trigger_frames: 255,
            mode_hysteresis_frames: 10,
            fire_tolerance_bam: 9,
            fire_distance_px: 350.0,
            speed_soft_cap: 4.45,
            panic: ModeWeights {
                survival_weight: 2.1,
                aggression_weight: 0.46,
                fire_reward: 0.7,
                shot_penalty: 1.08,
                miss_fire_penalty: 1.24,
                min_fire_quality: 0.22,
                action_penalty: 0.0078,
                turn_penalty: 0.0088,
                thrust_penalty: 0.008,
                center_weight: 0.52,
                edge_penalty: 0.36,
                flow_weight: 1.0,
            },
            harvest: ModeWeights {
                survival_weight: 1.52,
                aggression_weight: 1.18,
                fire_reward: 1.34,
                shot_penalty: 0.72,
                miss_fire_penalty: 0.92,
                min_fire_quality: 0.16,
                action_penalty: 0.009,
                turn_penalty: 0.0105,
                thrust_penalty: 0.0098,
                center_weight: 0.38,
                edge_penalty: 0.23,
                flow_weight: 0.66,
            },
            lurk_break: ModeWeights {
                survival_weight: 1.38,
                aggression_weight: 1.58,
                fire_reward: 1.64,
                shot_penalty: 0.62,
                miss_fire_penalty: 0.8,
                min_fire_quality: 0.1,
                action_penalty: 0.0098,
                turn_penalty: 0.011,
                thrust_penalty: 0.0105,
                center_weight: 0.28,
                edge_penalty: 0.18,
                flow_weight: 0.54,
            },
        },
    ]
}

pub(super) fn rollout_bot_configs() -> &'static [RolloutConfig] {
    &[
        RolloutConfig {
            id: "codex-rollout-sentinel",
            description: "Two-plus-step rollout planner emphasizing durable positioning and shot selectivity.",
            depth: 3,
            discount: 0.9,
            lookahead_frames: 20.0,
            risk_weight_asteroid: 1.54,
            risk_weight_saucer: 2.24,
            risk_weight_bullet: 3.08,
            survival_weight: 2.24,
            aggression_weight: 0.74,
            fire_reward: 0.94,
            shot_penalty: 1.04,
            miss_fire_penalty: 1.34,
            min_fire_quality: 0.21,
            action_penalty: 0.0115,
            turn_penalty: 0.013,
            thrust_penalty: 0.012,
            center_weight: 0.53,
            edge_penalty: 0.37,
            speed_soft_cap: 3.95,
            fire_tolerance_bam: 8,
            fire_distance_px: 295.0,
            lurk_trigger_frames: 325,
            lurk_boost: 1.38,
        },
        RolloutConfig {
            id: "codex-rollout-overdrive",
            description: "Short-horizon rollout hunter with faster kill conversion and higher risk tolerance.",
            depth: 4,
            discount: 0.88,
            lookahead_frames: 17.0,
            risk_weight_asteroid: 1.28,
            risk_weight_saucer: 1.86,
            risk_weight_bullet: 2.54,
            survival_weight: 1.68,
            aggression_weight: 1.26,
            fire_reward: 1.5,
            shot_penalty: 0.7,
            miss_fire_penalty: 0.92,
            min_fire_quality: 0.15,
            action_penalty: 0.0095,
            turn_penalty: 0.0108,
            thrust_penalty: 0.0102,
            center_weight: 0.36,
            edge_penalty: 0.22,
            speed_soft_cap: 4.7,
            fire_tolerance_bam: 9,
            fire_distance_px: 360.0,
            lurk_trigger_frames: 250,
            lurk_boost: 1.88,
        },
    ]
}

impl PotentialBot {
    pub(super) fn new(cfg: PotentialConfig) -> Self {
        Self {
            cfg,
            rng: SeededRng::new(0xC0D3_5001),
            orbit_sign: 1,
        }
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);
        let pressure = compute_endgame_pressure(world);
        let target = best_target(
            world,
            pred,
            self.cfg.lookahead_frames + 48.0,
            pressure.target_value_mult,
        );

        let mut aggression = self.cfg.aggression_weight;
        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            aggression *= self.cfg.lurk_boost;
        }
        aggression *= 1.0 + 0.28 * pressure.endgame_push + 0.44 * pressure.saucer_push;

        let survival_weight = self.cfg.survival_weight * (1.0 - 0.22 * pressure.endgame_push);
        let fire_reward = self.cfg.fire_reward
            * (1.0 + 0.26 * pressure.endgame_push + 0.24 * pressure.saucer_push);
        let shot_penalty = (self.cfg.shot_penalty
            * (1.0 - 0.2 * pressure.endgame_push - 0.12 * pressure.saucer_push))
            .max(self.cfg.shot_penalty * 0.35);
        let miss_fire_penalty = (self.cfg.miss_fire_penalty
            * (1.0 - 0.16 * pressure.endgame_push - 0.14 * pressure.saucer_push))
            .max(self.cfg.miss_fire_penalty * 0.4);

        let risk = total_risk(
            world,
            pred,
            self.cfg.lookahead_frames,
            self.cfg.risk_weight_asteroid * pressure.risk_asteroid_mult,
            self.cfg.risk_weight_saucer * pressure.risk_saucer_mult,
            self.cfg.risk_weight_bullet * pressure.risk_bullet_mult,
        );

        let flow_align = self
            .flow_angle(world, pred, aggression, target)
            .map(|angle| angle_alignment(pred.angle, angle))
            .unwrap_or(0.0);

        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        if let Some(plan) = target {
            let is_saucer_target = looks_like_saucer_target(plan.target);
            attack += plan.value * aggression;
            fire_alignment = angle_alignment(pred.angle, plan.aim_angle);
            let fire_distance_push = self.cfg.fire_distance_px * (1.0 + 0.2 * pressure.saucer_push);
            if plan.distance_px < fire_distance_push {
                attack += 0.11;
            }
            if plan.intercept_frames < 14.0 {
                attack += 0.05;
            }
            if is_saucer_target {
                attack += 0.1 * pressure.saucer_push;
                if plan.intercept_frames < 18.0 {
                    attack += 0.08 * pressure.saucer_push;
                }
            }
        }

        let center_term = center_term(pred, self.cfg.center_weight);
        let edge_term = edge_term(pred, self.cfg.edge_penalty);
        let speed_term = speed_term(pred, self.cfg.speed_soft_cap);
        let nearest_threat = nearest_threat_distance_px(world, pred);
        let nearest_saucer = nearest_saucer_distance_px(world, pred);

        let mut fire_term = 0.0;
        if input.fire {
            if can_fire(world, pred) {
                let fire_quality = estimate_fire_quality(world, pred);
                let min_quality = (dynamic_min_fire_quality(
                    self.cfg.min_fire_quality,
                    world.time_since_last_kill,
                    self.cfg.lurk_trigger_frames,
                    nearest_saucer,
                ) - pressure.min_fire_quality_relief)
                    .clamp(0.04, 0.6);
                let duplicate = target
                    .map(|plan| target_already_covered_by_ship_bullets(plan.target, &world.bullets))
                    .unwrap_or(false);
                let (active_bullets, shortest_life) = own_bullet_in_flight_stats(&world.bullets);
                let discipline_ok = disciplined_fire_gate(
                    active_bullets,
                    shortest_life,
                    fire_quality,
                    min_quality,
                    nearest_saucer,
                    nearest_threat,
                    duplicate,
                );

                if discipline_ok && !duplicate && fire_quality >= min_quality {
                    fire_term += fire_reward * (0.35 + 0.65 * fire_alignment) * fire_quality;
                    fire_term -= shot_penalty * 0.72;
                } else if duplicate {
                    fire_term -= shot_penalty * 0.7;
                } else {
                    fire_term -= miss_fire_penalty * (min_quality - fire_quality).max(0.0);
                    fire_term -= shot_penalty * 0.32;
                }
            } else {
                fire_term -= shot_penalty * 0.54;
            }
        }

        let mut control_term = 0.0;
        let mut control_scale: f64 = if risk > 3.0 {
            0.22
        } else if risk > 1.8 {
            0.42
        } else {
            1.0
        };
        control_scale = (control_scale * (1.0 - pressure.control_relief)).max(0.08);
        if action != 0x00 {
            control_term -= self.cfg.action_penalty * control_scale;
        }
        if input.left || input.right {
            control_term -= self.cfg.turn_penalty * control_scale;
        }
        if input.thrust {
            control_term -= self.cfg.thrust_penalty * control_scale;
            if pressure.saucer_push > 0.5 && risk < 1.05 {
                control_term += self.cfg.thrust_penalty * 0.08 * pressure.saucer_push;
            }
        } else if action == 0x00 && nearest_threat > 185.0 {
            control_term += self.cfg.action_penalty * 0.16 * (1.0 - pressure.control_relief * 0.75);
        }

        -risk * survival_weight
            + attack
            + fire_term
            + self.cfg.flow_weight * flow_align
            + control_term
            + center_term
            + edge_term
            + speed_term
    }

    fn flow_angle(
        &self,
        world: &WorldSnapshot,
        pred: PredictedShip,
        aggression: f64,
        target: Option<TargetPlan>,
    ) -> Option<i32> {
        let mut fx = 0.0;
        let mut fy = 0.0;

        for asteroid in &world.asteroids {
            add_repulsion(
                pred,
                asteroid.x,
                asteroid.y,
                asteroid_weight(asteroid.size),
                &mut fx,
                &mut fy,
            );
        }

        for saucer in &world.saucers {
            let w = if saucer.small { 2.35 } else { 1.7 };
            add_repulsion(pred, saucer.x, saucer.y, w, &mut fx, &mut fy);
        }

        for bullet in &world.saucer_bullets {
            add_repulsion(pred, bullet.x, bullet.y, 2.7, &mut fx, &mut fy);
        }

        let cx =
            shortest_delta_q12_4(pred.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let cy =
            shortest_delta_q12_4(pred.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let center_dist = (cx * cx + cy * cy).sqrt().max(1.0);

        fx += (cx / center_dist) * 0.5;
        fy += (cy / center_dist) * 0.5;

        // Orbit component keeps the ship from tunneling into danger while still moving.
        let orbit_gain = 0.38;
        fx += (self.orbit_sign as f64) * (-cy / center_dist) * orbit_gain;
        fy += (self.orbit_sign as f64) * (cx / center_dist) * orbit_gain;

        if let Some(plan) = target {
            let (tdx, tdy) = torus_delta(pred.x, pred.y, plan.target.x, plan.target.y);
            let dist = (tdx * tdx + tdy * tdy).sqrt().max(1.0);
            let pull = (plan.value * aggression).clamp(0.2, 2.4);
            fx += (tdx / dist) * pull;
            fy += (tdy / dist) * pull;
        }

        if fx.abs() + fy.abs() <= 1e-6 {
            return None;
        }

        Some(atan2_bam((fy * 64.0) as i32, (fx * 64.0) as i32))
    }
}

impl AutopilotBot for PotentialBot {
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
        self.rng = SeededRng::new(seed ^ hash ^ 0xC0D3_A11A);
        self.orbit_sign = if self.rng.next() & 1 == 0 { 1 } else { -1 };
    }

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return no_input();
        }

        let mut best = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        for action in ACTIONS_WIDE {
            let value = self.evaluate_action(world, action);
            if value > best_value {
                best_value = value;
                best = action;
            }
        }

        decode_input_byte(best)
    }
}

impl StanceBot {
    pub(super) fn new(cfg: StanceConfig) -> Self {
        Self {
            cfg,
            rng: SeededRng::new(0xC0D3_5002),
            orbit_sign: 1,
            mode: StanceMode::Harvest,
            mode_hold_frames: 0,
        }
    }

    fn current_weights(&self) -> ModeWeights {
        match self.mode {
            StanceMode::Panic => self.cfg.panic,
            StanceMode::Harvest => self.cfg.harvest,
            StanceMode::LurkBreak => self.cfg.lurk_break,
        }
    }

    fn desired_mode(&self, world: &WorldSnapshot, pred: PredictedShip) -> StanceMode {
        let nearest = nearest_threat_distance_px(world, pred);
        if nearest <= self.cfg.panic_distance_px {
            return StanceMode::Panic;
        }

        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            return StanceMode::LurkBreak;
        }

        if nearest >= self.cfg.harvest_distance_px {
            return StanceMode::Harvest;
        }

        if world.lives <= 1 && nearest < self.cfg.harvest_distance_px * 0.8 {
            StanceMode::Panic
        } else {
            StanceMode::Harvest
        }
    }

    fn maybe_switch_mode(&mut self, desired: StanceMode) {
        if desired == self.mode {
            self.mode_hold_frames = (self.mode_hold_frames - 1).max(0);
            return;
        }

        if self.mode_hold_frames > 0 {
            self.mode_hold_frames -= 1;
            return;
        }

        self.mode = desired;
        self.mode_hold_frames = self.cfg.mode_hysteresis_frames.max(0);
    }

    fn evaluate_action(&self, world: &WorldSnapshot, action: u8) -> f64 {
        let pred = predict_ship(world, action);
        let input = decode_input_byte(action);
        let weights = self.current_weights();

        let risk = total_risk(
            world,
            pred,
            self.cfg.lookahead_frames,
            self.cfg.risk_weight_asteroid,
            self.cfg.risk_weight_saucer,
            self.cfg.risk_weight_bullet,
        );

        let target = best_target(world, pred, self.cfg.lookahead_frames + 48.0, 1.0);
        let nearest_threat = nearest_threat_distance_px(world, pred);
        let nearest_saucer = nearest_saucer_distance_px(world, pred);

        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        if let Some(plan) = target {
            fire_alignment = angle_alignment(pred.angle, plan.aim_angle);
            attack += plan.value * weights.aggression_weight;
            if plan.distance_px < self.cfg.fire_distance_px {
                attack += 0.1;
            }
        }

        let flow_align = match self.mode {
            StanceMode::Panic => nearest_threat_angle(world, pred)
                .map(|angle| angle_alignment(pred.angle, (angle + 128) & 0xff))
                .unwrap_or(0.0),
            StanceMode::Harvest | StanceMode::LurkBreak => target
                .map(|plan| angle_alignment(pred.angle, plan.aim_angle))
                .unwrap_or_else(|| {
                    let tangent = (pred.angle + self.orbit_sign * 64) & 0xff;
                    angle_alignment(pred.angle, tangent)
                }),
        };

        let mut fire_term = 0.0;
        if input.fire {
            if can_fire(world, pred) {
                let fire_quality = estimate_fire_quality(world, pred);
                let min_quality = dynamic_min_fire_quality(
                    weights.min_fire_quality,
                    world.time_since_last_kill,
                    self.cfg.lurk_trigger_frames,
                    nearest_saucer,
                );
                let duplicate = target
                    .map(|plan| target_already_covered_by_ship_bullets(plan.target, &world.bullets))
                    .unwrap_or(false);
                let (active_bullets, shortest_life) = own_bullet_in_flight_stats(&world.bullets);
                let discipline_ok = disciplined_fire_gate(
                    active_bullets,
                    shortest_life,
                    fire_quality,
                    min_quality,
                    nearest_saucer,
                    nearest_threat,
                    duplicate,
                );

                if discipline_ok && !duplicate && fire_quality >= min_quality {
                    fire_term +=
                        weights.fire_reward * (0.32 + 0.68 * fire_alignment) * fire_quality;
                    fire_term -= weights.shot_penalty * 0.72;
                } else if duplicate {
                    fire_term -= weights.shot_penalty * 0.7;
                } else {
                    fire_term -= weights.miss_fire_penalty * (min_quality - fire_quality).max(0.0);
                    fire_term -= weights.shot_penalty * 0.24;
                }
            } else {
                fire_term -= weights.shot_penalty * 0.55;
            }
        }

        let mut control_term = 0.0;
        let control_scale = if risk > 3.0 {
            0.2
        } else if risk > 1.8 {
            0.38
        } else {
            1.0
        };

        if action != 0x00 {
            control_term -= weights.action_penalty * control_scale;
        }
        if input.left || input.right {
            control_term -= weights.turn_penalty * control_scale;
        }
        if input.thrust {
            control_term -= weights.thrust_penalty * control_scale;
        }

        if matches!(self.mode, StanceMode::Panic)
            && nearest_threat < self.cfg.panic_distance_px * 1.2
        {
            control_term += 0.06 * flow_align;
        }

        -risk * weights.survival_weight
            + attack
            + fire_term
            + weights.flow_weight * flow_align
            + control_term
            + center_term(pred, weights.center_weight)
            + edge_term(pred, weights.edge_penalty)
            + speed_term(pred, self.cfg.speed_soft_cap)
    }
}

impl AutopilotBot for StanceBot {
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
        self.rng = SeededRng::new(seed ^ hash ^ 0xC0D3_A22A);
        self.orbit_sign = if self.rng.next() & 1 == 0 { 1 } else { -1 };
        self.mode = StanceMode::Harvest;
        self.mode_hold_frames = 0;
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
        let desired = self.desired_mode(world, pred_now);
        self.maybe_switch_mode(desired);

        let mut best = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        for action in ACTIONS_WIDE {
            let value = self.evaluate_action(world, action);
            if value > best_value {
                best_value = value;
                best = action;
            }
        }

        decode_input_byte(best)
    }
}

impl RolloutBot {
    pub(super) fn new(cfg: RolloutConfig) -> Self {
        Self {
            cfg,
            rng: SeededRng::new(0xC0D3_5003),
        }
    }

    fn evaluate_rollout_action(
        &self,
        world: &WorldSnapshot,
        before: &RolloutState,
        after: &RolloutState,
        action: u8,
        fired: bool,
        step_index: u8,
    ) -> f64 {
        let input = decode_input_byte(action);
        let pred = after.ship;
        let target = best_target_at_step(
            world,
            pred,
            step_index,
            self.cfg.lookahead_frames + 40.0,
            1.0,
        );

        let mut aggression = self.cfg.aggression_weight;
        if world.time_since_last_kill >= self.cfg.lurk_trigger_frames {
            aggression *= self.cfg.lurk_boost;
        }

        let risk = total_risk_at_step(
            world,
            pred,
            step_index,
            self.cfg.lookahead_frames,
            self.cfg.risk_weight_asteroid,
            self.cfg.risk_weight_saucer,
            self.cfg.risk_weight_bullet,
        );

        let nearest_saucer = nearest_saucer_distance_px_at_step(world, pred, step_index);

        let mut attack = 0.0;
        let mut fire_alignment = 0.0;
        if let Some(plan) = target {
            fire_alignment = angle_alignment(pred.angle, plan.aim_angle);
            attack += plan.value * aggression;
            if plan.distance_px < self.cfg.fire_distance_px {
                attack += 0.1;
            }
        }

        let mut fire_term = 0.0;
        if fired {
            let fire_quality = estimate_fire_quality_at_step(world, pred, step_index);
            let min_quality = dynamic_min_fire_quality(
                self.cfg.min_fire_quality,
                world.time_since_last_kill + i32::from(step_index),
                self.cfg.lurk_trigger_frames,
                nearest_saucer,
            );
            if fire_quality >= min_quality {
                fire_term += self.cfg.fire_reward * (0.35 + 0.65 * fire_alignment) * fire_quality;
                fire_term -= self.cfg.shot_penalty * 0.72;
            } else {
                fire_term -= self.cfg.miss_fire_penalty * (min_quality - fire_quality);
                fire_term -= self.cfg.shot_penalty * 0.34;
            }
        } else if input.fire {
            fire_term -= self.cfg.shot_penalty * 0.44;
        }

        let mut control = 0.0;
        let control_scale = if risk > 3.0 {
            0.22
        } else if risk > 1.8 {
            0.42
        } else {
            1.0
        };
        if action != 0x00 {
            control -= self.cfg.action_penalty * control_scale;
        }
        if input.left || input.right {
            control -= self.cfg.turn_penalty * control_scale;
        }
        if input.thrust {
            control -= self.cfg.thrust_penalty * control_scale;
        }

        if before.bullets_in_flight >= SHIP_BULLET_LIMIT && input.fire {
            control -= self.cfg.shot_penalty * 0.4;
        }

        -risk * self.cfg.survival_weight
            + attack
            + fire_term
            + control
            + center_term(pred, self.cfg.center_weight)
            + edge_term(pred, self.cfg.edge_penalty)
            + speed_term(pred, self.cfg.speed_soft_cap)
    }

    fn simulate_step(
        &self,
        state: &RolloutState,
        action: u8,
        world: &WorldSnapshot,
    ) -> (RolloutState, bool) {
        let mut next = *state;
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

        next.ship.x = wrap_x_q12_4(next.ship.x + (next.ship.vx >> 4));
        next.ship.y = wrap_y_q12_4(next.ship.y + (next.ship.vy >> 4));

        let mut fired = false;
        if input.fire
            && next.ship.fire_cooldown <= 0
            && next.bullets_in_flight < SHIP_BULLET_LIMIT
            && world.bullets.len() < SHIP_BULLET_LIMIT
        {
            next.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
            next.bullets_in_flight += 1;
            fired = true;
        }

        (next, fired)
    }

    fn rollout_value(
        &self,
        world: &WorldSnapshot,
        state: &RolloutState,
        depth: u8,
        step_index: u8,
    ) -> f64 {
        if depth == 0 {
            return center_term(state.ship, self.cfg.center_weight)
                + edge_term(state.ship, self.cfg.edge_penalty)
                + speed_term(state.ship, self.cfg.speed_soft_cap)
                - total_risk_at_step(
                    world,
                    state.ship,
                    step_index,
                    self.cfg.lookahead_frames,
                    self.cfg.risk_weight_asteroid,
                    self.cfg.risk_weight_saucer,
                    self.cfg.risk_weight_bullet,
                ) * self.cfg.survival_weight;
        }

        let mut best = f64::NEG_INFINITY;
        for action in ACTIONS_ROLLOUT {
            let (next, fired) = self.simulate_step(state, action, world);
            let immediate =
                self.evaluate_rollout_action(world, state, &next, action, fired, step_index);
            let future = self.rollout_value(world, &next, depth - 1, step_index.saturating_add(1));
            let total = immediate + self.cfg.discount * future;
            if total > best {
                best = total;
            }
        }

        best
    }
}

impl AutopilotBot for RolloutBot {
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
        self.rng = SeededRng::new(seed ^ hash ^ 0xC0D3_A33A);
    }

    fn next_input(&mut self, world: &WorldSnapshot) -> FrameInput {
        if world.is_game_over || !world.ship.can_control {
            return no_input();
        }

        let root = RolloutState {
            ship: PredictedShip {
                x: world.ship.x,
                y: world.ship.y,
                vx: world.ship.vx,
                vy: world.ship.vy,
                angle: world.ship.angle,
                radius: world.ship.radius,
                fire_cooldown: world.ship.fire_cooldown,
            },
            bullets_in_flight: world.bullets.len(),
        };

        let mut best = 0x00u8;
        let mut best_value = f64::NEG_INFINITY;
        for action in ACTIONS_ROLLOUT {
            let (next, fired) = self.simulate_step(&root, action, world);
            let immediate = self.evaluate_rollout_action(world, &root, &next, action, fired, 0);
            let future = if self.cfg.depth > 1 {
                self.rollout_value(world, &next, self.cfg.depth - 1, 1)
            } else {
                0.0
            };
            let total = immediate + self.cfg.discount * future;
            if total > best_value {
                best_value = total;
                best = action;
            }
        }

        decode_input_byte(best)
    }
}

#[inline]
fn predict_ship(world: &WorldSnapshot, input_byte: u8) -> PredictedShip {
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

#[inline]
fn can_fire(world: &WorldSnapshot, pred: PredictedShip) -> bool {
    pred.fire_cooldown <= 0 && world.bullets.len() < SHIP_BULLET_LIMIT
}

fn total_risk(
    world: &WorldSnapshot,
    pred: PredictedShip,
    horizon: f64,
    asteroid_weight: f64,
    saucer_weight: f64,
    bullet_weight: f64,
) -> f64 {
    let mut risk = 0.0;

    for asteroid in &world.asteroids {
        risk += entity_risk(
            pred,
            asteroid.x,
            asteroid.y,
            asteroid.vx,
            asteroid.vy,
            asteroid.radius,
            asteroid_weight,
            horizon,
        );
    }
    for saucer in &world.saucers {
        let w = if saucer.small {
            saucer_weight * 1.28
        } else {
            saucer_weight
        };
        risk += entity_risk(
            pred,
            saucer.x,
            saucer.y,
            saucer.vx,
            saucer.vy,
            saucer.radius,
            w,
            horizon,
        );
    }
    for bullet in &world.saucer_bullets {
        risk += entity_risk(
            pred,
            bullet.x,
            bullet.y,
            bullet.vx,
            bullet.vy,
            bullet.radius,
            bullet_weight,
            horizon,
        );
    }

    risk
}

fn total_risk_at_step(
    world: &WorldSnapshot,
    pred: PredictedShip,
    step_index: u8,
    horizon: f64,
    asteroid_weight: f64,
    saucer_weight: f64,
    bullet_weight: f64,
) -> f64 {
    let mut risk = 0.0;

    for asteroid in &world.asteroids {
        let (x, y) = advance_entity(asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, step_index);
        risk += entity_risk(
            pred,
            x,
            y,
            asteroid.vx,
            asteroid.vy,
            asteroid.radius,
            asteroid_weight,
            horizon,
        );
    }
    for saucer in &world.saucers {
        let (x, y) = advance_entity(saucer.x, saucer.y, saucer.vx, saucer.vy, step_index);
        let w = if saucer.small {
            saucer_weight * 1.28
        } else {
            saucer_weight
        };
        risk += entity_risk(pred, x, y, saucer.vx, saucer.vy, saucer.radius, w, horizon);
    }
    for bullet in &world.saucer_bullets {
        let (x, y) = advance_entity(bullet.x, bullet.y, bullet.vx, bullet.vy, step_index);
        risk += entity_risk(
            pred,
            x,
            y,
            bullet.vx,
            bullet.vy,
            bullet.radius,
            bullet_weight,
            horizon,
        );
    }

    risk
}

fn entity_risk(
    pred: PredictedShip,
    ex: i32,
    ey: i32,
    evx: i32,
    evy: i32,
    radius: i32,
    weight: f64,
    horizon: f64,
) -> f64 {
    let approach =
        torus_relative_approach(pred.x, pred.y, pred.vx, pred.vy, ex, ey, evx, evy, horizon);
    let safe = (pred.radius + radius + 8) as f64;

    let closeness = (safe / (approach.closest_px + 1.0)).powf(2.0);
    let immediate = (safe / (approach.immediate_px + 1.0)).powf(1.3);
    let closing_boost = if approach.dot < 0.0 { 1.24 } else { 0.92 };
    let time_boost =
        1.0 + ((horizon - approach.t_closest) / horizon.max(1.0)).clamp(0.0, 1.0) * 0.35;

    weight * (0.74 * closeness + 0.26 * immediate) * closing_boost * time_boost
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
                if !t.is_finite() || t < 0.0 || t > lead_cap {
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

fn best_target(
    world: &WorldSnapshot,
    ship: PredictedShip,
    lead_cap: f64,
    value_multiplier: f64,
) -> Option<TargetPlan> {
    best_target_internal(world, ship, 0, lead_cap, value_multiplier)
}

fn best_target_at_step(
    world: &WorldSnapshot,
    ship: PredictedShip,
    step_index: u8,
    lead_cap: f64,
    value_multiplier: f64,
) -> Option<TargetPlan> {
    best_target_internal(world, ship, step_index, lead_cap, value_multiplier)
}

fn best_target_internal(
    world: &WorldSnapshot,
    ship: PredictedShip,
    step_index: u8,
    lead_cap: f64,
    value_multiplier: f64,
) -> Option<TargetPlan> {
    let speed_px = ((ship.vx as f64 / 256.0).powi(2) + (ship.vy as f64 / 256.0).powi(2)).sqrt();
    let bullet_speed = 8.6 + speed_px * 0.33;

    let mut best: Option<TargetPlan> = None;
    let mut consider = |target: MovingTarget| {
        let Some(intercept) = best_wrapped_aim(
            ship.x,
            ship.y,
            ship.vx,
            ship.vy,
            ship.angle,
            target.x,
            target.y,
            target.vx,
            target.vy,
            bullet_speed,
            lead_cap,
        ) else {
            return;
        };

        let angle_bonus = angle_alignment(ship.angle, intercept.aim_angle) * 0.55;
        let mut value = (target.value_hint * value_multiplier) / (intercept.distance_px + 20.0);
        value += angle_bonus;
        value *=
            1.0 + (1.0 - (intercept.intercept_frames / lead_cap.max(1.0))).clamp(0.0, 1.0) * 0.18;

        let candidate = TargetPlan {
            aim_angle: intercept.aim_angle,
            distance_px: intercept.distance_px,
            intercept_frames: intercept.intercept_frames,
            value,
            target,
        };

        match best {
            None => best = Some(candidate),
            Some(existing) if candidate.value > existing.value => best = Some(candidate),
            _ => {}
        }
    };

    for asteroid in &world.asteroids {
        let (x, y) = advance_entity(asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, step_index);
        consider(MovingTarget {
            x,
            y,
            vx: asteroid.vx,
            vy: asteroid.vy,
            radius: asteroid.radius,
            value_hint: asteroid_weight(asteroid.size),
        });
    }

    for saucer in &world.saucers {
        let (x, y) = advance_entity(saucer.x, saucer.y, saucer.vx, saucer.vy, step_index);
        let mut value = if saucer.small { 2.8 } else { 1.9 };
        let approach = torus_relative_approach(
            ship.x, ship.y, ship.vx, ship.vy, x, y, saucer.vx, saucer.vy, 24.0,
        );
        if approach.closest_px < 210.0 {
            let urgency = ((210.0 - approach.closest_px) / 210.0).clamp(0.0, 1.0);
            value *= 1.0 + urgency * 0.7;
        }

        consider(MovingTarget {
            x,
            y,
            vx: saucer.vx,
            vy: saucer.vy,
            radius: saucer.radius,
            value_hint: value,
        });
    }

    best
}

#[inline]
fn asteroid_weight(size: AsteroidSizeSnapshot) -> f64 {
    match size {
        AsteroidSizeSnapshot::Large => 0.98,
        AsteroidSizeSnapshot::Medium => 1.26,
        AsteroidSizeSnapshot::Small => 1.5,
    }
}

fn estimate_fire_quality(world: &WorldSnapshot, ship: PredictedShip) -> f64 {
    estimate_fire_quality_at_step(world, ship, 0)
}

fn estimate_fire_quality_at_step(
    world: &WorldSnapshot,
    ship: PredictedShip,
    step_index: u8,
) -> f64 {
    let (dx, dy) = displace_q12_4(ship.angle, ship.radius + 6);
    let start_x = wrap_x_q12_4(ship.x + dx);
    let start_y = wrap_y_q12_4(ship.y + dy);

    let ship_speed_approx = ((ship.vx.abs() + ship.vy.abs()) * 3) >> 2;
    let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
    let (bvx, bvy) = velocity_q8_8(ship.angle, bullet_speed_q8_8);
    let bullet_vx = ship.vx + bvx;
    let bullet_vy = ship.vy + bvy;

    let mut best: f64 = 0.0;
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
        best = best.max(weight * hit_score * time_factor);
    };

    for asteroid in &world.asteroids {
        let (x, y) = advance_entity(asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, step_index);
        consider(
            x,
            y,
            asteroid.vx,
            asteroid.vy,
            asteroid.radius,
            asteroid_weight(asteroid.size),
        );
    }

    for saucer in &world.saucers {
        let (x, y) = advance_entity(saucer.x, saucer.y, saucer.vx, saucer.vy, step_index);
        consider(
            x,
            y,
            saucer.vx,
            saucer.vy,
            saucer.radius,
            if saucer.small { 2.7 } else { 1.9 },
        );
    }

    best
}

fn nearest_threat_distance_px(world: &WorldSnapshot, ship: PredictedShip) -> f64 {
    nearest_threat_distance_px_at_step(world, ship, 0)
}

fn nearest_threat_distance_px_at_step(
    world: &WorldSnapshot,
    ship: PredictedShip,
    step_index: u8,
) -> f64 {
    let mut nearest = f64::MAX;
    let mut consider = |x: i32, y: i32| {
        let (dx, dy) = torus_delta(ship.x, ship.y, x, y);
        nearest = nearest.min((dx * dx + dy * dy).sqrt());
    };

    for asteroid in &world.asteroids {
        let (x, y) = advance_entity(asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, step_index);
        consider(x, y);
    }
    for saucer in &world.saucers {
        let (x, y) = advance_entity(saucer.x, saucer.y, saucer.vx, saucer.vy, step_index);
        consider(x, y);
    }
    for bullet in &world.saucer_bullets {
        let (x, y) = advance_entity(bullet.x, bullet.y, bullet.vx, bullet.vy, step_index);
        consider(x, y);
    }

    if nearest == f64::MAX {
        9999.0
    } else {
        nearest
    }
}

fn nearest_saucer_distance_px(world: &WorldSnapshot, ship: PredictedShip) -> f64 {
    nearest_saucer_distance_px_at_step(world, ship, 0)
}

fn nearest_saucer_distance_px_at_step(
    world: &WorldSnapshot,
    ship: PredictedShip,
    step_index: u8,
) -> f64 {
    let mut nearest = f64::MAX;
    for saucer in &world.saucers {
        let (x, y) = advance_entity(saucer.x, saucer.y, saucer.vx, saucer.vy, step_index);
        let (dx, dy) = torus_delta(ship.x, ship.y, x, y);
        nearest = nearest.min((dx * dx + dy * dy).sqrt());
    }

    if nearest == f64::MAX {
        9999.0
    } else {
        nearest
    }
}

#[inline]
fn looks_like_saucer_target(target: MovingTarget) -> bool {
    target.value_hint >= 1.8
}

#[inline]
fn compute_endgame_pressure(world: &WorldSnapshot) -> EndgamePressure {
    let frame = world.frame_count as i32;
    let remaining_frames = (ENDGAME_FRAME_CAP - frame).max(0);

    let endgame_phase = if frame > ENDGAME_PUSH_START_FRAME {
        ((frame - ENDGAME_PUSH_START_FRAME) as f64
            / (ENDGAME_FRAME_CAP - ENDGAME_PUSH_START_FRAME) as f64)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let endgame_phase = endgame_phase.powi(2);

    let life_buffer = ((world.lives - 1).max(0) as f64 / 10.0).clamp(0.0, 1.0);
    let late_life_push = ((world.lives - 3).max(0) as f64 / 8.0).clamp(0.0, 1.0);

    // Saucer intensity saturates around wave 9+ in this sim; push harder when it is active.
    let wave_pressure = ((world.wave - 8).max(0) as f64 / 8.0).clamp(0.0, 1.0);
    let visible_saucer_pressure = (world.saucers.len() as f64 / 3.0).clamp(0.0, 1.0);
    let spawn_timer = world.saucer_spawn_timer.max(0);
    let spawn_imminence = ((96 - spawn_timer) as f64 / 96.0).clamp(0.0, 1.0);
    let saucer_push =
        (0.52 * wave_pressure + 0.34 * visible_saucer_pressure + 0.14 * spawn_imminence)
            .clamp(0.0, 1.0);

    let remaining_ratio = (remaining_frames as f64 / ENDGAME_FRAME_CAP as f64).clamp(0.0, 1.0);
    let time_urgency = (1.0 - remaining_ratio).clamp(0.0, 1.0);

    // Every death burns 75 control-less frames; soften risk push when remaining time is tight.
    let death_frame_tax =
        (SHIP_RESPAWN_FRAMES as f64 / remaining_frames.max(1) as f64).clamp(0.0, 1.0);
    let death_tax_weight = (1.0 - 0.42 * life_buffer).clamp(0.5, 1.0);
    let death_tax = death_frame_tax * death_tax_weight;

    let base_push = endgame_phase * (0.55 + 0.45 * late_life_push)
        + saucer_push * (0.35 + 0.25 * life_buffer)
        + time_urgency * 0.18;
    let endgame_push = (base_push * (1.0 - death_tax * 0.45)).clamp(0.0, 1.0);

    let target_value_mult = 1.0 + 0.24 * endgame_push + 0.42 * saucer_push;
    let min_fire_quality_relief =
        (0.02 + 0.1 * endgame_push + 0.1 * saucer_push - 0.05 * death_tax).clamp(0.0, 0.22);
    let control_relief =
        (0.08 + 0.22 * endgame_push + 0.18 * saucer_push - 0.1 * death_tax).clamp(0.0, 0.42);

    let risk_asteroid_mult = (1.0 - 0.18 * endgame_push + 0.2 * death_tax).clamp(0.62, 1.32);
    let risk_saucer_mult =
        (1.0 - 0.3 * endgame_push - 0.16 * saucer_push + 0.24 * death_tax).clamp(0.48, 1.3);
    let risk_bullet_mult =
        (1.0 - 0.22 * endgame_push - 0.08 * saucer_push + 0.24 * death_tax).clamp(0.52, 1.34);

    EndgamePressure {
        endgame_push,
        saucer_push,
        target_value_mult,
        min_fire_quality_relief,
        control_relief,
        risk_asteroid_mult,
        risk_saucer_mult,
        risk_bullet_mult,
    }
}

fn nearest_threat_angle(world: &WorldSnapshot, ship: PredictedShip) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;

    let mut consider = |x: i32, y: i32| {
        let (dx, dy) = torus_delta(ship.x, ship.y, x, y);
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = atan2_bam((dy * 16.0) as i32, (dx * 16.0) as i32);

        match best {
            None => best = Some((dist, angle)),
            Some((best_dist, _)) if dist < best_dist => best = Some((dist, angle)),
            _ => {}
        }
    };

    for asteroid in &world.asteroids {
        consider(asteroid.x, asteroid.y);
    }
    for saucer in &world.saucers {
        consider(saucer.x, saucer.y);
    }
    for bullet in &world.saucer_bullets {
        consider(bullet.x, bullet.y);
    }

    best.map(|(_, angle)| angle)
}

#[inline]
fn center_term(ship: PredictedShip, center_weight: f64) -> f64 {
    let cx = shortest_delta_q12_4(ship.x, WORLD_WIDTH_Q12_4 / 2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
    let cy = shortest_delta_q12_4(ship.y, WORLD_HEIGHT_Q12_4 / 2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
    let center_dist = (cx * cx + cy * cy).sqrt();
    -(center_dist / 900.0) * center_weight
}

#[inline]
fn edge_term(ship: PredictedShip, edge_penalty: f64) -> f64 {
    let left_edge = ship.x as f64 / 16.0;
    let right_edge = (WORLD_WIDTH_Q12_4 - ship.x) as f64 / 16.0;
    let top_edge = ship.y as f64 / 16.0;
    let bottom_edge = (WORLD_HEIGHT_Q12_4 - ship.y) as f64 / 16.0;
    let min_edge = left_edge.min(right_edge).min(top_edge).min(bottom_edge);
    -((140.0 - min_edge).max(0.0) / 140.0) * edge_penalty
}

#[inline]
fn speed_term(ship: PredictedShip, speed_soft_cap: f64) -> f64 {
    let speed_px = ((ship.vx as f64 / 256.0).powi(2) + (ship.vy as f64 / 256.0).powi(2)).sqrt();
    if speed_px > speed_soft_cap {
        -((speed_px - speed_soft_cap) / speed_soft_cap.max(0.1)) * 0.38
    } else {
        0.0
    }
}

#[inline]
fn angle_alignment(current: i32, target: i32) -> f64 {
    let error = signed_angle_delta(current, target).abs() as f64;
    (1.0 - (error / 128.0)).clamp(0.0, 1.0)
}

#[inline]
fn dynamic_min_fire_quality(
    base: f64,
    time_since_last_kill: i32,
    lurk_trigger_frames: i32,
    nearest_saucer: f64,
) -> f64 {
    let mut floor = base;

    if time_since_last_kill >= lurk_trigger_frames {
        floor -= 0.03;
    }
    if nearest_saucer < 240.0 {
        floor -= 0.03;
    }
    if nearest_saucer < 140.0 {
        floor -= 0.04;
    }

    floor.clamp(0.08, 0.6)
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

fn target_already_covered_by_ship_bullets(
    target: MovingTarget,
    bullets: &[BulletSnapshot],
) -> bool {
    bullets
        .iter()
        .any(|bullet| bullet_confidently_tracks_target(bullet, target))
}

fn bullet_confidently_tracks_target(bullet: &BulletSnapshot, target: MovingTarget) -> bool {
    if !bullet.alive || bullet.life <= 0 {
        return false;
    }

    let horizon = (bullet.life as f64).min(32.0).max(1.0);
    let (closest, t) = projectile_wrap_closest_approach(
        bullet.x, bullet.y, bullet.vx, bullet.vy, target.x, target.y, target.vx, target.vy, horizon,
    );
    let hit_radius = (bullet.radius + target.radius) as f64;

    closest <= hit_radius * 1.03 && t <= horizon * 0.9
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
    let strict_quality = (min_fire_quality + 0.08).clamp(0.16, 0.9);
    if active_bullets == 0 {
        return fire_quality >= strict_quality;
    }

    if !duplicate_target_shot {
        let rapid_switch = nearest_threat_px < 118.0 || nearest_saucer_px < 136.0;
        let switch_quality = (strict_quality + 0.18).clamp(0.24, 0.95);
        if rapid_switch && fire_quality >= switch_quality {
            return true;
        }
    }

    let emergency = nearest_threat_px < 78.0 || nearest_saucer_px < 88.0;
    let life_gate = if emergency { 3 } else { 2 };
    if shortest_life > life_gate {
        return false;
    }

    let stacked_quality = (strict_quality + if emergency { 0.06 } else { 0.16 }).clamp(0.22, 0.94);
    fire_quality >= stacked_quality
}

#[inline]
fn add_repulsion(ship: PredictedShip, x: i32, y: i32, weight: f64, fx: &mut f64, fy: &mut f64) {
    let (dx, dy) = torus_delta(ship.x, ship.y, x, y);
    let dist_sq = (dx * dx + dy * dy).max(1.0);
    let dist = dist_sq.sqrt();
    let strength = weight / dist_sq;

    *fx -= (dx / dist) * strength;
    *fy -= (dy / dist) * strength;
}

#[inline]
fn torus_delta(from_x: i32, from_y: i32, to_x: i32, to_y: i32) -> (f64, f64) {
    (
        shortest_delta_q12_4(from_x, to_x, WORLD_WIDTH_Q12_4) as f64 / 16.0,
        shortest_delta_q12_4(from_y, to_y, WORLD_HEIGHT_Q12_4) as f64 / 16.0,
    )
}

fn advance_entity(x: i32, y: i32, vx: i32, vy: i32, steps: u8) -> (i32, i32) {
    let mut nx = x;
    let mut ny = y;
    let step_x = vx >> 4;
    let step_y = vy >> 4;

    for _ in 0..steps {
        nx = wrap_x_q12_4(nx + step_x);
        ny = wrap_y_q12_4(ny + step_y);
    }

    (nx, ny)
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

pub(super) fn create_codex_bot(id: &str) -> Option<Box<dyn AutopilotBot>> {
    if id == "codex-potential-adaptive" {
        let cfg = apply_adaptive_profile(adaptive_base_potential_config(), load_adaptive_profile());
        return Some(Box::new(PotentialBot::new(cfg)));
    }
    if let Some(cfg) = potential_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(PotentialBot::new(*cfg)));
    }
    if let Some(cfg) = stance_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(StanceBot::new(*cfg)));
    }
    if let Some(cfg) = rollout_bot_configs().iter().find(|cfg| cfg.id == id) {
        return Some(Box::new(RolloutBot::new(*cfg)));
    }
    None
}

pub(super) fn describe_codex_bots() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    let adaptive = adaptive_base_potential_config();
    out.push((adaptive.id, adaptive.description));
    out.extend(
        potential_bot_configs()
            .iter()
            .map(|cfg| (cfg.id, cfg.description)),
    );
    out.extend(
        stance_bot_configs()
            .iter()
            .map(|cfg| (cfg.id, cfg.description)),
    );
    out.extend(
        rollout_bot_configs()
            .iter()
            .map(|cfg| (cfg.id, cfg.description)),
    );
    out
}

pub(super) fn codex_bot_ids() -> Vec<&'static str> {
    let mut ids = vec![adaptive_base_potential_config().id];
    ids.extend(potential_bot_configs().iter().map(|cfg| cfg.id));
    ids.extend(stance_bot_configs().iter().map(|cfg| cfg.id));
    ids.extend(rollout_bot_configs().iter().map(|cfg| cfg.id));
    ids
}

pub(super) fn codex_manifest_entries() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    let mut out = Vec::new();

    let adaptive_profile = load_adaptive_profile();
    let mut adaptive_cfg = serde_json::to_value(apply_adaptive_profile(
        adaptive_base_potential_config(),
        adaptive_profile,
    ))
    .expect("adaptive potential config should serialize");
    adaptive_cfg["adaptive_profile_path"] =
        serde_json::Value::String(ADAPTIVE_PROFILE_REL_PATH.to_string());
    adaptive_cfg["adaptive_profile"] =
        serde_json::to_value(adaptive_profile).expect("adaptive profile should serialize");
    out.push((
        adaptive_base_potential_config().id,
        "codex_potential_adaptive",
        adaptive_cfg,
    ));

    for cfg in potential_bot_configs() {
        out.push((
            cfg.id,
            "codex_potential",
            serde_json::to_value(cfg).expect("potential config should serialize"),
        ));
    }

    for cfg in stance_bot_configs() {
        out.push((
            cfg.id,
            "codex_stance",
            serde_json::to_value(cfg).expect("stance config should serialize"),
        ));
    }

    for cfg in rollout_bot_configs() {
        out.push((
            cfg.id,
            "codex_rollout",
            serde_json::to_value(cfg).expect("rollout config should serialize"),
        ));
    }

    out
}

#[allow(dead_code)]
fn _score_hint_for_target(target: &MovingTarget) -> u32 {
    if target.value_hint >= 2.6 {
        SCORE_SMALL_SAUCER
    } else if target.value_hint >= 1.8 {
        SCORE_LARGE_SAUCER
    } else {
        0
    }
}
