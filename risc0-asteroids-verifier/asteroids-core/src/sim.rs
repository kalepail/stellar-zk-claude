use alloc::vec::Vec;

use crate::constants::{
    ASTEROID_CAP, ASTEROID_RADIUS_LARGE, ASTEROID_RADIUS_MEDIUM, ASTEROID_RADIUS_SMALL,
    ASTEROID_SPEED_LARGE_Q8_8, ASTEROID_SPEED_MEDIUM_Q8_8, ASTEROID_SPEED_SMALL_Q8_8,
    EXTRA_LIFE_SCORE_STEP, LURK_SAUCER_SPAWN_FAST_FRAMES, LURK_TIME_THRESHOLD_FRAMES,
    SAUCER_BULLET_LIFETIME_FRAMES, SAUCER_BULLET_SPEED_Q8_8, SAUCER_RADIUS_LARGE,
    SAUCER_RADIUS_SMALL, SAUCER_SPAWN_MAX_FRAMES, SAUCER_SPAWN_MIN_FRAMES, SAUCER_SPEED_LARGE_Q8_8,
    SAUCER_SPEED_SMALL_Q8_8, SCORE_LARGE_ASTEROID, SCORE_LARGE_SAUCER, SCORE_MEDIUM_ASTEROID,
    SCORE_SMALL_ASTEROID, SCORE_SMALL_SAUCER, SHIP_BULLET_COOLDOWN_FRAMES,
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
use crate::tape::decode_input_byte;

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
    radius: i32,
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

const SHIP_SPAWN_X_Q12_4: i32 = 7_680;
const SHIP_SPAWN_Y_Q12_4: i32 = 5_760;
const SHIP_RESPAWN_CLEAR_RADIUS_Q12_4: i32 = 1_920;
const WAVE_SAFE_DIST_Q12_4: i32 = 2_880;
const WAVE_SAFE_DIST_SQ_Q24_8: i32 = WAVE_SAFE_DIST_Q12_4 * WAVE_SAFE_DIST_Q12_4;

const SAUCER_START_X_LEFT_Q12_4: i32 = -480;
const SAUCER_START_X_RIGHT_Q12_4: i32 = 15_840;
const SAUCER_START_Y_MIN_Q12_4: i32 = 1_152;
const SAUCER_START_Y_MAX_Q12_4: i32 = 10_368;
const SAUCER_CULL_MIN_X_Q12_4: i32 = -1_280;
const SAUCER_CULL_MAX_X_Q12_4: i32 = 16_640;

const ASTEROID_VEC_CAPACITY: usize = ASTEROID_CAP + 16;
const SHIP_BULLET_VEC_CAPACITY: usize = SHIP_BULLET_LIMIT;
const SAUCER_VEC_CAPACITY: usize = 4;
const SAUCER_BULLET_VEC_CAPACITY: usize = 16;

pub fn replay(seed: u32, inputs: &[u8]) -> ReplayResult {
    let mut game = Game::new(seed);

    for input in inputs {
        game.step(*input);
    }

    ReplayResult {
        final_score: game.score,
        final_rng_state: game.rng.state(),
        frame_count: game.frame_count,
    }
}

pub fn replay_strict(seed: u32, inputs: &[u8]) -> Result<ReplayResult, ReplayViolation> {
    let mut game = Game::new(seed);
    game.validate_invariants().map_err(|rule| ReplayViolation {
        frame_count: game.frame_count,
        rule,
    })?;

    for input in inputs {
        game.step(*input);
        game.validate_invariants().map_err(|rule| ReplayViolation {
            frame_count: game.frame_count,
            rule,
        })?;
    }

    Ok(ReplayResult {
        final_score: game.score,
        final_rng_state: game.rng.state(),
        frame_count: game.frame_count,
    })
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
fn in_world_bounds_q12_4(x: i32, y: i32) -> bool {
    (0..WORLD_WIDTH_Q12_4).contains(&x) && (0..WORLD_HEIGHT_Q12_4).contains(&y)
}

struct Game {
    mode: GameMode,
    score: u32,
    lives: i32,
    wave: i32,
    next_extra_life_score: u32,
    ship: Ship,
    asteroids: Vec<Asteroid>,
    bullets: Vec<Bullet>,
    saucers: Vec<Saucer>,
    saucer_bullets: Vec<Bullet>,
    saucer_spawn_timer: i32,
    time_since_last_kill: i32,
    frame_count: u32,
    rng: SeededRng,
}

impl Game {
    fn new(seed: u32) -> Self {
        let mut game = Self {
            mode: GameMode::Playing,
            score: 0,
            lives: STARTING_LIVES,
            wave: 0,
            next_extra_life_score: EXTRA_LIFE_SCORE_STEP,
            ship: Ship {
                x: 0,
                y: 0,
                vx: 0,
                vy: 0,
                angle: 0,
                radius: SHIP_RADIUS,
                can_control: true,
                fire_cooldown: 0,
                respawn_timer: 0,
                invulnerable_timer: 0,
            },
            asteroids: Vec::with_capacity(ASTEROID_VEC_CAPACITY),
            bullets: Vec::with_capacity(SHIP_BULLET_VEC_CAPACITY),
            saucers: Vec::with_capacity(SAUCER_VEC_CAPACITY),
            saucer_bullets: Vec::with_capacity(SAUCER_BULLET_VEC_CAPACITY),
            saucer_spawn_timer: 0,
            time_since_last_kill: 0,
            frame_count: 0,
            rng: SeededRng::new(seed),
        };

        game.ship = game.create_ship();
        game.spawn_wave();

        let (spawn_min, spawn_max) = saucer_spawn_range_for_wave(game.wave);
        game.saucer_spawn_timer = game.random_int(spawn_min, spawn_max);

        game
    }

    fn checkpoint(&self) -> ReplayCheckpoint {
        ReplayCheckpoint {
            frame_count: self.frame_count,
            rng_state: self.rng.state(),
            score: self.score,
            lives: self.lives,
            wave: self.wave,
            asteroids: self.asteroids.len(),
            bullets: self.bullets.len(),
            saucers: self.saucers.len(),
            saucer_bullets: self.saucer_bullets.len(),
            ship_x: self.ship.x,
            ship_y: self.ship.y,
            ship_vx: self.ship.vx,
            ship_vy: self.ship.vy,
            ship_angle: self.ship.angle,
            ship_can_control: self.ship.can_control,
            ship_fire_cooldown: self.ship.fire_cooldown,
            ship_respawn_timer: self.ship.respawn_timer,
            ship_invulnerable_timer: self.ship.invulnerable_timer,
        }
    }

    fn random_int(&mut self, min: i32, max_exclusive: i32) -> i32 {
        self.rng.next_range(min, max_exclusive)
    }

    fn create_ship(&self) -> Ship {
        Ship {
            x: SHIP_SPAWN_X_Q12_4,
            y: SHIP_SPAWN_Y_Q12_4,
            vx: 0,
            vy: 0,
            angle: 192,
            radius: SHIP_RADIUS,
            can_control: true,
            fire_cooldown: 0,
            respawn_timer: 0,
            invulnerable_timer: SHIP_SPAWN_INVULNERABLE_FRAMES,
        }
    }

    fn step(&mut self, input_byte: u8) {
        self.frame_count += 1;

        let input = decode_input_byte(input_byte);

        self.update_ship(input.left, input.right, input.thrust, input.fire);
        self.update_asteroids();
        self.update_bullets();
        self.update_saucers();
        self.update_saucer_bullets();

        self.handle_collisions();
        self.prune_destroyed_entities();

        self.time_since_last_kill += 1;

        if matches!(self.mode, GameMode::Playing)
            && self.asteroids.is_empty()
            && self.saucers.is_empty()
        {
            self.spawn_wave();
        }
    }

    fn validate_invariants(&self) -> Result<(), RuleCode> {
        if self.wave < 1 {
            return Err(RuleCode::GlobalWaveNonZero);
        }

        let mode_lives_consistent = match self.mode {
            GameMode::Playing => self.lives > 0,
            GameMode::GameOver => self.lives <= 0,
        };
        if !mode_lives_consistent {
            return Err(RuleCode::GlobalModeLivesConsistency);
        }

        let next_extra_life_valid = self.next_extra_life_score > self.score
            && self.next_extra_life_score >= EXTRA_LIFE_SCORE_STEP
            && self
                .next_extra_life_score
                .is_multiple_of(EXTRA_LIFE_SCORE_STEP);
        if !next_extra_life_valid {
            return Err(RuleCode::GlobalNextExtraLifeScore);
        }

        if !in_world_bounds_q12_4(self.ship.x, self.ship.y) {
            return Err(RuleCode::ShipBounds);
        }

        if !(0..=255).contains(&self.ship.angle) {
            return Err(RuleCode::ShipAngleRange);
        }

        if self.ship.fire_cooldown < 0 {
            return Err(RuleCode::ShipCooldownRange);
        }

        if self.ship.respawn_timer < 0 {
            return Err(RuleCode::ShipRespawnTimerRange);
        }

        if self.ship.invulnerable_timer < 0 {
            return Err(RuleCode::ShipInvulnerabilityRange);
        }

        if self.bullets.len() > SHIP_BULLET_LIMIT {
            return Err(RuleCode::PlayerBulletLimit);
        }

        for bullet in &self.bullets {
            if !bullet.alive || bullet.life <= 0 || !in_world_bounds_q12_4(bullet.x, bullet.y) {
                return Err(RuleCode::PlayerBulletState);
            }
        }

        for bullet in &self.saucer_bullets {
            if !bullet.alive || bullet.life <= 0 || !in_world_bounds_q12_4(bullet.x, bullet.y) {
                return Err(RuleCode::SaucerBulletState);
            }
        }

        for asteroid in &self.asteroids {
            if !asteroid.alive
                || !in_world_bounds_q12_4(asteroid.x, asteroid.y)
                || !(0..=255).contains(&asteroid.angle)
            {
                return Err(RuleCode::AsteroidState);
            }
        }

        let max_saucers = max_saucers_for_wave(self.wave);
        if (self.saucers.len() as i32) > max_saucers {
            return Err(RuleCode::SaucerCap);
        }

        for saucer in &self.saucers {
            if !saucer.alive
                || saucer.x < SAUCER_CULL_MIN_X_Q12_4
                || saucer.x > SAUCER_CULL_MAX_X_Q12_4
                || !(0..WORLD_HEIGHT_Q12_4).contains(&saucer.y)
                || saucer.fire_cooldown < 0
                || saucer.drift_timer < 0
            {
                return Err(RuleCode::SaucerState);
            }
        }

        Ok(())
    }

    fn get_ship_spawn_point(&self) -> (i32, i32) {
        (SHIP_SPAWN_X_Q12_4, SHIP_SPAWN_Y_Q12_4)
    }

    fn queue_ship_respawn(&mut self, delay_frames: i32) {
        self.ship.can_control = false;
        self.ship.respawn_timer = delay_frames;
        self.ship.vx = 0;
        self.ship.vy = 0;
        self.ship.fire_cooldown = 0;
        self.ship.invulnerable_timer = 0;
    }

    fn is_ship_spawn_area_clear(
        &self,
        spawn_x: i32,
        spawn_y: i32,
        clear_radius_q12_4: i32,
    ) -> bool {
        let blocked_by_asteroid = self.asteroids.iter().any(|asteroid| {
            let hit_dist = (asteroid.radius << 4) + clear_radius_q12_4;
            collision_dist_sq_q12_4(asteroid.x, asteroid.y, spawn_x, spawn_y) < hit_dist * hit_dist
        });

        if blocked_by_asteroid {
            return false;
        }

        let blocked_by_saucer = self.saucers.iter().any(|saucer| {
            if !saucer.alive {
                return false;
            }
            let hit_dist = (saucer.radius << 4) + clear_radius_q12_4;
            collision_dist_sq_q12_4(saucer.x, saucer.y, spawn_x, spawn_y) < hit_dist * hit_dist
        });

        if blocked_by_saucer {
            return false;
        }

        !self.saucer_bullets.iter().any(|bullet| {
            if !bullet.alive {
                return false;
            }
            let hit_dist = (bullet.radius << 4) + clear_radius_q12_4;
            collision_dist_sq_q12_4(bullet.x, bullet.y, spawn_x, spawn_y) < hit_dist * hit_dist
        })
    }

    fn try_spawn_ship_at_center(&mut self) -> bool {
        let (spawn_x, spawn_y) = self.get_ship_spawn_point();

        if !self.is_ship_spawn_area_clear(spawn_x, spawn_y, SHIP_RESPAWN_CLEAR_RADIUS_Q12_4) {
            return false;
        }

        self.ship.x = spawn_x;
        self.ship.y = spawn_y;
        self.ship.vx = 0;
        self.ship.vy = 0;
        self.ship.angle = 192;
        self.ship.can_control = true;
        self.ship.invulnerable_timer = SHIP_SPAWN_INVULNERABLE_FRAMES;

        true
    }

    fn spawn_wave(&mut self) {
        self.wave += 1;
        self.time_since_last_kill = 0;

        let large_count = core::cmp::min(16, 4 + (self.wave - 1) * 2);
        let (avoid_x, avoid_y) = self.get_ship_spawn_point();

        for _ in 0..large_count {
            let mut x = self.random_int(0, WORLD_WIDTH_Q12_4);
            let mut y = self.random_int(0, WORLD_HEIGHT_Q12_4);
            let mut guard = 0;

            while collision_dist_sq_q12_4(x, y, avoid_x, avoid_y) < WAVE_SAFE_DIST_SQ_Q24_8
                && guard < 20
            {
                x = self.random_int(0, WORLD_WIDTH_Q12_4);
                y = self.random_int(0, WORLD_HEIGHT_Q12_4);
                guard += 1;
            }

            let asteroid = self.create_asteroid(AsteroidSize::Large, x, y);
            self.asteroids.push(asteroid);
        }

        self.queue_ship_respawn(0);
        self.try_spawn_ship_at_center();
    }

    fn create_asteroid(&mut self, size: AsteroidSize, x: i32, y: i32) -> Asteroid {
        let (min_q8_8, max_q8_8) = match size {
            AsteroidSize::Large => ASTEROID_SPEED_LARGE_Q8_8,
            AsteroidSize::Medium => ASTEROID_SPEED_MEDIUM_Q8_8,
            AsteroidSize::Small => ASTEROID_SPEED_SMALL_Q8_8,
        };

        let move_angle = self.random_int(0, 256);
        let mut speed = self.random_int(min_q8_8, max_q8_8);
        speed += (speed * core::cmp::min(128, (self.wave - 1) * 15)) >> 8;
        let (vx, vy) = velocity_q8_8(move_angle, speed);
        let start_angle = self.random_int(0, 256);
        let spin = self.random_int(-3, 4);

        let radius = match size {
            AsteroidSize::Large => ASTEROID_RADIUS_LARGE,
            AsteroidSize::Medium => ASTEROID_RADIUS_MEDIUM,
            AsteroidSize::Small => ASTEROID_RADIUS_SMALL,
        };

        Asteroid {
            x,
            y,
            vx,
            vy,
            angle: start_angle,
            alive: true,
            radius,
            size,
            spin,
        }
    }

    fn update_ship(&mut self, turn_left: bool, turn_right: bool, thrust: bool, fire: bool) {
        if self.ship.fire_cooldown > 0 {
            self.ship.fire_cooldown -= 1;
        }

        if !self.ship.can_control {
            if self.ship.respawn_timer > 0 {
                self.ship.respawn_timer -= 1;
            }

            if self.ship.respawn_timer <= 0 {
                self.try_spawn_ship_at_center();
            }

            return;
        }

        if self.ship.invulnerable_timer > 0 {
            self.ship.invulnerable_timer -= 1;
        }

        if turn_left {
            self.ship.angle = (self.ship.angle - SHIP_TURN_SPEED_BAM) & 0xff;
        }

        if turn_right {
            self.ship.angle = (self.ship.angle + SHIP_TURN_SPEED_BAM) & 0xff;
        }

        if thrust {
            let accel_vx = (cos_bam(self.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            let accel_vy = (sin_bam(self.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            self.ship.vx += accel_vx;
            self.ship.vy += accel_vy;
        }

        self.ship.vx = apply_drag(self.ship.vx);
        self.ship.vy = apply_drag(self.ship.vy);
        (self.ship.vx, self.ship.vy) =
            clamp_speed_q8_8(self.ship.vx, self.ship.vy, SHIP_MAX_SPEED_SQ_Q16_16);

        if fire && self.ship.fire_cooldown <= 0 && self.bullets.len() < SHIP_BULLET_LIMIT {
            self.spawn_ship_bullet();
            self.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
        }

        self.ship.x = wrap_x_q12_4(self.ship.x + (self.ship.vx >> 4));
        self.ship.y = wrap_y_q12_4(self.ship.y + (self.ship.vy >> 4));
    }

    fn spawn_ship_bullet(&mut self) {
        let (dx, dy) = displace_q12_4(self.ship.angle, self.ship.radius + 6);
        let start_x = wrap_x_q12_4(self.ship.x + dx);
        let start_y = wrap_y_q12_4(self.ship.y + dy);

        let ship_speed_approx = ((self.ship.vx.abs() + self.ship.vy.abs()) * 3) >> 2;
        let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
        let (bvx, bvy) = velocity_q8_8(self.ship.angle, bullet_speed_q8_8);

        self.bullets.push(Bullet {
            x: start_x,
            y: start_y,
            vx: self.ship.vx + bvx,
            vy: self.ship.vy + bvy,
            alive: true,
            radius: 2,
            life: SHIP_BULLET_LIFETIME_FRAMES,
        });
    }

    fn update_asteroids(&mut self) {
        for asteroid in &mut self.asteroids {
            if !asteroid.alive {
                continue;
            }

            asteroid.x = wrap_x_q12_4(asteroid.x + (asteroid.vx >> 4));
            asteroid.y = wrap_y_q12_4(asteroid.y + (asteroid.vy >> 4));
            asteroid.angle = (asteroid.angle + asteroid.spin) & 0xff;
        }
    }

    fn update_bullets(&mut self) {
        Self::update_projectiles(&mut self.bullets);
    }

    fn update_saucer_bullets(&mut self) {
        Self::update_projectiles(&mut self.saucer_bullets);
    }

    fn update_projectiles(projectiles: &mut [Bullet]) {
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

    fn update_saucers(&mut self) {
        if self.saucer_spawn_timer > 0 {
            self.saucer_spawn_timer -= 1;
        }

        let is_lurking = self.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let spawn_threshold = if is_lurking {
            LURK_SAUCER_SPAWN_FAST_FRAMES
        } else {
            0
        };
        let max_saucers = max_saucers_for_wave(self.wave);

        if (self.saucers.len() as i32) < max_saucers && self.saucer_spawn_timer <= spawn_threshold {
            self.spawn_saucer();
            let (spawn_min, spawn_max) = saucer_spawn_range_for_wave(self.wave);
            self.saucer_spawn_timer = if is_lurking {
                self.random_int(
                    LURK_SAUCER_SPAWN_FAST_FRAMES,
                    LURK_SAUCER_SPAWN_FAST_FRAMES + 120,
                )
            } else {
                self.random_int(spawn_min, spawn_max)
            };
        }

        for index in 0..self.saucers.len() {
            if !self.saucers[index].alive {
                continue;
            }

            {
                let saucer = &mut self.saucers[index];
                saucer.x += saucer.vx >> 4;
                saucer.y = wrap_y_q12_4(saucer.y + (saucer.vy >> 4));

                if saucer.x < SAUCER_CULL_MIN_X_Q12_4 || saucer.x > SAUCER_CULL_MAX_X_Q12_4 {
                    saucer.alive = false;
                    continue;
                }

                if saucer.drift_timer > 0 {
                    saucer.drift_timer -= 1;
                }
            }

            if !self.saucers[index].alive {
                continue;
            }

            if self.saucers[index].drift_timer <= 0 {
                self.saucers[index].drift_timer = self.random_int(48, 120);
                self.saucers[index].vy = self.random_int(-163, 164);
            }

            if self.saucers[index].fire_cooldown > 0 {
                self.saucers[index].fire_cooldown -= 1;
            }

            if self.saucers[index].fire_cooldown <= 0 {
                let saucer = self.saucers[index];
                self.spawn_saucer_bullet(saucer);
                self.saucers[index].fire_cooldown = if saucer.small {
                    if is_lurking {
                        self.random_int(27, 46)
                    } else {
                        self.random_int(39, 66)
                    }
                } else if is_lurking {
                    self.random_int(46, 67)
                } else {
                    self.random_int(66, 96)
                };
            }
        }
    }

    fn spawn_saucer(&mut self) {
        let enter_from_left = self.rng.next().is_multiple_of(2);
        let is_lurking = self.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let small_pct = if is_lurking {
            90
        } else if self.score > 4_000 {
            70
        } else {
            22
        };
        let small = (self.rng.next() % 100) < small_pct;
        let speed_q8_8 = if small {
            SAUCER_SPEED_SMALL_Q8_8
        } else {
            SAUCER_SPEED_LARGE_Q8_8
        };

        let start_x = if enter_from_left {
            SAUCER_START_X_LEFT_Q12_4
        } else {
            SAUCER_START_X_RIGHT_Q12_4
        };
        let start_y = self.random_int(SAUCER_START_Y_MIN_Q12_4, SAUCER_START_Y_MAX_Q12_4);
        let vy = self.random_int(-94, 95);
        let fire_cooldown = self.random_int(18, 48);
        let drift_timer = self.random_int(48, 120);

        self.saucers.push(Saucer {
            x: start_x,
            y: start_y,
            vx: if enter_from_left {
                speed_q8_8
            } else {
                -speed_q8_8
            },
            vy,
            alive: true,
            radius: if small {
                SAUCER_RADIUS_SMALL
            } else {
                SAUCER_RADIUS_LARGE
            },
            small,
            fire_cooldown,
            drift_timer,
        });
    }

    fn spawn_saucer_bullet(&mut self, saucer: Saucer) {
        let shot_angle = if saucer.small {
            let dx = shortest_delta_q12_4(saucer.x, self.ship.x, WORLD_WIDTH_Q12_4);
            let dy = shortest_delta_q12_4(saucer.y, self.ship.y, WORLD_HEIGHT_Q12_4);
            let target_angle = atan2_bam(dy, dx);
            let is_lurking = self.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
            let base_error_bam = if is_lurking { 11 } else { 21 };
            let score_bonus = (self.score / 2_500) as i32;
            let wave_bonus = core::cmp::min(11, self.wave);
            let error_bam = clamp_i32(base_error_bam - score_bonus - wave_bonus, 3, base_error_bam);
            (target_angle + self.random_int(-error_bam, error_bam + 1)) & 0xff
        } else {
            self.random_int(0, 256)
        };

        let (vx, vy) = velocity_q8_8(shot_angle, SAUCER_BULLET_SPEED_Q8_8);
        let (off_dx, off_dy) = displace_q12_4(shot_angle, saucer.radius + 4);
        let start_x = wrap_x_q12_4(saucer.x + off_dx);
        let start_y = wrap_y_q12_4(saucer.y + off_dy);

        self.saucer_bullets.push(Bullet {
            x: start_x,
            y: start_y,
            vx,
            vy,
            alive: true,
            radius: 2,
            life: SAUCER_BULLET_LIFETIME_FRAMES,
        });
    }

    fn handle_collisions(&mut self) {
        for bullet_index in 0..self.bullets.len() {
            if !self.bullets[bullet_index].alive {
                continue;
            }

            let (bx, by, br) = {
                let bullet = self.bullets[bullet_index];
                (bullet.x, bullet.y, bullet.radius)
            };

            for asteroid_index in 0..self.asteroids.len() {
                if !self.asteroids[asteroid_index].alive {
                    continue;
                }

                let asteroid = self.asteroids[asteroid_index];
                let hit_dist_q12_4 = (br + asteroid.radius) << 4;
                if collision_dist_sq_q12_4(bx, by, asteroid.x, asteroid.y)
                    <= hit_dist_q12_4 * hit_dist_q12_4
                {
                    self.bullets[bullet_index].alive = false;
                    self.destroy_asteroid(asteroid_index, true);
                    break;
                }
            }
        }

        for bullet_index in 0..self.saucer_bullets.len() {
            if !self.saucer_bullets[bullet_index].alive {
                continue;
            }

            let (bx, by, br) = {
                let bullet = self.saucer_bullets[bullet_index];
                (bullet.x, bullet.y, bullet.radius)
            };

            for asteroid_index in 0..self.asteroids.len() {
                if !self.asteroids[asteroid_index].alive {
                    continue;
                }

                let asteroid = self.asteroids[asteroid_index];
                let hit_dist_q12_4 = (br + asteroid.radius) << 4;
                if collision_dist_sq_q12_4(bx, by, asteroid.x, asteroid.y)
                    <= hit_dist_q12_4 * hit_dist_q12_4
                {
                    self.saucer_bullets[bullet_index].alive = false;
                    self.destroy_asteroid(asteroid_index, false);
                    break;
                }
            }
        }

        for bullet_index in 0..self.bullets.len() {
            if !self.bullets[bullet_index].alive {
                continue;
            }

            let (bx, by, br) = {
                let bullet = self.bullets[bullet_index];
                (bullet.x, bullet.y, bullet.radius)
            };

            for saucer_index in 0..self.saucers.len() {
                if !self.saucers[saucer_index].alive {
                    continue;
                }

                let saucer = self.saucers[saucer_index];
                let hit_dist_q12_4 = (br + saucer.radius) << 4;
                if collision_dist_sq_q12_4(bx, by, saucer.x, saucer.y)
                    <= hit_dist_q12_4 * hit_dist_q12_4
                {
                    self.bullets[bullet_index].alive = false;
                    self.saucers[saucer_index].alive = false;
                    self.add_score(if saucer.small {
                        SCORE_SMALL_SAUCER
                    } else {
                        SCORE_LARGE_SAUCER
                    });
                    break;
                }
            }
        }

        if !self.ship.can_control || self.ship.invulnerable_timer > 0 {
            return;
        }

        for asteroid in &self.asteroids {
            if !asteroid.alive {
                continue;
            }

            let adjusted_radius = (asteroid.radius * 225) >> 8;
            let hit_dist_q12_4 = (self.ship.radius + adjusted_radius) << 4;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, asteroid.x, asteroid.y)
                <= hit_dist_q12_4 * hit_dist_q12_4
            {
                self.destroy_ship();
                return;
            }
        }

        for bullet in &mut self.saucer_bullets {
            if !bullet.alive {
                continue;
            }

            let hit_dist_q12_4 = (self.ship.radius + bullet.radius) << 4;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, bullet.x, bullet.y)
                <= hit_dist_q12_4 * hit_dist_q12_4
            {
                bullet.alive = false;
                self.destroy_ship();
                return;
            }
        }

        for saucer in &mut self.saucers {
            if !saucer.alive {
                continue;
            }

            let hit_dist_q12_4 = (self.ship.radius + saucer.radius) << 4;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, saucer.x, saucer.y)
                <= hit_dist_q12_4 * hit_dist_q12_4
            {
                saucer.alive = false;
                self.destroy_ship();
                return;
            }
        }
    }

    fn destroy_asteroid(&mut self, asteroid_index: usize, award_score: bool) {
        if asteroid_index >= self.asteroids.len() {
            return;
        }

        let (size, x, y, vx, vy) = {
            let asteroid = &mut self.asteroids[asteroid_index];
            if !asteroid.alive {
                return;
            }
            asteroid.alive = false;
            (
                asteroid.size,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
            )
        };

        if award_score {
            self.time_since_last_kill = 0;
            match size {
                AsteroidSize::Large => self.add_score(SCORE_LARGE_ASTEROID),
                AsteroidSize::Medium => self.add_score(SCORE_MEDIUM_ASTEROID),
                AsteroidSize::Small => self.add_score(SCORE_SMALL_ASTEROID),
            }
        }

        if matches!(size, AsteroidSize::Small) {
            return;
        }

        let child_size = if matches!(size, AsteroidSize::Large) {
            AsteroidSize::Medium
        } else {
            AsteroidSize::Small
        };

        let total_objects = self.asteroids.iter().filter(|entry| entry.alive).count();
        let split_count = if total_objects >= ASTEROID_CAP { 1 } else { 2 };

        for _ in 0..split_count {
            let mut child = self.create_asteroid(child_size, x, y);
            child.vx += (vx * 46) >> 8;
            child.vy += (vy * 46) >> 8;
            self.asteroids.push(child);
        }
    }

    fn destroy_ship(&mut self) {
        self.queue_ship_respawn(SHIP_RESPAWN_FRAMES);
        self.lives -= 1;

        if self.lives <= 0 {
            self.mode = GameMode::GameOver;
            self.ship.can_control = false;
            self.ship.respawn_timer = 99_999;
        }
    }

    fn add_score(&mut self, points: u32) {
        self.score = self.score.saturating_add(points);

        while self.score >= self.next_extra_life_score {
            self.lives += 1;
            self.next_extra_life_score += EXTRA_LIFE_SCORE_STEP;
        }
    }

    fn prune_destroyed_entities(&mut self) {
        self.asteroids.retain(|entry| entry.alive);
        self.bullets.retain(|entry| entry.alive);
        self.saucers.retain(|entry| entry.alive);
        self.saucer_bullets.retain(|entry| entry.alive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_inputs_are_deterministic() {
        let inputs = [0x00u8, 0x01, 0x04, 0x0C, 0x00, 0x08, 0x02, 0x00];
        let a = replay(0x1234_5678, &inputs);
        let b = replay(0x1234_5678, &inputs);
        assert_eq!(a, b);
    }
}
