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

    // ====================================================================
    // Test helpers
    // ====================================================================

    fn no_input() -> FrameInput {
        FrameInput::default()
    }

    fn input(left: bool, right: bool, thrust: bool, fire: bool) -> FrameInput {
        FrameInput { left, right, thrust, fire }
    }

    fn step_n(game: &mut AsteroidsGame, inp: FrameInput, n: u32) {
        for _ in 0..n {
            game.step(inp);
        }
    }

    // ====================================================================
    // Existing tests
    // ====================================================================

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

    // ====================================================================
    // Group A: Pure math helpers
    // ====================================================================

    #[test]
    fn test_wrap_x_q12_4() {
        // Identity — value in range stays the same
        assert_eq!(wrap_x_q12_4(1000), 1000);
        // Boundary — at exactly WORLD_WIDTH_Q12_4, wraps to 0
        assert_eq!(wrap_x_q12_4(WORLD_WIDTH_Q12_4), 0);
        // Wrap-over — one past boundary
        assert_eq!(wrap_x_q12_4(WORLD_WIDTH_Q12_4 + 5), 5);
        // Wrap-under — negative wraps up
        assert_eq!(wrap_x_q12_4(-1), WORLD_WIDTH_Q12_4 - 1);
    }

    #[test]
    fn test_wrap_y_q12_4() {
        assert_eq!(wrap_y_q12_4(500), 500);
        assert_eq!(wrap_y_q12_4(WORLD_HEIGHT_Q12_4), 0);
        assert_eq!(wrap_y_q12_4(WORLD_HEIGHT_Q12_4 + 10), 10);
        assert_eq!(wrap_y_q12_4(-3), WORLD_HEIGHT_Q12_4 - 3);
    }

    #[test]
    fn test_shortest_delta_q12_4() {
        // Direct positive delta
        assert_eq!(shortest_delta_q12_4(100, 200, WORLD_WIDTH_Q12_4), 100);
        // Direct negative delta
        assert_eq!(shortest_delta_q12_4(200, 100, WORLD_WIDTH_Q12_4), -100);
        // Wrapping: from near-right to near-left is shorter going right
        assert_eq!(
            shortest_delta_q12_4(WORLD_WIDTH_Q12_4 - 10, 10, WORLD_WIDTH_Q12_4),
            20
        );
        // Wrapping other direction: from near-left to near-right
        assert_eq!(
            shortest_delta_q12_4(10, WORLD_WIDTH_Q12_4 - 10, WORLD_WIDTH_Q12_4),
            -20
        );
    }

    #[test]
    fn test_collision_dist_sq_q12_4() {
        // Same point → distance 0
        assert_eq!(collision_dist_sq_q12_4(100, 200, 100, 200), 0);
        // Known triangle: dx=3*16=48, dy=4*16=64 → dist_sq = 48^2 + 64^2 = 2304 + 4096 = 6400
        assert_eq!(
            collision_dist_sq_q12_4(0, 0, to_q12_4(3), to_q12_4(4)),
            6400
        );
        // Cross-boundary: points at opposite edges are close
        let dist = collision_dist_sq_q12_4(10, 10, WORLD_WIDTH_Q12_4 - 10, WORLD_HEIGHT_Q12_4 - 10);
        // dx=20, dy=20 → dist_sq = 400+400 = 800
        assert_eq!(dist, 800);
    }

    #[test]
    fn test_to_q12_4() {
        assert_eq!(to_q12_4(0), 0);
        assert_eq!(to_q12_4(1), 16);
        assert_eq!(to_q12_4(960), 15360);
        assert_eq!(to_q12_4(-1), -16);
    }

    #[test]
    fn test_clamp() {
        // Within range
        assert_eq!(clamp(5, 0, 10), 5);
        // Below min
        assert_eq!(clamp(-5, 0, 10), 0);
        // Above max
        assert_eq!(clamp(15, 0, 10), 10);
    }

    // ====================================================================
    // Group B: Ship physics
    // ====================================================================

    #[test]
    fn test_ship_thrust_increases_velocity() {
        let mut game = AsteroidsGame::new(12345);
        // Ship starts facing up (angle=192), thrust should make vy negative
        let initial_vy = game.ship.vy;
        game.step(input(false, false, true, false));
        assert!(game.ship.vy < initial_vy, "thrust up should decrease vy (make it more negative)");
    }

    #[test]
    fn test_ship_turning() {
        let mut game = AsteroidsGame::new(12345);
        let initial_angle = game.ship.angle;

        // Turn left: angle decreases by SHIP_TURN_SPEED_BAM
        game.step(input(true, false, false, false));
        assert_eq!(
            game.ship.angle,
            initial_angle.wrapping_sub(SHIP_TURN_SPEED_BAM as u8)
        );

        // Turn right from new position
        let angle_after_left = game.ship.angle;
        game.step(input(false, true, false, false));
        assert_eq!(
            game.ship.angle,
            angle_after_left.wrapping_add(SHIP_TURN_SPEED_BAM as u8)
        );

        // Wrapping: turn left many times past 0
        let mut game2 = AsteroidsGame::new(99);
        game2.ship.angle = 1;
        game2.step(input(true, false, false, false));
        // 1 - 3 = 254 (wrapping)
        assert_eq!(game2.ship.angle, 254);
    }

    #[test]
    fn test_ship_drag_decelerates() {
        let mut game = AsteroidsGame::new(12345);
        // Give the ship some velocity via thrust
        step_n(&mut game, input(false, false, true, false), 10);
        let speed_after_thrust = game.ship.vx.abs() + game.ship.vy.abs();
        assert!(speed_after_thrust > 0, "ship should have velocity after thrust");

        // Coast without thrust — drag should reduce speed
        step_n(&mut game, no_input(), 30);
        let speed_after_coast = game.ship.vx.abs() + game.ship.vy.abs();
        assert!(
            speed_after_coast < speed_after_thrust,
            "drag should reduce speed: {} < {}",
            speed_after_coast,
            speed_after_thrust
        );
    }

    #[test]
    fn test_ship_fire_cooldown() {
        let mut game = AsteroidsGame::new(12345);
        // Fire a bullet
        game.step(input(false, false, false, true));
        assert_eq!(game.bullets.len(), 1, "first shot should create a bullet");
        assert_eq!(game.ship.fire_cooldown, SHIP_BULLET_COOLDOWN_FRAMES);

        // Fire again immediately — should be blocked by cooldown
        game.step(input(false, false, false, true));
        assert_eq!(game.bullets.len(), 1, "can't fire during cooldown");

        // Wait out the cooldown
        step_n(&mut game, no_input(), SHIP_BULLET_COOLDOWN_FRAMES as u32);
        assert_eq!(game.ship.fire_cooldown, 0);

        // Now fire again
        game.step(input(false, false, false, true));
        assert_eq!(game.bullets.len(), 2, "should fire after cooldown expires");
    }

    #[test]
    fn test_ship_bullet_limit() {
        let mut game = AsteroidsGame::new(12345);

        // Fire 4 bullets (max) with cooldown waits between
        for i in 0..SHIP_BULLET_LIMIT {
            game.step(input(false, false, false, true));
            assert_eq!(game.bullets.len(), i + 1);
            if i < SHIP_BULLET_LIMIT - 1 {
                step_n(&mut game, no_input(), SHIP_BULLET_COOLDOWN_FRAMES as u32);
            }
        }

        assert_eq!(game.bullets.len(), SHIP_BULLET_LIMIT);

        // Wait for cooldown and try to fire a 5th — should be blocked
        step_n(&mut game, no_input(), SHIP_BULLET_COOLDOWN_FRAMES as u32);
        let count_before = game.bullets.len();
        game.step(input(false, false, false, true));
        assert_eq!(game.bullets.len(), count_before, "5th bullet should be blocked");
    }

    // ====================================================================
    // Group C: Collision & scoring
    // ====================================================================

    #[test]
    fn test_asteroid_scoring_by_size() {
        for (size, expected_score) in [
            (AsteroidSize::Large, SCORE_LARGE_ASTEROID),
            (AsteroidSize::Medium, SCORE_MEDIUM_ASTEROID),
            (AsteroidSize::Small, SCORE_SMALL_ASTEROID),
        ] {
            let mut game = AsteroidsGame::new(12345);
            game.asteroids.clear();
            game.saucers.clear();
            game.saucer_bullets.clear();
            game.score = 0;
            game.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;

            // Place a test asteroid at a known position
            let ax = to_q12_4(100);
            let ay = to_q12_4(100);
            game.asteroids.push(Asteroid {
                x: ax,
                y: ay,
                vx: 0,
                vy: 0,
                angle: 0,
                alive: true,
                radius: asteroid_radius(size),
                size,
                spin: 0,
            });

            // Place a bullet overlapping the asteroid
            game.bullets.push(Bullet {
                x: ax,
                y: ay,
                vx: 0,
                vy: 0,
                angle: 0,
                alive: true,
                radius: 2,
                life: 60,
            });

            game.handle_collisions();
            game.prune_destroyed_entities();

            assert_eq!(
                game.score, expected_score,
                "score for {:?}: expected {}, got {}",
                size, expected_score, game.score
            );
        }
    }

    #[test]
    fn test_saucer_scoring() {
        for (small, expected_score) in [(false, SCORE_LARGE_SAUCER), (true, SCORE_SMALL_SAUCER)] {
            let mut game = AsteroidsGame::new(12345);
            game.asteroids.clear();
            game.saucers.clear();
            game.saucer_bullets.clear();
            game.bullets.clear();
            game.score = 0;
            game.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;

            let sx = to_q12_4(200);
            let sy = to_q12_4(200);
            game.saucers.push(Saucer {
                x: sx,
                y: sy,
                vx: 0,
                vy: 0,
                alive: true,
                radius: if small { SAUCER_RADIUS_SMALL } else { SAUCER_RADIUS_LARGE },
                small,
                fire_cooldown: 999,
                drift_timer: 999,
            });

            // Bullet overlapping saucer
            game.bullets.push(Bullet {
                x: sx,
                y: sy,
                vx: 0,
                vy: 0,
                angle: 0,
                alive: true,
                radius: 2,
                life: 60,
            });

            game.handle_collisions();
            game.prune_destroyed_entities();

            assert_eq!(
                game.score, expected_score,
                "score for saucer (small={}): expected {}, got {}",
                small, expected_score, game.score
            );
        }
    }

    #[test]
    fn test_extra_life_at_10k() {
        let mut game = AsteroidsGame::new(12345);
        let initial_lives = game.lives;
        // Push score just past 10000
        game.add_score(10000);
        assert_eq!(
            game.lives,
            initial_lives + 1,
            "should gain an extra life at 10000"
        );
    }

    #[test]
    fn test_invulnerability_prevents_death() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.saucer_bullets.clear();
        game.bullets.clear();

        // Ensure ship is invulnerable
        game.ship.invulnerable_timer = 60;
        game.ship.can_control = true;

        let initial_lives = game.lives;

        // Place asteroid overlapping ship
        game.asteroids.push(Asteroid {
            x: game.ship.x,
            y: game.ship.y,
            vx: 0,
            vy: 0,
            angle: 0,
            alive: true,
            radius: ASTEROID_RADIUS_LARGE,
            size: AsteroidSize::Large,
            spin: 0,
        });

        game.handle_collisions();

        assert_eq!(game.lives, initial_lives, "invulnerable ship should not lose a life");
        assert!(game.ship.can_control, "invulnerable ship should still be controllable");
    }

    #[test]
    fn test_game_over_at_zero_lives() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.saucer_bullets.clear();
        game.bullets.clear();

        game.lives = 1;
        game.ship.invulnerable_timer = 0;
        game.ship.can_control = true;

        // Place asteroid overlapping ship to kill it
        game.asteroids.push(Asteroid {
            x: game.ship.x,
            y: game.ship.y,
            vx: 0,
            vy: 0,
            angle: 0,
            alive: true,
            radius: ASTEROID_RADIUS_LARGE,
            size: AsteroidSize::Large,
            spin: 0,
        });

        game.handle_collisions();

        assert_eq!(game.lives, 0);
        assert!(game.game_over, "game should be over when lives reach 0");
    }

    // ====================================================================
    // Group D: Wave progression
    // ====================================================================

    #[test]
    fn test_wave_asteroid_count() {
        // Wave 1: 4 + (1-1)*2 = 4 asteroids
        let game1 = AsteroidsGame::new(12345);
        assert_eq!(game1.wave(), 1);
        assert_eq!(game1.asteroids.len(), 4);

        // Simulate clearing wave 1 to get to wave 2
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        // step triggers spawn_wave since field is empty
        game.step(no_input());
        assert_eq!(game.wave(), 2);
        // Wave 2: min(16, 4 + (2-1)*2) = 6
        assert_eq!(game.asteroids.len(), 6);
    }

    #[test]
    fn test_wave_advances_when_clear() {
        let mut game = AsteroidsGame::new(12345);
        assert_eq!(game.wave(), 1);

        // Clear all entities
        game.asteroids.clear();
        game.saucers.clear();

        // Step should trigger a new wave
        game.step(no_input());
        assert_eq!(game.wave(), 2, "wave should advance when field is clear");
        assert!(!game.asteroids.is_empty(), "new wave should spawn asteroids");
    }

    // ====================================================================
    // Group E: Spawn safety
    // ====================================================================

    #[test]
    fn test_spawn_area_clear_no_entities() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.saucer_bullets.clear();

        let (cx, cy) = game.get_ship_spawn_point();
        assert!(
            game.is_ship_spawn_area_clear(cx, cy),
            "empty field should allow spawn"
        );
    }

    #[test]
    fn test_spawn_area_blocked_by_asteroid() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.saucer_bullets.clear();

        let (cx, cy) = game.get_ship_spawn_point();

        // Place a large asteroid at center
        game.asteroids.push(Asteroid {
            x: cx,
            y: cy,
            vx: 0,
            vy: 0,
            angle: 0,
            alive: true,
            radius: ASTEROID_RADIUS_LARGE,
            size: AsteroidSize::Large,
            spin: 0,
        });

        assert!(
            !game.is_ship_spawn_area_clear(cx, cy),
            "asteroid at center should block spawn"
        );
    }

    // ====================================================================
    // Group F: Anti-cheat regression tests
    // ====================================================================

    #[test]
    fn test_ship_max_speed_clamped() {
        let mut game = AsteroidsGame::new(12345);
        // Thrust for many frames to saturate speed
        step_n(&mut game, input(false, false, true, false), 200);
        let speed_sq = game.ship.vx * game.ship.vx + game.ship.vy * game.ship.vy;
        assert!(
            speed_sq <= SHIP_MAX_SPEED_SQ_Q16_16,
            "speed_sq {} exceeds max {}",
            speed_sq,
            SHIP_MAX_SPEED_SQ_Q16_16
        );
        // Also verify the ship IS moving (didn't clamp to zero)
        assert!(speed_sq > 0, "ship should have nonzero velocity after sustained thrust");
    }

    #[test]
    fn test_different_seed_different_rng_state() {
        let (_, rng1) = replay_tape(0xDEADBEEF, &[0x00; 100]);
        let (_, rng2) = replay_tape(0xDEADBEF0, &[0x00; 100]);
        assert_ne!(
            rng1, rng2,
            "different seeds must produce different final RNG states"
        );
    }

    #[test]
    fn test_active_vs_idle_play_differs() {
        // Over enough frames for saucer spawns and potential kills,
        // active play (fire+thrust) must produce a different proven
        // outcome than idle play. This is the core anti-cheat property:
        // you can't claim someone else's score with different inputs.
        let active: Vec<u8> = vec![0x0C; 600]; // thrust+fire every frame
        let idle: Vec<u8> = vec![0x00; 600];
        let (score_a, rng_a) = replay_tape(0xDEADBEEF, &active);
        let (score_i, rng_i) = replay_tape(0xDEADBEEF, &idle);
        assert!(
            (score_a, rng_a) != (score_i, rng_i),
            "active vs idle must produce different (score, rng): ({}, 0x{:08x}) vs ({}, 0x{:08x})",
            score_a, rng_a, score_i, rng_i
        );
    }

    #[test]
    fn test_saucer_bullet_kills_ship() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.bullets.clear();
        game.saucer_bullets.clear();
        game.ship.invulnerable_timer = 0;
        game.ship.can_control = true;
        let initial_lives = game.lives;

        // Place saucer bullet overlapping ship
        game.saucer_bullets.push(Bullet {
            x: game.ship.x,
            y: game.ship.y,
            vx: 0,
            vy: 0,
            angle: 0,
            alive: true,
            radius: 2,
            life: 60,
        });

        game.handle_collisions();

        assert_eq!(game.lives, initial_lives - 1, "saucer bullet should kill ship");
        assert!(!game.ship.can_control, "ship should be in respawn state after death");
    }

    #[test]
    fn test_ship_saucer_body_collision() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.bullets.clear();
        game.saucer_bullets.clear();
        game.ship.invulnerable_timer = 0;
        game.ship.can_control = true;
        let initial_lives = game.lives;

        // Place saucer overlapping ship
        game.saucers.push(Saucer {
            x: game.ship.x,
            y: game.ship.y,
            vx: 0,
            vy: 0,
            alive: true,
            radius: SAUCER_RADIUS_LARGE,
            small: false,
            fire_cooldown: 999,
            drift_timer: 999,
        });

        game.handle_collisions();

        assert_eq!(game.lives, initial_lives - 1, "saucer body collision should kill ship");
        assert!(!game.saucers[0].alive, "saucer should also be destroyed on collision");
    }

    #[test]
    fn test_asteroid_splitting() {
        // Large → 2 medium
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.saucer_bullets.clear();
        game.bullets.clear();

        let ax = to_q12_4(500);
        let ay = to_q12_4(500);
        game.asteroids.push(Asteroid {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: ASTEROID_RADIUS_LARGE,
            size: AsteroidSize::Large, spin: 0,
        });
        game.bullets.push(Bullet {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: 2, life: 60,
        });

        game.handle_collisions();
        game.prune_destroyed_entities();

        let medium_count = game.asteroids.iter().filter(|a| a.size == AsteroidSize::Medium).count();
        assert_eq!(medium_count, 2, "large asteroid should split into 2 medium");

        // Medium → 2 small
        let mut game2 = AsteroidsGame::new(12345);
        game2.asteroids.clear();
        game2.saucers.clear();
        game2.saucer_bullets.clear();
        game2.bullets.clear();

        game2.asteroids.push(Asteroid {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: ASTEROID_RADIUS_MEDIUM,
            size: AsteroidSize::Medium, spin: 0,
        });
        game2.bullets.push(Bullet {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: 2, life: 60,
        });

        game2.handle_collisions();
        game2.prune_destroyed_entities();

        let small_count = game2.asteroids.iter().filter(|a| a.size == AsteroidSize::Small).count();
        assert_eq!(small_count, 2, "medium asteroid should split into 2 small");

        // Small → nothing
        let mut game3 = AsteroidsGame::new(12345);
        game3.asteroids.clear();
        game3.saucers.clear();
        game3.saucer_bullets.clear();
        game3.bullets.clear();

        game3.asteroids.push(Asteroid {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: ASTEROID_RADIUS_SMALL,
            size: AsteroidSize::Small, spin: 0,
        });
        game3.bullets.push(Bullet {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: 2, life: 60,
        });

        game3.handle_collisions();
        game3.prune_destroyed_entities();

        assert_eq!(game3.asteroids.len(), 0, "small asteroid should not split");
    }

    #[test]
    fn test_saucer_bullet_destroys_asteroid_no_score() {
        let mut game = AsteroidsGame::new(12345);
        game.asteroids.clear();
        game.saucers.clear();
        game.bullets.clear();
        game.saucer_bullets.clear();
        game.score = 0;
        game.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;
        // Move ship out of the way so it doesn't collide
        game.ship.x = to_q12_4(800);
        game.ship.y = to_q12_4(600);
        game.ship.invulnerable_timer = 999;

        let ax = to_q12_4(100);
        let ay = to_q12_4(100);
        game.asteroids.push(Asteroid {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: ASTEROID_RADIUS_LARGE,
            size: AsteroidSize::Large, spin: 0,
        });

        // Saucer bullet overlapping the asteroid
        game.saucer_bullets.push(Bullet {
            x: ax, y: ay, vx: 0, vy: 0, angle: 0,
            alive: true, radius: 2, life: 60,
        });

        game.handle_collisions();
        game.prune_destroyed_entities();

        assert_eq!(game.score, 0, "saucer bullet destroying asteroid must NOT award score");
        // Asteroid should still be destroyed (and split)
        assert!(
            game.asteroids.iter().all(|a| a.size == AsteroidSize::Medium),
            "asteroid should still split even without score"
        );
    }

    #[test]
    fn test_wave_does_not_advance_with_saucers_alive() {
        let mut game = AsteroidsGame::new(12345);
        let initial_wave = game.wave();

        // Clear asteroids but leave a saucer
        game.asteroids.clear();
        game.saucers.clear();
        game.saucers.push(Saucer {
            x: to_q12_4(100),
            y: to_q12_4(100),
            vx: 0,
            vy: 0,
            alive: true,
            radius: SAUCER_RADIUS_LARGE,
            small: false,
            fire_cooldown: 999,
            drift_timer: 999,
        });

        game.step(no_input());

        assert_eq!(
            game.wave(), initial_wave,
            "wave must not advance while saucers are alive"
        );
    }
}
