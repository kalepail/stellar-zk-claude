//! Deterministic Asteroids game engine for ZK verification.
//!
//! This is a headless, integer-only re-implementation of AsteroidsGame.ts.
//! It replays a tape of inputs and produces the same final score + RNG state
//! as the TypeScript engine, bit-for-bit.

extern crate alloc;
use alloc::vec::Vec;

use crate::constants::*;
use crate::fixed_point::*;
use crate::rng::SeededRng;
use crate::types::*;

/// Wrapping helpers for Q12.4 coordinates.
#[inline]
fn wrap_x_q12_4(x: i32) -> i32 {
    if x < 0 {
        x + WORLD_WIDTH_Q12_4
    } else if x >= WORLD_WIDTH_Q12_4 {
        x - WORLD_WIDTH_Q12_4
    } else {
        x
    }
}

#[inline]
fn wrap_y_q12_4(y: i32) -> i32 {
    if y < 0 {
        y + WORLD_HEIGHT_Q12_4
    } else if y >= WORLD_HEIGHT_Q12_4 {
        y - WORLD_HEIGHT_Q12_4
    } else {
        y
    }
}

/// Shortest delta accounting for toroidal wrapping.
#[inline]
fn shortest_delta_q12_4(from: i32, to: i32, size: i32) -> i32 {
    let mut delta = to - from;
    let half = size >> 1;
    if delta > half {
        delta -= size;
    }
    if delta < -half {
        delta += size;
    }
    delta
}

/// Distance-squared between two Q12.4 points with wrapping.
#[inline]
fn collision_dist_sq_q12_4(ax: i32, ay: i32, bx: i32, by: i32) -> i64 {
    let dx = shortest_delta_q12_4(ax, bx, WORLD_WIDTH_Q12_4) as i64;
    let dy = shortest_delta_q12_4(ay, by, WORLD_HEIGHT_Q12_4) as i64;
    dx * dx + dy * dy
}

/// Convert pixel value to Q12.4
#[inline]
fn to_q12_4(pixels: i32) -> i32 {
    pixels * 16 // equivalent to Math.round(v * 16) for integers
}

/// Clamp value to [min, max].
#[inline]
fn clamp(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// The deterministic game state, suitable for ZK verification.
pub struct AsteroidsGame {
    // RNG
    rng: SeededRng,

    // Game state
    score: u32,
    lives: i32,
    wave: i32,
    frame_count: u32,
    next_extra_life_score: u32,
    time_since_last_kill: i32,

    // Entities
    ship: Ship,
    asteroids: Vec<Asteroid>,
    bullets: Vec<Bullet>,
    saucers: Vec<Saucer>,
    saucer_bullets: Vec<Bullet>,

    // Saucer spawn timer
    saucer_spawn_timer: i32,

    // Current frame input
    current_input: FrameInput,

    // Game mode
    game_over: bool,
}

impl AsteroidsGame {
    /// Create a new game with the given seed and initialize all state.
    pub fn new(seed: u32) -> Self {
        let rng = SeededRng::new(seed);

        let ship_x = to_q12_4(WORLD_WIDTH / 2);  // 7680
        let ship_y = to_q12_4(WORLD_HEIGHT / 2); // 5760

        let ship = Ship {
            x: ship_x,
            y: ship_y,
            vx: 0,
            vy: 0,
            angle: SHIP_FACING_UP_BAM,
            radius: SHIP_RADIUS,
            can_control: true,
            fire_cooldown: 0,
            respawn_timer: 0,
            invulnerable_timer: SHIP_SPAWN_INVULNERABLE_FRAMES,
        };

        let mut game = Self {
            rng,
            score: 0,
            lives: STARTING_LIVES,
            wave: 0,
            frame_count: 0,
            next_extra_life_score: EXTRA_LIFE_SCORE_STEP,
            time_since_last_kill: 0,
            ship,
            asteroids: Vec::with_capacity(ASTEROID_CAP + 2),
            bullets: Vec::with_capacity(SHIP_BULLET_LIMIT),
            saucers: Vec::with_capacity(3),
            saucer_bullets: Vec::with_capacity(12),
            saucer_spawn_timer: 0,
            current_input: FrameInput::default(),
            game_over: false,
        };

        // Spawn first wave (matches startNewGame -> spawnWave)
        game.spawn_wave();

        // Set initial saucer spawn timer (matches startNewGame after spawnWave)
        let wave_mult_pct = (100 - (game.wave - 1) * 8).max(40);
        let spawn_min = (SAUCER_SPAWN_MIN_FRAMES * wave_mult_pct) / 100;
        let spawn_max = (SAUCER_SPAWN_MAX_FRAMES * wave_mult_pct) / 100;
        game.saucer_spawn_timer = game.rng.next_range(spawn_min, spawn_max);

        game
    }

    /// Run one simulation frame with the given input.
    /// This matches `stepSimulation()` in TypeScript.
    pub fn step(&mut self, input: FrameInput) {
        // storePreviousPositions is visual-only; skip in verifier
        self.update_simulation(input);
    }

    /// Get current score.
    pub fn score(&self) -> u32 {
        self.score
    }

    /// Get current RNG state.
    pub fn rng_state(&self) -> u32 {
        self.rng.get_state()
    }

    /// Get current lives.
    pub fn lives(&self) -> i32 {
        self.lives
    }

    /// Get current wave.
    pub fn wave(&self) -> i32 {
        self.wave
    }

    /// Get current frame count.
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// Is the game over?
    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    // ========================================================================
    // Core simulation - exact match to updateSimulation() in TypeScript
    // ========================================================================

    fn update_simulation(&mut self, input: FrameInput) {
        self.frame_count += 1;
        self.current_input = input;

        self.update_ship();
        self.update_asteroids();
        self.update_bullets();
        self.update_saucers();
        self.update_saucer_bullets();

        // Headless: skip particles, debris, screen shake

        self.handle_collisions();
        self.prune_destroyed_entities();

        // Anti-lurking timer
        self.time_since_last_kill += 1;

        // Check wave advancement
        if !self.game_over
            && self.asteroids.is_empty()
            && self.saucers.is_empty()
        {
            self.spawn_wave();
        }
    }

    // ========================================================================
    // Ship
    // ========================================================================

    fn update_ship(&mut self) {
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

        let input = self.current_input;

        if input.left {
            self.ship.angle = self.ship.angle.wrapping_sub(SHIP_TURN_SPEED_BAM as u8);
        }
        if input.right {
            self.ship.angle = self.ship.angle.wrapping_add(SHIP_TURN_SPEED_BAM as u8);
        }

        if input.thrust {
            let accel_vx = (cos_bam(self.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            let accel_vy = (sin_bam(self.ship.angle) * SHIP_THRUST_Q8_8) >> 14;
            self.ship.vx += accel_vx;
            self.ship.vy += accel_vy;
            // Thrust particles are visual-only, skipped
        }

        self.ship.vx = apply_drag(self.ship.vx);
        self.ship.vy = apply_drag(self.ship.vy);

        let (vx, vy) = clamp_speed_q8_8(self.ship.vx, self.ship.vy, SHIP_MAX_SPEED_SQ_Q16_16);
        self.ship.vx = vx;
        self.ship.vy = vy;

        if input.fire && self.ship.fire_cooldown <= 0 && self.bullets.len() < SHIP_BULLET_LIMIT {
            self.spawn_ship_bullet();
            self.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
        }

        // Q8.8 velocity >> 4 -> Q12.4 displacement
        self.ship.x = wrap_x_q12_4(self.ship.x + (self.ship.vx >> 4));
        self.ship.y = wrap_y_q12_4(self.ship.y + (self.ship.vy >> 4));
    }

    fn spawn_ship_bullet(&mut self) {
        let ship = &self.ship;
        let (dx, dy) = displace_q12_4(ship.angle, ship.radius + 6);
        let start_x = wrap_x_q12_4(ship.x + dx);
        let start_y = wrap_y_q12_4(ship.y + dy);

        // Bullet speed = base + ship speed boost
        let ship_speed_approx = ((ship.vx.abs() + ship.vy.abs()) * 3) >> 2;
        let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
        let (bvx, bvy) = velocity_q8_8(ship.angle, bullet_speed_q8_8);

        let bullet = Bullet {
            x: start_x,
            y: start_y,
            vx: ship.vx + bvx,
            vy: ship.vy + bvy,
            angle: ship.angle,
            alive: true,
            radius: 2,
            life: SHIP_BULLET_LIFETIME_FRAMES,
        };

        self.bullets.push(bullet);
        // Audio is visual-only, skipped
    }

    fn get_ship_spawn_point(&self) -> (i32, i32) {
        (to_q12_4(WORLD_WIDTH / 2), to_q12_4(WORLD_HEIGHT / 2))
    }

    fn queue_ship_respawn(&mut self, delay_frames: i32) {
        self.ship.can_control = false;
        self.ship.respawn_timer = delay_frames;
        self.ship.vx = 0;
        self.ship.vy = 0;
        self.ship.fire_cooldown = 0;
        self.ship.invulnerable_timer = 0;
    }

    fn is_ship_spawn_area_clear(&self, spawn_x: i32, spawn_y: i32) -> bool {
        let clear_radius_q12_4: i64 = 1920; // 120px * 16

        for asteroid in &self.asteroids {
            let hit_dist = (asteroid.radius as i64) * 16 + clear_radius_q12_4;
            if collision_dist_sq_q12_4(asteroid.x, asteroid.y, spawn_x, spawn_y) < hit_dist * hit_dist {
                return false;
            }
        }

        for saucer in &self.saucers {
            if !saucer.alive {
                continue;
            }
            let hit_dist = (saucer.radius as i64) * 16 + clear_radius_q12_4;
            if collision_dist_sq_q12_4(saucer.x, saucer.y, spawn_x, spawn_y) < hit_dist * hit_dist {
                return false;
            }
        }

        for bullet in &self.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            let hit_dist = (bullet.radius as i64) * 16 + clear_radius_q12_4;
            if collision_dist_sq_q12_4(bullet.x, bullet.y, spawn_x, spawn_y) < hit_dist * hit_dist {
                return false;
            }
        }

        true
    }

    fn try_spawn_ship_at_center(&mut self) -> bool {
        let (spawn_x, spawn_y) = self.get_ship_spawn_point();

        if !self.is_ship_spawn_area_clear(spawn_x, spawn_y) {
            return false;
        }

        self.ship.x = spawn_x;
        self.ship.y = spawn_y;
        self.ship.vx = 0;
        self.ship.vy = 0;
        self.ship.angle = SHIP_FACING_UP_BAM;
        self.ship.can_control = true;
        self.ship.invulnerable_timer = SHIP_SPAWN_INVULNERABLE_FRAMES;
        true
    }

    // ========================================================================
    // Wave spawning
    // ========================================================================

    fn spawn_wave(&mut self) {
        self.wave += 1;
        self.time_since_last_kill = 0;

        let large_count = 16i32.min(4 + (self.wave - 1) * 2);
        let (avoid_x, avoid_y) = self.get_ship_spawn_point();
        // 180px in Q12.4 = 2880; squared = 8,294,400
        let safe_dist_sq: i64 = 2880 * 2880;

        for _ in 0..large_count {
            let mut x = self.rng.next_range(0, WORLD_WIDTH_Q12_4);
            let mut y = self.rng.next_range(0, WORLD_HEIGHT_Q12_4);

            let mut guard = 0;
            while collision_dist_sq_q12_4(x, y, avoid_x, avoid_y) < safe_dist_sq && guard < 20 {
                x = self.rng.next_range(0, WORLD_WIDTH_Q12_4);
                y = self.rng.next_range(0, WORLD_HEIGHT_Q12_4);
                guard += 1;
            }

            let asteroid = self.create_asteroid(AsteroidSize::Large, x, y);
            self.asteroids.push(asteroid);
        }

        // Use same spawn policy as death-respawn
        self.queue_ship_respawn(0);
        self.try_spawn_ship_at_center();
    }

    fn create_asteroid(&mut self, size: AsteroidSize, x: i32, y: i32) -> Asteroid {
        let (min_q8_8, max_q8_8) = asteroid_speed_range(size);
        let move_angle = self.rng.next_range(0, 256) as u8;
        let mut speed = self.rng.next_range(min_q8_8, max_q8_8);

        // Wave speed multiplier: speed + speed * min(128, (wave-1)*15) >> 8
        let wave_bonus = 128i32.min((self.wave - 1) * 15);
        speed = speed + ((speed * wave_bonus) >> 8);

        let (vx, vy) = velocity_q8_8(move_angle, speed);

        // Vertices are visual-only, but the visualRng calls consume visual RNG state.
        // The visual RNG is a SEPARATE stream, so we do NOT call it here.
        // (TypeScript: createAsteroidVertices uses visualRandomInt/visualRandomRange)

        let start_angle = self.rng.next_range(0, 256);
        let spin = self.rng.next_range(-3, 4); // +-3 BAM/frame

        Asteroid {
            x,
            y,
            vx,
            vy,
            angle: start_angle,
            alive: true,
            radius: asteroid_radius(size),
            size,
            spin,
        }
    }

    // ========================================================================
    // Asteroids
    // ========================================================================

    fn update_asteroids(&mut self) {
        for asteroid in &mut self.asteroids {
            if !asteroid.alive {
                continue;
            }
            asteroid.x = wrap_x_q12_4(asteroid.x + (asteroid.vx >> 4));
            asteroid.y = wrap_y_q12_4(asteroid.y + (asteroid.vy >> 4));
            asteroid.angle = (asteroid.angle + asteroid.spin) & 0xFF;
        }
    }

    // ========================================================================
    // Bullets
    // ========================================================================

    fn update_bullets(&mut self) {
        for bullet in &mut self.bullets {
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

    fn update_saucer_bullets(&mut self) {
        for bullet in &mut self.saucer_bullets {
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

    // ========================================================================
    // Saucers
    // ========================================================================

    fn update_saucers(&mut self) {
        if self.saucer_spawn_timer > 0 {
            self.saucer_spawn_timer -= 1;
        }

        let is_lurking = self.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let spawn_threshold = if is_lurking { LURK_SAUCER_SPAWN_FAST_FRAMES } else { 0 };

        let max_saucers: usize = if self.wave < 4 {
            1
        } else if self.wave < 7 {
            2
        } else {
            3
        };

        if self.saucers.len() < max_saucers && self.saucer_spawn_timer <= spawn_threshold {
            self.spawn_saucer();

            let wave_mult_pct = (100 - (self.wave - 1) * 8).max(40);
            let spawn_min = (SAUCER_SPAWN_MIN_FRAMES * wave_mult_pct) / 100;
            let spawn_max = (SAUCER_SPAWN_MAX_FRAMES * wave_mult_pct) / 100;

            self.saucer_spawn_timer = if is_lurking {
                self.rng.next_range(LURK_SAUCER_SPAWN_FAST_FRAMES, LURK_SAUCER_SPAWN_FAST_FRAMES + 120)
            } else {
                self.rng.next_range(spawn_min, spawn_max)
            };
        }

        // Update existing saucers
        let ship_x = self.ship.x;
        let ship_y = self.ship.y;
        let score = self.score;
        let wave = self.wave;
        let time_since_last_kill = self.time_since_last_kill;

        // We need to collect saucer bullets to spawn, because we can't borrow self mutably
        // while iterating saucers. Collect spawn requests.
        let mut bullets_to_spawn: Vec<Bullet> = Vec::with_capacity(3);

        for saucer in &mut self.saucers {
            if !saucer.alive {
                continue;
            }

            // Saucer doesn't wrap X (exits screen)
            saucer.x = saucer.x + (saucer.vx >> 4);
            saucer.y = wrap_y_q12_4(saucer.y + (saucer.vy >> 4));

            // Off-screen check in Q12.4
            if saucer.x < to_q12_4(-80) || saucer.x > to_q12_4(WORLD_WIDTH + 80) {
                saucer.alive = false;
                continue;
            }

            if saucer.drift_timer > 0 {
                saucer.drift_timer -= 1;
            }
            if saucer.drift_timer <= 0 {
                saucer.drift_timer = self.rng.next_range(48, 120);
                saucer.vy = self.rng.next_range(-163, 164);
            }

            if saucer.fire_cooldown > 0 {
                saucer.fire_cooldown -= 1;
            }

            if saucer.fire_cooldown <= 0 {
                // Spawn saucer bullet
                let bullet = Self::create_saucer_bullet(
                    &mut self.rng,
                    saucer,
                    ship_x,
                    ship_y,
                    score,
                    wave,
                    time_since_last_kill,
                );
                bullets_to_spawn.push(bullet);

                let is_lurking = time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
                saucer.fire_cooldown = if saucer.small {
                    if is_lurking {
                        self.rng.next_range(27, 46)
                    } else {
                        self.rng.next_range(39, 66)
                    }
                } else {
                    if is_lurking {
                        self.rng.next_range(46, 67)
                    } else {
                        self.rng.next_range(66, 96)
                    }
                };
            }
        }

        self.saucer_bullets.extend(bullets_to_spawn);
    }

    fn spawn_saucer(&mut self) {
        let enter_from_left = self.rng.next() % 2 == 0;

        let is_lurking = self.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let small_pct = if is_lurking {
            90
        } else if self.score > 4000 {
            70
        } else {
            22
        };
        let small = self.rng.next() % 100 < small_pct;
        let speed_q8_8 = if small { SAUCER_SPEED_SMALL_Q8_8 } else { SAUCER_SPEED_LARGE_Q8_8 };

        let start_x = to_q12_4(if enter_from_left { -30 } else { WORLD_WIDTH + 30 });
        let start_y = self.rng.next_range(to_q12_4(72), to_q12_4(WORLD_HEIGHT - 72));

        let saucer = Saucer {
            x: start_x,
            y: start_y,
            vx: if enter_from_left { speed_q8_8 } else { -speed_q8_8 },
            vy: self.rng.next_range(-94, 95),
            alive: true,
            radius: if small { SAUCER_RADIUS_SMALL } else { SAUCER_RADIUS_LARGE },
            small,
            fire_cooldown: self.rng.next_range(18, 48),
            drift_timer: self.rng.next_range(48, 120),
        };

        self.saucers.push(saucer);
    }

    /// Create a saucer bullet. This is a static-ish method to avoid borrow conflicts.
    fn create_saucer_bullet(
        rng: &mut SeededRng,
        saucer: &Saucer,
        ship_x: i32,
        ship_y: i32,
        score: u32,
        wave: i32,
        time_since_last_kill: i32,
    ) -> Bullet {
        let shot_angle: u8;

        if saucer.small {
            // Aimed shot
            let dx = shortest_delta_q12_4(saucer.x, ship_x, WORLD_WIDTH_Q12_4);
            let dy = shortest_delta_q12_4(saucer.y, ship_y, WORLD_HEIGHT_Q12_4);
            let target_angle = atan2_bam(dy, dx);

            let is_lurking = time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
            let base_error_bam: i32 = if is_lurking { 11 } else { 21 };
            let score_bonus = (score / 2500) as i32;
            let wave_bonus = 11i32.min(wave * 1);
            let error_bam = clamp(base_error_bam - score_bonus - wave_bonus, 3, base_error_bam);
            shot_angle = ((target_angle as i32 + rng.next_range(-error_bam, error_bam + 1)) & 0xFF) as u8;
        } else {
            // Random shot
            shot_angle = rng.next_range(0, 256) as u8;
        }

        let (vx, vy) = velocity_q8_8(shot_angle, SAUCER_BULLET_SPEED_Q8_8);
        let (off_dx, off_dy) = displace_q12_4(shot_angle, saucer.radius + 4);
        let start_x = wrap_x_q12_4(saucer.x + off_dx);
        let start_y = wrap_y_q12_4(saucer.y + off_dy);

        Bullet {
            x: start_x,
            y: start_y,
            vx,
            vy,
            angle: shot_angle,
            alive: true,
            radius: 2,
            life: SAUCER_BULLET_LIFETIME_FRAMES,
        }
    }

    // ========================================================================
    // Collisions
    // ========================================================================

    fn handle_collisions(&mut self) {
        // We need to match TypeScript's exact collision order:
        // 1. Bullet-asteroid (player bullets, award score)
        // 2. Saucer bullet-asteroid (no score)
        // 3. Player bullet-saucer (award score)
        // 4. Ship-asteroid (if ship controllable and not invulnerable)
        // 5. Ship-saucer bullet
        // 6. Ship-saucer

        // Collect asteroid destruction events: (index, award_score)
        let mut destroyed_asteroids: Vec<(usize, bool)> = Vec::with_capacity(8);
        let mut destroyed_saucers: Vec<usize> = Vec::with_capacity(3);

        // 1. Player bullet-asteroid
        for bullet in &mut self.bullets {
            if !bullet.alive {
                continue;
            }
            for (i, asteroid) in self.asteroids.iter().enumerate() {
                if !asteroid.alive {
                    continue;
                }
                let hit_dist = ((bullet.radius + asteroid.radius) as i64) * 16;
                if collision_dist_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y)
                    <= hit_dist * hit_dist
                {
                    bullet.alive = false;
                    destroyed_asteroids.push((i, true));
                    break;
                }
            }
        }

        // Mark destroyed asteroids
        for &(i, _) in &destroyed_asteroids {
            self.asteroids[i].alive = false;
        }

        // 2. Saucer bullet-asteroid
        let mut saucer_destroyed_asteroids: Vec<(usize, bool)> = Vec::with_capacity(8);
        for bullet in &mut self.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            for (i, asteroid) in self.asteroids.iter().enumerate() {
                if !asteroid.alive {
                    continue;
                }
                let hit_dist = ((bullet.radius + asteroid.radius) as i64) * 16;
                if collision_dist_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y)
                    <= hit_dist * hit_dist
                {
                    bullet.alive = false;
                    saucer_destroyed_asteroids.push((i, false));
                    break;
                }
            }
        }

        for &(i, _) in &saucer_destroyed_asteroids {
            self.asteroids[i].alive = false;
        }
        destroyed_asteroids.extend(saucer_destroyed_asteroids);

        // 3. Player bullet-saucer
        for bullet in &mut self.bullets {
            if !bullet.alive {
                continue;
            }
            for (i, saucer) in self.saucers.iter().enumerate() {
                if !saucer.alive {
                    continue;
                }
                let hit_dist = ((bullet.radius + saucer.radius) as i64) * 16;
                if collision_dist_sq_q12_4(bullet.x, bullet.y, saucer.x, saucer.y)
                    <= hit_dist * hit_dist
                {
                    bullet.alive = false;
                    destroyed_saucers.push(i);
                    break;
                }
            }
        }

        // Mark destroyed saucers and award score
        for &i in &destroyed_saucers {
            let saucer = &self.saucers[i];
            let points = if saucer.small { SCORE_SMALL_SAUCER } else { SCORE_LARGE_SAUCER };
            self.saucers[i].alive = false;
            self.add_score(points);
            // Explosion effects are visual-only
        }

        // Process destroyed asteroids (scoring + splitting)
        // Sort by index descending so we can safely push children
        // Actually, TS processes them in iteration order. We should match that.
        for (i, award_score) in destroyed_asteroids {
            self.destroy_asteroid_at(i, award_score);
        }

        // 4-6. Ship collisions
        if !self.ship.can_control || self.ship.invulnerable_timer > 0 {
            return;
        }

        // Ship-asteroid
        for asteroid in &self.asteroids {
            if !asteroid.alive {
                continue;
            }
            let adjusted_radius = (asteroid.radius * 225) >> 8; // 0.88 fudge
            let hit_dist = ((self.ship.radius + adjusted_radius) as i64) * 16;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, asteroid.x, asteroid.y)
                <= hit_dist * hit_dist
            {
                self.destroy_ship();
                return;
            }
        }

        // Ship-saucer bullet
        for bullet in &mut self.saucer_bullets {
            if !bullet.alive {
                continue;
            }
            let hit_dist = ((self.ship.radius + bullet.radius) as i64) * 16;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, bullet.x, bullet.y)
                <= hit_dist * hit_dist
            {
                bullet.alive = false;
                self.destroy_ship();
                return;
            }
        }

        // Ship-saucer
        for saucer in &mut self.saucers {
            if !saucer.alive {
                continue;
            }
            let hit_dist = ((self.ship.radius + saucer.radius) as i64) * 16;
            if collision_dist_sq_q12_4(self.ship.x, self.ship.y, saucer.x, saucer.y)
                <= hit_dist * hit_dist
            {
                saucer.alive = false;
                self.destroy_ship();
                return;
            }
        }
    }

    fn destroy_asteroid_at(&mut self, index: usize, award_score: bool) {
        let asteroid = &self.asteroids[index];
        let size = asteroid.size;
        let x = asteroid.x;
        let y = asteroid.y;
        let parent_vx = asteroid.vx;
        let parent_vy = asteroid.vy;

        if award_score {
            self.time_since_last_kill = 0;
            let points = match size {
                AsteroidSize::Large => SCORE_LARGE_ASTEROID,
                AsteroidSize::Medium => SCORE_MEDIUM_ASTEROID,
                AsteroidSize::Small => SCORE_SMALL_ASTEROID,
            };
            self.add_score(points);
        }

        // Visual effects are skipped (explosion, debris, screen shake, audio)

        if size == AsteroidSize::Small {
            return;
        }

        let child_size = size.child_size().unwrap();
        let total_alive = self.asteroids.iter().filter(|a| a.alive).count();
        let split_count = if total_alive >= ASTEROID_CAP { 1 } else { 2 };

        for _ in 0..split_count {
            let mut child = self.create_asteroid(child_size, x, y);
            // Velocity inheritance: (vx * 46) >> 8 ≈ 0.18
            child.vx += (parent_vx * 46) >> 8;
            child.vy += (parent_vy * 46) >> 8;
            self.asteroids.push(child);
        }
    }

    fn destroy_ship(&mut self) {
        self.queue_ship_respawn(SHIP_RESPAWN_FRAMES);
        self.lives -= 1;

        // Visual effects skipped

        if self.lives <= 0 {
            self.game_over = true;
            self.ship.can_control = false;
            self.ship.respawn_timer = 99999;
        }
    }

    fn add_score(&mut self, points: u32) {
        self.score += points;

        while self.score >= self.next_extra_life_score {
            self.lives += 1;
            self.next_extra_life_score += EXTRA_LIFE_SCORE_STEP;
            // Audio and visual effects skipped
        }
    }

    fn prune_destroyed_entities(&mut self) {
        self.asteroids.retain(|a| a.alive);
        self.bullets.retain(|b| b.alive);
        self.saucers.retain(|s| s.alive);
        self.saucer_bullets.retain(|b| b.alive);
    }
}

/// Replay a tape and return (final_score, final_rng_state).
/// This is the core verification function used by the ZK guest.
pub fn replay_tape(seed: u32, inputs: &[u8]) -> (u32, u32) {
    let mut game = AsteroidsGame::new(seed);

    for &input_byte in inputs {
        let input = FrameInput::from_byte(input_byte);
        game.step(input);
    }

    (game.score(), game.rng_state())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_game_creation() {
        let game = AsteroidsGame::new(0xDEADBEEF);
        assert_eq!(game.score(), 0);
        assert_eq!(game.lives(), STARTING_LIVES);
        assert_eq!(game.wave(), 1);
        assert_eq!(game.frame_count(), 0);
    }

    #[test]
    fn test_empty_inputs() {
        // 0 frames should just return initial state
        let (score, _rng) = replay_tape(0xDEADBEEF, &[]);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_initial_rng_state_matches_ts() {
        // After creating game with seed 0xDEADBEEF and spawning wave 1,
        // the RNG state at frame 0 should be 4160380745 (from TS dump).
        let game = AsteroidsGame::new(0xDEADBEEF);
        assert_eq!(game.rng_state(), 4160380745);
    }
}
