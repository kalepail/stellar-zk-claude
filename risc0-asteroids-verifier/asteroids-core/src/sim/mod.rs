use alloc::vec::Vec;

use crate::constants::{
    ASTEROID_CAP, ASTEROID_RADIUS_LARGE, ASTEROID_RADIUS_MEDIUM, ASTEROID_RADIUS_SMALL,
    ASTEROID_SPEED_LARGE_Q8_8, ASTEROID_SPEED_MEDIUM_Q8_8, ASTEROID_SPEED_SMALL_Q8_8,
    EXTRA_LIFE_SCORE_STEP, LURK_SAUCER_SPAWN_FAST_FRAMES, LURK_TIME_THRESHOLD_FRAMES,
    SAUCER_BULLET_LIFETIME_FRAMES, SAUCER_BULLET_LIMIT, SAUCER_BULLET_SPEED_Q8_8,
    SAUCER_RADIUS_LARGE, SAUCER_RADIUS_SMALL, SAUCER_SPAWN_MAX_FRAMES, SAUCER_SPAWN_MIN_FRAMES,
    SAUCER_SPEED_LARGE_Q8_8, SAUCER_SPEED_SMALL_Q8_8, SCORE_LARGE_ASTEROID, SCORE_LARGE_SAUCER,
    SCORE_MEDIUM_ASTEROID, SCORE_SMALL_ASTEROID, SCORE_SMALL_SAUCER, SHIP_BULLET_COOLDOWN_FRAMES,
    SHIP_BULLET_LIFETIME_FRAMES, SHIP_BULLET_LIMIT, SHIP_BULLET_SPEED_Q8_8,
    SHIP_MAX_SPEED_SQ_Q16_16, SHIP_RADIUS, SHIP_RESPAWN_FRAMES, SHIP_SPAWN_INVULNERABLE_FRAMES,
    SHIP_THRUST_Q8_8, SHIP_TURN_SPEED_BAM, STARTING_LIVES, WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use crate::error::RuleCode;
use crate::fixed_point::{
    apply_drag, atan2_bam, clamp_i32, clamp_speed_q8_8, collision_dist_sq_q12_4, cos_bam,
    displace_q12_4, shortest_delta_q12_4, sin_bam, velocity_q8_8, wrap_x_q12_4, wrap_y_q12_4,
};
use crate::rng::SeededRng;
use crate::tape::{decode_input_byte, encode_input_byte, FrameInput};

mod game;

use game::Game;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameMode {
    Playing,
    GameOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsteroidSize {
    Large,
    Medium,
    Small,
}

#[derive(Clone, Copy, Debug)]
struct Ship {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    angle: i32,
    radius: i32,
    can_control: bool,
    fire_cooldown: i32,
    respawn_timer: i32,
    invulnerable_timer: i32,
}

#[derive(Clone, Copy, Debug)]
struct Asteroid {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    angle: i32,
    alive: bool,
    radius: i32,
    size: AsteroidSize,
    spin: i32,
}

#[derive(Clone, Copy, Debug)]
struct Bullet {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    alive: bool,
    life: i32,
}

#[derive(Clone, Copy, Debug)]
struct Saucer {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    alive: bool,
    radius: i32,
    small: bool,
    fire_cooldown: i32,
    drift_timer: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayResult {
    pub final_score: u32,
    pub final_rng_state: u32,
    pub frame_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayCheckpoint {
    pub frame_count: u32,
    pub rng_state: u32,
    pub score: u32,
    pub lives: i32,
    pub wave: i32,
    pub asteroids: usize,
    pub bullets: usize,
    pub saucers: usize,
    pub saucer_bullets: usize,
    pub ship_x: i32,
    pub ship_y: i32,
    pub ship_vx: i32,
    pub ship_vy: i32,
    pub ship_angle: i32,
    pub ship_can_control: bool,
    pub ship_fire_cooldown: i32,
    pub ship_respawn_timer: i32,
    pub ship_invulnerable_timer: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayViolation {
    pub frame_count: u32,
    pub rule: RuleCode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipSnapshot {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub angle: i32,
    pub radius: i32,
    pub can_control: bool,
    pub fire_cooldown: i32,
    pub respawn_timer: i32,
    pub invulnerable_timer: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsteroidSizeSnapshot {
    Large,
    Medium,
    Small,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsteroidSnapshot {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub angle: i32,
    pub alive: bool,
    pub radius: i32,
    pub size: AsteroidSizeSnapshot,
    pub spin: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BulletSnapshot {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub alive: bool,
    pub radius: i32,
    pub life: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaucerSnapshot {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub alive: bool,
    pub radius: i32,
    pub small: bool,
    pub fire_cooldown: i32,
    pub drift_timer: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldSnapshot {
    pub frame_count: u32,
    pub score: u32,
    pub lives: i32,
    pub wave: i32,
    pub is_game_over: bool,
    pub rng_state: u32,
    pub saucer_spawn_timer: i32,
    pub time_since_last_kill: i32,
    pub next_extra_life_score: u32,
    pub ship: ShipSnapshot,
    pub asteroids: Vec<AsteroidSnapshot>,
    pub bullets: Vec<BulletSnapshot>,
    pub saucers: Vec<SaucerSnapshot>,
    pub saucer_bullets: Vec<BulletSnapshot>,
}

pub struct LiveGame {
    game: Game,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransitionState {
    frame_count: u32,
    score: u32,
    wave: i32,
    asteroids: usize,
    bullets: usize,
    saucers: usize,
    ship_x: i32,
    ship_y: i32,
    ship_vx: i32,
    ship_vy: i32,
    ship_angle: i32,
    ship_can_control: bool,
    ship_fire_cooldown: i32,
    ship_fire_latch: bool,
    ship_respawn_timer: i32,
}

const SHIP_SPAWN_X_Q12_4: i32 = 7_680;
const SHIP_SPAWN_Y_Q12_4: i32 = 5_760;
const SHIP_RESPAWN_EDGE_PADDING_Q12_4: i32 = 1_536; // 96px
const SHIP_RESPAWN_GRID_STEP_Q12_4: i32 = 1_024; // 64px
const WAVE_SAFE_DIST_Q12_4: i32 = 2_880;
const WAVE_SAFE_DIST_SQ_Q24_8: i32 = WAVE_SAFE_DIST_Q12_4 * WAVE_SAFE_DIST_Q12_4;

const SAUCER_START_X_LEFT_Q12_4: i32 = -480;
const SAUCER_START_X_RIGHT_Q12_4: i32 = 15_840;
const SAUCER_START_Y_MIN_Q12_4: i32 = 1_152;
const SAUCER_START_Y_MAX_Q12_4: i32 = 10_368;
const SAUCER_CULL_MIN_X_Q12_4: i32 = -1_280;
const SAUCER_CULL_MAX_X_Q12_4: i32 = 16_640;

const BULLET_RADIUS: i32 = 2;

const ASTEROID_VEC_CAPACITY: usize = ASTEROID_CAP + 16;
const SHIP_BULLET_VEC_CAPACITY: usize = SHIP_BULLET_LIMIT;
const SAUCER_VEC_CAPACITY: usize = 4;
const SAUCER_BULLET_VEC_CAPACITY: usize = SAUCER_BULLET_LIMIT;
const MAX_SCORE_DELTA_PER_FRAME: u32 = (SHIP_BULLET_LIMIT as u32) * SCORE_SMALL_SAUCER;
const LEGAL_SCORE_DELTA_TABLE_SIZE: usize = MAX_SCORE_DELTA_PER_FRAME as usize + 1;
const SCORE_EVENT_VALUES: [u32; 5] = [
    SCORE_LARGE_ASTEROID,
    SCORE_MEDIUM_ASTEROID,
    SCORE_SMALL_ASTEROID,
    SCORE_LARGE_SAUCER,
    SCORE_SMALL_SAUCER,
];
const LEGAL_SCORE_DELTAS: [bool; LEGAL_SCORE_DELTA_TABLE_SIZE] = build_legal_score_delta_table();

pub fn replay(seed: u32, inputs: &[u8]) -> ReplayResult {
    let mut game = Game::new(seed);

    for input in inputs {
        game.step(*input);
    }

    game.result()
}

pub fn replay_strict(seed: u32, inputs: &[u8]) -> Result<ReplayResult, ReplayViolation> {
    let mut game = Game::new(seed);
    game.validate_invariants().map_err(|rule| ReplayViolation {
        frame_count: game.frame_count(),
        rule,
    })?;

    for input in inputs {
        let frame_input = decode_input_byte(*input);
        let before_step = game.transition_state();
        game.step_decoded(frame_input);
        let after_step = game.transition_state();

        validate_transition(&before_step, &after_step, frame_input).map_err(|rule| {
            ReplayViolation {
                frame_count: game.frame_count(),
                rule,
            }
        })?;

        game.validate_invariants().map_err(|rule| ReplayViolation {
            frame_count: game.frame_count(),
            rule,
        })?;
    }

    Ok(game.result())
}

fn validate_transition(
    prev: &TransitionState,
    next: &TransitionState,
    input: FrameInput,
) -> Result<(), RuleCode> {
    if next.score < prev.score {
        return Err(RuleCode::ProgressionScoreDelta);
    }
    if !is_legal_score_delta(next.score - prev.score) {
        return Err(RuleCode::ProgressionScoreDelta);
    }

    if next.wave < prev.wave || next.wave > prev.wave + 1 {
        return Err(RuleCode::ProgressionWaveAdvance);
    }
    let wave_advanced_this_frame = next.wave == prev.wave + 1;
    if wave_advanced_this_frame {
        let expected_asteroid_count = wave_asteroid_count(next.wave);
        if next.asteroids != expected_asteroid_count || next.saucers != 0 {
            return Err(RuleCode::ProgressionWaveAdvance);
        }
    }

    // Keep this in i32 for RV32 guest performance (speed components are tightly bounded).
    let ship_speed_sq = (next.ship_vx * next.ship_vx) + (next.ship_vy * next.ship_vy);
    if ship_speed_sq > SHIP_MAX_SPEED_SQ_Q16_16 {
        return Err(RuleCode::ShipSpeedClamp);
    }

    let turn_delta = (next.ship_angle - prev.ship_angle) & 0xff;
    if !wave_advanced_this_frame {
        if prev.ship_can_control {
            let expected_turn_delta = expected_ship_turn_delta(input);
            if turn_delta != expected_turn_delta {
                return Err(RuleCode::ShipTurnRateStep);
            }
        } else if !next.ship_can_control && turn_delta != 0 {
            return Err(RuleCode::ShipTurnRateStep);
        }
    }

    let ship_died_this_frame = prev.ship_can_control
        && !next.ship_can_control
        && next.ship_respawn_timer >= SHIP_RESPAWN_FRAMES;
    if !wave_advanced_this_frame {
        let respawned_this_frame = !prev.ship_can_control && next.ship_can_control;

        if prev.ship_can_control {
            if ship_died_this_frame {
                // queue_ship_respawn() zeros vx/vy after movement, so we can't derive the expected
                // displacement from the post-step velocity in this case.
                let dx = shortest_delta_q12_4(prev.ship_x, next.ship_x, WORLD_WIDTH_Q12_4);
                let dy = shortest_delta_q12_4(prev.ship_y, next.ship_y, WORLD_HEIGHT_Q12_4);
                let step_sq = (dx * dx) + (dy * dy);
                if step_sq > max_ship_step_sq_q12_4() {
                    return Err(RuleCode::ShipPositionStep);
                }
            } else {
                let expected_x = wrap_x_q12_4(prev.ship_x + (next.ship_vx >> 4));
                let expected_y = wrap_y_q12_4(prev.ship_y + (next.ship_vy >> 4));
                if next.ship_x != expected_x || next.ship_y != expected_y {
                    return Err(RuleCode::ShipPositionStep);
                }
            }
        } else if !respawned_this_frame {
            if prev.ship_x != next.ship_x || prev.ship_y != next.ship_y {
                return Err(RuleCode::ShipPositionStep);
            }
        }
    }

    let expected_fire_cooldown = expected_ship_fire_cooldown(
        prev,
        next,
        input,
        wave_advanced_this_frame,
        ship_died_this_frame,
    );
    if next.ship_fire_cooldown != expected_fire_cooldown {
        return Err(RuleCode::PlayerBulletCooldownBypass);
    }
    let expected_fire_latch =
        expected_ship_fire_latch(prev, input, wave_advanced_this_frame, ship_died_this_frame);
    if next.ship_fire_latch != expected_fire_latch {
        return Err(RuleCode::PlayerBulletCooldownBypass);
    }

    Ok(())
}

#[inline]
fn wave_asteroid_count(wave: i32) -> usize {
    if wave <= 4 {
        (4 + (wave - 1) * 2) as usize
    } else {
        core::cmp::min(16, 10 + (wave - 4)) as usize
    }
}

#[inline]
fn max_ship_step_sq_q12_4() -> i32 {
    (SHIP_MAX_SPEED_SQ_Q16_16 >> 8) + 4
}

#[inline]
fn expected_ship_turn_delta(input: FrameInput) -> i32 {
    if input.left == input.right {
        0
    } else if input.left {
        (256 - SHIP_TURN_SPEED_BAM) & 0xff
    } else {
        SHIP_TURN_SPEED_BAM
    }
}

#[inline]
fn expected_ship_fire_cooldown(
    prev: &TransitionState,
    next: &TransitionState,
    input: FrameInput,
    wave_advanced_this_frame: bool,
    ship_died_this_frame: bool,
) -> i32 {
    if wave_advanced_this_frame {
        return 0;
    }
    if ship_died_this_frame {
        return 0;
    }

    let decremented = if prev.ship_fire_cooldown > 0 {
        prev.ship_fire_cooldown - 1
    } else {
        prev.ship_fire_cooldown
    };
    let fire_pressed_this_frame = input.fire && !prev.ship_fire_latch;

    if !prev.ship_can_control {
        if next.ship_can_control {
            0
        } else {
            decremented
        }
    } else if fire_pressed_this_frame && decremented <= 0 && prev.bullets < SHIP_BULLET_LIMIT {
        SHIP_BULLET_COOLDOWN_FRAMES
    } else {
        decremented
    }
}

#[inline]
fn expected_ship_fire_latch(
    _prev: &TransitionState,
    input: FrameInput,
    wave_advanced_this_frame: bool,
    ship_died_this_frame: bool,
) -> bool {
    if wave_advanced_this_frame || ship_died_this_frame {
        return false;
    }

    if !input.fire {
        return false;
    }

    true
}

const fn build_legal_score_delta_table() -> [bool; LEGAL_SCORE_DELTA_TABLE_SIZE] {
    let mut table = [false; LEGAL_SCORE_DELTA_TABLE_SIZE];
    table[0] = true;

    let mut i = 0;
    while i < SCORE_EVENT_VALUES.len() {
        let a = SCORE_EVENT_VALUES[i];
        table[a as usize] = true;

        let mut j = 0;
        while j < SCORE_EVENT_VALUES.len() {
            let two = a + SCORE_EVENT_VALUES[j];
            if two <= MAX_SCORE_DELTA_PER_FRAME {
                table[two as usize] = true;
            }

            let mut k = 0;
            while k < SCORE_EVENT_VALUES.len() {
                let three = two + SCORE_EVENT_VALUES[k];
                if three <= MAX_SCORE_DELTA_PER_FRAME {
                    table[three as usize] = true;
                }

                let mut m = 0;
                while m < SCORE_EVENT_VALUES.len() {
                    let four = three + SCORE_EVENT_VALUES[m];
                    if four <= MAX_SCORE_DELTA_PER_FRAME {
                        table[four as usize] = true;
                    }
                    m += 1;
                }
                k += 1;
            }
            j += 1;
        }
        i += 1;
    }

    table
}

fn is_legal_score_delta(delta: u32) -> bool {
    if delta > MAX_SCORE_DELTA_PER_FRAME {
        return false;
    }

    LEGAL_SCORE_DELTAS[delta as usize]
}

pub fn replay_with_checkpoints(
    seed: u32,
    inputs: &[u8],
    sample_every: u32,
) -> Vec<ReplayCheckpoint> {
    let mut game = Game::new(seed);
    let stride = if sample_every == 0 { 1 } else { sample_every };
    let total_frames = inputs.len() as u32;
    let mut checkpoints = Vec::new();
    checkpoints.push(game.checkpoint());

    for (index, input) in inputs.iter().enumerate() {
        game.step(*input);
        let frame = (index + 1) as u32;
        if frame.is_multiple_of(stride) || frame == total_frames {
            checkpoints.push(game.checkpoint());
        }
    }

    checkpoints
}

impl LiveGame {
    pub fn new(seed: u32) -> Self {
        Self {
            game: Game::new(seed),
        }
    }

    #[inline]
    pub fn step(&mut self, input_byte: u8) {
        self.game.step(input_byte);
    }

    pub fn can_step_strict(&self, input_byte: u8) -> Result<(), RuleCode> {
        let before_step = self.game.transition_state();
        let mut next = self.game.clone();
        let frame_input = decode_input_byte(input_byte);
        next.step_decoded(frame_input);
        let after_step = next.transition_state();

        validate_transition(&before_step, &after_step, frame_input)?;
        next.validate_invariants()?;
        Ok(())
    }

    pub fn step_checked(&mut self, input_byte: u8) -> Result<(), RuleCode> {
        self.can_step_strict(input_byte)?;
        self.game.step(input_byte);
        Ok(())
    }

    #[inline]
    pub fn step_input(&mut self, input: FrameInput) {
        self.step(encode_input_byte(input));
    }

    #[inline]
    pub fn snapshot(&self) -> WorldSnapshot {
        self.game.world_snapshot()
    }

    #[inline]
    pub fn result(&self) -> ReplayResult {
        self.game.result()
    }

    #[inline]
    pub fn validate(&self) -> Result<(), RuleCode> {
        self.game.validate_invariants()
    }
}

#[inline]
fn max_saucers_for_wave(wave: i32) -> i32 {
    if wave < 4 {
        1
    } else if wave < 7 {
        2
    } else {
        3
    }
}

#[inline]
fn saucer_spawn_range_for_wave(wave: i32) -> (i32, i32) {
    let wave_mult_pct = core::cmp::max(40, 100 - (wave - 1) * 8);
    (
        (SAUCER_SPAWN_MIN_FRAMES * wave_mult_pct) / 100,
        (SAUCER_SPAWN_MAX_FRAMES * wave_mult_pct) / 100,
    )
}

#[inline]
fn saucer_wave_pressure_pct(wave: i32) -> i32 {
    clamp_i32((wave - 1) * 8, 0, 100)
}

#[inline]
fn saucer_lurk_pressure_pct(time_since_last_kill: i32) -> i32 {
    let over = core::cmp::max(0, time_since_last_kill - LURK_TIME_THRESHOLD_FRAMES);
    clamp_i32((over * 100) / (LURK_TIME_THRESHOLD_FRAMES * 2), 0, 100)
}

#[inline]
fn saucer_pressure_pct(wave: i32, time_since_last_kill: i32) -> i32 {
    let wave_pressure = saucer_wave_pressure_pct(wave);
    let lurk_pressure = saucer_lurk_pressure_pct(time_since_last_kill);
    core::cmp::min(100, wave_pressure + ((lurk_pressure * 50) / 100))
}

#[inline]
fn saucer_fire_cooldown_range(small: bool, wave: i32, time_since_last_kill: i32) -> (i32, i32) {
    let pressure = saucer_pressure_pct(wave, time_since_last_kill);

    let (base_min, base_max, floor_min, floor_max) = if small {
        (42, 68, 22, 40)
    } else {
        (66, 96, 36, 56)
    };

    let min = base_min - (((base_min - floor_min) * pressure) / 100);
    let max = base_max - (((base_max - floor_max) * pressure) / 100);
    if max > min {
        (min, max)
    } else {
        (min, min + 1)
    }
}

#[inline]
fn small_saucer_aim_error_bam(wave: i32, time_since_last_kill: i32) -> i32 {
    let pressure = saucer_pressure_pct(wave, time_since_last_kill);
    let base_error = 22;
    let min_error = 3;
    let range = base_error - min_error;
    clamp_i32(
        base_error - ((range * pressure) / 100),
        min_error,
        base_error,
    )
}
