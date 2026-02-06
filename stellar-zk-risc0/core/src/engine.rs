use crate::constants::*;
use crate::fixed_point::*;
use crate::rng::Rng;
use crate::rules::{check_frame_invariants, RuleViolation};
use crate::types::*;

/// Game engine for deterministic Asteroids simulation
pub struct GameEngine {
    state: GameState,
    rng: Rng,
}

impl GameEngine {
    /// Create new game engine with seed
    pub fn new(seed: u32) -> Self {
        let mut engine = GameEngine {
            state: GameState::default(),
            rng: Rng::new(seed),
        };
        engine.start_new_game();
        engine
    }

    /// Start a new game
    fn start_new_game(&mut self) {
        self.state = GameState::default();
        self.state.mode = GameMode::Playing;
        self.state.score = 0;
        self.state.lives = STARTING_LIVES;
        self.state.wave = 0;
        self.state.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;

        // Create ship at center
        self.state.ship = Ship {
            x: SHIP_START_X_Q12_4,
            y: SHIP_START_Y_Q12_4,
            vx: 0,
            vy: 0,
            angle: SHIP_START_ANGLE_BAM,
            can_control: true,
            fire_cooldown: 0,
            invulnerable_timer: SHIP_SPAWN_INVULNERABLE_FRAMES,
            respawn_timer: 0,
        };

        // Initialize saucer spawn timer
        self.state.saucer_spawn_timer = self.rng.next_range(
            SAUCER_SPAWN_MIN_FRAMES as u32,
            SAUCER_SPAWN_MAX_FRAMES as u32,
        ) as u16;

        // Spawn first wave
        self.spawn_wave();
    }

    /// Get current RNG state
    pub fn rng_state(&self) -> u32 {
        self.rng.state()
    }

    /// Get current game state
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// Get mutable game state (for testing)
    pub fn state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Spawn a new wave of asteroids
    fn spawn_wave(&mut self) {
        self.state.wave += 1;
        self.state.time_since_last_kill = 0;

        // Calculate number of asteroids: min(16, 4 + (wave-1)*2)
        let count = (4 + (self.state.wave - 1) * 2).min(16) as usize;

        for _ in 0..count {
            self.spawn_asteroid(AsteroidSize::Large);
        }
    }

    /// Spawn a single asteroid
    fn spawn_asteroid(&mut self, size: AsteroidSize) {
        // Try to spawn at safe distance from ship
        let mut attempts = 0;
        let max_attempts = 20;

        while attempts < max_attempts {
            let x = self.rng.next_int(WORLD_WIDTH_Q12_4 as u32) as u16;
            let y = self.rng.next_int(WORLD_HEIGHT_Q12_4 as u32) as u16;

            // Check distance from ship
            let dist_sq = distance_sq_q12_4(x, y, self.state.ship.x, self.state.ship.y);
            let safe_dist_sq =
                (SPAWN_SAFE_DISTANCE_Q12_4 as u32) * (SPAWN_SAFE_DISTANCE_Q12_4 as u32);

            if dist_sq > safe_dist_sq {
                // Random angle and speed based on size
                let angle = self.rng.next_angle();
                let base_speed = match size {
                    AsteroidSize::Large => self.rng.next_range(
                        ASTEROID_SPEED_LARGE_Q8_8.0 as u32,
                        ASTEROID_SPEED_LARGE_Q8_8.1 as u32,
                    ) as i16,
                    AsteroidSize::Medium => self.rng.next_range(
                        ASTEROID_SPEED_MEDIUM_Q8_8.0 as u32,
                        ASTEROID_SPEED_MEDIUM_Q8_8.1 as u32,
                    ) as i16,
                    AsteroidSize::Small => self.rng.next_range(
                        ASTEROID_SPEED_SMALL_Q8_8.0 as u32,
                        ASTEROID_SPEED_SMALL_Q8_8.1 as u32,
                    ) as i16,
                };

                // Apply wave speed multiplier: speed * (1 + min(0.5, (wave-1)*0.06))
                // Integer: speed + speed * min(128, (wave-1)*15) >> 8
                let wave_bonus = ((self.state.wave - 1) * 15).min(128) as i32;
                let speed = base_speed + ((base_speed as i32 * wave_bonus) >> 8) as i16;

                let start_angle = self.rng.next_angle();
                let spin = self.rng.next_spin();

                let (vx, vy) = velocity_q8_8(angle, speed);

                self.state.asteroids.push(Asteroid {
                    x,
                    y,
                    vx,
                    vy,
                    angle: start_angle,
                    spin,
                    size,
                    alive: true,
                });
                return;
            }

            attempts += 1;
        }

        // If we couldn't find a safe spot, spawn anyway (edge case)
        let x = self.rng.next_int(WORLD_WIDTH_Q12_4 as u32) as u16;
        let y = self.rng.next_int(WORLD_HEIGHT_Q12_4 as u32) as u16;
        let angle = self.rng.next_angle();
        let base_speed = self.rng.next_range(
            ASTEROID_SPEED_LARGE_Q8_8.0 as u32,
            ASTEROID_SPEED_LARGE_Q8_8.1 as u32,
        ) as i16;
        let wave_bonus = ((self.state.wave - 1) * 15).min(128) as i32;
        let speed = base_speed + ((base_speed as i32 * wave_bonus) >> 8) as i16;
        let (vx, vy) = velocity_q8_8(angle, speed);

        self.state.asteroids.push(Asteroid {
            x,
            y,
            vx,
            vy,
            angle: self.rng.next_angle(),
            spin: self.rng.next_spin(),
            size,
            alive: true,
        });
    }

    /// Step simulation for one frame with given input
    pub fn step(&mut self, input: FrameInput) {
        self.state.frame_count += 1;

        // Update ship
        self.update_ship(input);

        // Update asteroids
        self.update_asteroids();

        // Update bullets
        self.update_bullets();

        // Update saucers
        self.update_saucers();

        // Update saucer bullets
        self.update_saucer_bullets();

        // Handle collisions
        self.handle_collisions();

        // Prune destroyed entities
        self.prune_destroyed();

        // Update time since last kill
        self.state.time_since_last_kill += 1;

        // Check wave completion
        if self.state.mode == GameMode::Playing {
            let asteroids_alive = self.state.asteroids.iter().any(|a| a.alive);
            let saucers_alive = self.state.saucers.iter().any(|s| s.alive);

            if !asteroids_alive && !saucers_alive {
                self.spawn_wave();
            }
        }
    }

    /// Update ship state
    fn update_ship(&mut self, input: FrameInput) {
        if self.state.ship.can_control {
            // Handle rotation
            let mut turn = 0i8;
            if input.left && !input.right {
                turn = -SHIP_TURN_SPEED_BAM;
            } else if input.right && !input.left {
                turn = SHIP_TURN_SPEED_BAM;
            }
            self.state.ship.angle = add_bam(self.state.ship.angle, turn);

            // Handle thrust
            if input.thrust {
                let cos_val = cos_bam(self.state.ship.angle);
                let sin_val = sin_bam(self.state.ship.angle);

                // Add thrust acceleration (Q8.8)
                let ax = mul_q8_8_by_q0_14(SHIP_THRUST_Q8_8, cos_val);
                let ay = mul_q8_8_by_q0_14(SHIP_THRUST_Q8_8, sin_val);

                self.state.ship.vx = self.state.ship.vx.saturating_add(ax);
                self.state.ship.vy = self.state.ship.vy.saturating_add(ay);
            }

            // Apply drag
            self.state.ship.vx = apply_drag_q8_8(self.state.ship.vx);
            self.state.ship.vy = apply_drag_q8_8(self.state.ship.vy);

            // Clamp speed
            let (vx, vy) = clamp_speed_q8_8(self.state.ship.vx, self.state.ship.vy);
            self.state.ship.vx = vx;
            self.state.ship.vy = vy;

            // Update position
            let dx = vel_to_pos_delta(self.state.ship.vx);
            let dy = vel_to_pos_delta(self.state.ship.vy);

            self.state.ship.x =
                wrap_q12_4(add_q12_4(self.state.ship.x, dx as u16), WORLD_WIDTH_Q12_4);
            self.state.ship.y =
                wrap_q12_4(add_q12_4(self.state.ship.y, dy as u16), WORLD_HEIGHT_Q12_4);

            // Handle firing
            let should_fire = if self.state.ship.fire_cooldown > 0 {
                self.state.ship.fire_cooldown -= 1;
                false
            } else {
                input.fire && self.state.bullets.len() < SHIP_BULLET_LIMIT as usize
            };

            if should_fire {
                // Copy ship data for bullet spawning (Ship is Copy, so no clone needed)
                let ship_data = self.state.ship;
                self.spawn_bullet_from_ship(&ship_data);
                self.state.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
            }

            // Update invulnerability timer
            if self.state.ship.invulnerable_timer > 0 {
                self.state.ship.invulnerable_timer -= 1;
            }
        } else {
            // Ship is dead, handle respawn
            if self.state.ship.respawn_timer > 0 {
                self.state.ship.respawn_timer -= 1;

                if self.state.ship.respawn_timer == 0 {
                    self.try_respawn_ship();
                }
            }
        }
    }

    /// Try to respawn ship at center
    fn try_respawn_ship(&mut self) {
        // Check if spawn area is clear
        let spawn_x = SHIP_START_X_Q12_4;
        let spawn_y = SHIP_START_Y_Q12_4;

        let area_clear = self.state.asteroids.iter().all(|a| {
            if !a.alive {
                return true;
            }
            let dist_sq = distance_sq_q12_4(a.x, a.y, spawn_x, spawn_y);
            let min_dist = a.size.radius_q12_4() + SPAWN_SAFE_DISTANCE_Q12_4;
            dist_sq > (min_dist as u32) * (min_dist as u32)
        });

        if area_clear {
            let ship = &mut self.state.ship;
            ship.x = spawn_x;
            ship.y = spawn_y;
            ship.vx = 0;
            ship.vy = 0;
            ship.angle = SHIP_START_ANGLE_BAM;
            ship.can_control = true;
            ship.invulnerable_timer = SHIP_SPAWN_INVULNERABLE_FRAMES;
            ship.respawn_timer = 0;
        } else {
            // Retry next frame
            self.state.ship.respawn_timer = 1;
        }
    }

    /// Spawn a player bullet from ship data
    fn spawn_bullet_from_ship(&mut self, ship: &Ship) {
        // Spawn at ship nose (radius + 6 pixels offset)
        let offset = SHIP_RADIUS_Q12_4 + (6 << 4);
        let (bullet_x, bullet_y) = displace_q12_4(ship.x, ship.y, ship.angle, offset);

        // Bullet velocity: base speed + ship velocity
        let (base_vx, base_vy) = velocity_q8_8(ship.angle, SHIP_BULLET_SPEED_Q8_8);
        let bullet_vx = base_vx.saturating_add(ship.vx);
        let bullet_vy = base_vy.saturating_add(ship.vy);

        self.state.bullets.push(Bullet {
            x: bullet_x,
            y: bullet_y,
            vx: bullet_vx,
            vy: bullet_vy,
            life: SHIP_BULLET_LIFETIME_FRAMES,
            from_saucer: false,
        });
    }

    /// Update asteroids
    fn update_asteroids(&mut self) {
        for asteroid in &mut self.state.asteroids {
            if !asteroid.alive {
                continue;
            }

            // Update position
            let dx = vel_to_pos_delta(asteroid.vx);
            let dy = vel_to_pos_delta(asteroid.vy);

            asteroid.x = wrap_q12_4(add_q12_4(asteroid.x, dx as u16), WORLD_WIDTH_Q12_4);
            asteroid.y = wrap_q12_4(add_q12_4(asteroid.y, dy as u16), WORLD_HEIGHT_Q12_4);

            // Update rotation
            asteroid.angle = add_bam(asteroid.angle, asteroid.spin);
        }
    }

    /// Update bullets
    fn update_bullets(&mut self) {
        for bullet in &mut self.state.bullets {
            if bullet.life > 0 {
                bullet.life -= 1;

                // Update position
                let dx = vel_to_pos_delta(bullet.vx);
                let dy = vel_to_pos_delta(bullet.vy);

                bullet.x = wrap_q12_4(add_q12_4(bullet.x, dx as u16), WORLD_WIDTH_Q12_4);
                bullet.y = wrap_q12_4(add_q12_4(bullet.y, dy as u16), WORLD_HEIGHT_Q12_4);
            }
        }
    }

    /// Update saucers
    fn update_saucers(&mut self) {
        // Decrement spawn timer
        if self.state.saucer_spawn_timer > 0 {
            self.state.saucer_spawn_timer -= 1;
        }

        // Check if we should spawn a saucer
        let max_saucers = if self.state.wave < 4 {
            1
        } else if self.state.wave < 7 {
            2
        } else {
            3
        };

        let current_saucers = self.state.saucers.iter().filter(|s| s.alive).count();

        if self.state.saucer_spawn_timer == 0 && current_saucers < max_saucers {
            self.spawn_saucer();
        }

        // Collect bullets to spawn using stack-allocated array (max 3 saucers)
        let mut bullets_to_spawn: [(u16, u16, i16, i16, bool); 3] = [(0, 0, 0, 0, false); 3];
        let mut spawn_count: usize = 0;

        // Update existing saucers
        for saucer in &mut self.state.saucers {
            if !saucer.alive {
                continue;
            }

            // Update position (saucers don't wrap on X axis)
            let dx = vel_to_pos_delta(saucer.vx);
            let dy = vel_to_pos_delta(saucer.vy);

            // X: no wrap, dies if off-screen
            let new_x = add_q12_4(saucer.x, dx as u16);
            let offscreen_margin: u16 = (80 << 4) as u16; // 80 pixels

            if new_x < offscreen_margin || new_x > WORLD_WIDTH_Q12_4 + offscreen_margin {
                saucer.alive = false;
                continue;
            }
            saucer.x = new_x;

            // Y: wrap
            saucer.y = wrap_q12_4(add_q12_4(saucer.y, dy as u16), WORLD_HEIGHT_Q12_4);

            // Update drift timer
            if saucer.drift_timer > 0 {
                saucer.drift_timer -= 1;
            } else {
                // Change drift direction
                // TypeScript: randomInt(-163, 164) for vy
                let new_vy = self.rng.next_int(328) as i16 - 164;
                saucer.vy = new_vy;
                saucer.drift_timer = self.rng.next_range(48, 120) as u16;
            }

            // Handle firing
            if saucer.fire_cooldown > 0 {
                saucer.fire_cooldown -= 1;
            } else {
                // Calculate bullet velocity inline to avoid borrow issues
                let is_small = saucer.small;
                let saucer_x = saucer.x;
                let saucer_y = saucer.y;

                let (bullet_vx, bullet_vy) = if is_small {
                    // Small saucer: aim at ship with some error
                    let dx = shortest_delta_q12_4(saucer_x, self.state.ship.x, WORLD_WIDTH_Q12_4);
                    let dy = shortest_delta_q12_4(saucer_y, self.state.ship.y, WORLD_HEIGHT_Q12_4);

                    let base_angle = atan2_bam(dy, dx);

                    // Calculate error based on TypeScript logic:
                    // Base error: 21 BAM normally, 11 BAM when lurking
                    // Score bonus: score / 2500 (integer division)
                    // Wave bonus: min(11, wave)
                    // Final: clamp(base - score_bonus - wave_bonus, 3, base)
                    let is_lurking = self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
                    let base_error_bam: i16 = if is_lurking { 11 } else { 21 };
                    let score_bonus: i16 = (self.state.score / 2500) as i16;
                    let wave_bonus: i16 = (self.state.wave as i16).min(11);
                    let error_bam =
                        clamp(base_error_bam - score_bonus - wave_bonus, 3, base_error_bam);

                    let error = self.rng.next_int((error_bam * 2 + 1) as u32) as i16 - error_bam;
                    let angle = add_bam(base_angle, error as i8);

                    velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
                } else {
                    // Large saucer: random direction
                    let angle = self.rng.next_angle();
                    velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
                };

                if spawn_count < 3 {
                    bullets_to_spawn[spawn_count] =
                        (saucer_x, saucer_y, bullet_vx, bullet_vy, true);
                    spawn_count += 1;
                }

                // Reset cooldown based on saucer size and lurk state
                // TypeScript logic:
                // Small: lurking ? randomInt(27, 46) : randomInt(39, 66)
                // Large: lurking ? randomInt(46, 67) : randomInt(66, 96)
                let is_lurking = self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
                let new_cooldown = match (is_small, is_lurking) {
                    (true, true) => self.rng.next_range(27, 46),
                    (true, false) => self.rng.next_range(39, 66),
                    (false, true) => self.rng.next_range(46, 67),
                    (false, false) => self.rng.next_range(66, 96),
                } as u8;
                saucer.fire_cooldown = new_cooldown;
            }
        }

        // Spawn collected bullets
        for i in 0..spawn_count {
            let (x, y, vx, vy, from_saucer) = bullets_to_spawn[i];
            self.state.saucer_bullets.push(Bullet {
                x,
                y,
                vx,
                vy,
                life: SAUCER_BULLET_LIFETIME_FRAMES,
                from_saucer,
            });
        }
    }

    /// Spawn a saucer
    fn spawn_saucer(&mut self) {
        // Determine size based on wave and lurk state
        // TypeScript logic:
        // - When lurking (>360 frames since last kill): 90% small
        // - When score > 4000: 70% small
        // - Otherwise: 22% small
        let is_lurking = self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let small_threshold = if is_lurking {
            90u32 // 90% small when lurking
        } else if self.state.score > SAUCER_SMALL_SCORE_THRESHOLD {
            70u32 // 70% small when score > 4000
        } else {
            SAUCER_SMALL_PCT_BASE as u32 // 22% small normally
        };
        let is_small = self.rng.next_int(100) < small_threshold;

        // Random entry side (0 = left, 1 = right)
        let from_left = self.rng.next_bool_q8_8(128); // 50%

        let x: u16 = if from_left { 0 } else { WORLD_WIDTH_Q12_4 };

        let y = self.rng.next_int(WORLD_HEIGHT_Q12_4 as u32) as u16;

        let speed = if is_small {
            SAUCER_SPEED_SMALL_Q8_8
        } else {
            SAUCER_SPEED_LARGE_Q8_8
        };

        let vx: i16 = if from_left { speed } else { -speed };
        let vy: i16 = (self.rng.next_int(188) as i16) - 94; // Random Y velocity [-94, 94)

        let fire_cooldown = self.rng.next_range(18, 48) as u8;
        let drift_timer = self.rng.next_range(48, 120) as u16;

        self.state.saucers.push(Saucer {
            x,
            y,
            vx,
            vy,
            small: is_small,
            fire_cooldown,
            drift_timer,
            alive: true,
        });

        // Reset spawn timer
        let min_frames = if is_lurking {
            LURK_SAUCER_SPAWN_FAST_FRAMES
        } else {
            SAUCER_SPAWN_MIN_FRAMES
        };

        self.state.saucer_spawn_timer =
            self.rng
                .next_range(min_frames as u32, SAUCER_SPAWN_MAX_FRAMES as u32) as u16;
    }

    /// Update saucer bullets
    fn update_saucer_bullets(&mut self) {
        for bullet in &mut self.state.saucer_bullets {
            if bullet.life > 0 {
                bullet.life -= 1;

                let dx = vel_to_pos_delta(bullet.vx);
                let dy = vel_to_pos_delta(bullet.vy);

                bullet.x = wrap_q12_4(add_q12_4(bullet.x, dx as u16), WORLD_WIDTH_Q12_4);
                bullet.y = wrap_q12_4(add_q12_4(bullet.y, dy as u16), WORLD_HEIGHT_Q12_4);
            }
        }
    }

    /// Handle all collisions
    fn handle_collisions(&mut self) {
        // 1. Player bullets vs asteroids
        self.handle_bullet_asteroid_collisions();

        // 2. Player bullets vs saucers
        self.handle_bullet_saucer_collisions();

        // 3. Ship vs asteroids (if not invulnerable)
        self.handle_ship_asteroid_collisions();

        // 4. Ship vs saucers
        self.handle_ship_saucer_collisions();

        // 5. Ship vs saucer bullets
        self.handle_ship_saucer_bullet_collisions();

        // 6. Saucer bullets vs asteroids (no score)
        self.handle_saucer_bullet_asteroid_collisions();
    }

    /// Handle bullet-asteroid collisions using stack-allocated array (no Vec allocation)
    /// Max 4 bullets, so max 4 collisions
    fn handle_bullet_asteroid_collisions(&mut self) {
        // Stack-allocated array for collision pairs (max 4 bullets = max 4 collisions)
        let mut collisions: [(usize, usize); 4] = [(0, 0); 4];
        let mut collision_count: usize = 0;

        for i in 0..self.state.bullets.len() {
            if !self.state.bullets[i].alive() {
                continue;
            }

            for j in 0..self.state.asteroids.len() {
                if !self.state.asteroids[j].alive {
                    continue;
                }

                let bullet = &self.state.bullets[i];
                let asteroid = &self.state.asteroids[j];

                let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y);
                let threshold = (BULLET_RADIUS_Q12_4 + asteroid.size.radius_q12_4()) as u32;
                let threshold_sq = threshold * threshold;

                if dist_sq < threshold_sq {
                    if collision_count < 4 {
                        collisions[collision_count] = (i, j);
                        collision_count += 1;
                    }
                    break; // Bullet can only hit one asteroid
                }
            }
        }

        // Apply collisions
        for k in 0..collision_count {
            let (bullet_idx, asteroid_idx) = collisions[k];
            self.state.bullets[bullet_idx].life = 0;
            self.destroy_asteroid(asteroid_idx, true);
        }
    }

    /// Handle bullet-saucer collisions using stack-allocated array
    fn handle_bullet_saucer_collisions(&mut self) {
        // Stack-allocated array (max 4 bullets)
        let mut collisions: [(usize, usize, bool); 4] = [(0, 0, false); 4];
        let mut collision_count: usize = 0;

        for i in 0..self.state.bullets.len() {
            if !self.state.bullets[i].alive() {
                continue;
            }

            for j in 0..self.state.saucers.len() {
                if !self.state.saucers[j].alive {
                    continue;
                }

                let bullet = &self.state.bullets[i];
                let saucer = &self.state.saucers[j];

                let radius = if saucer.small {
                    SAUCER_RADIUS_SMALL_Q12_4
                } else {
                    SAUCER_RADIUS_LARGE_Q12_4
                };

                let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, saucer.x, saucer.y);
                let threshold = (BULLET_RADIUS_Q12_4 + radius) as u32;
                let threshold_sq = threshold * threshold;

                if dist_sq < threshold_sq {
                    if collision_count < 4 {
                        collisions[collision_count] = (i, j, saucer.small);
                        collision_count += 1;
                    }
                    break; // Bullet can only hit one saucer
                }
            }
        }

        // Apply collisions
        for k in 0..collision_count {
            let (bullet_idx, saucer_idx, is_small) = collisions[k];
            self.state.bullets[bullet_idx].life = 0;
            self.state.saucers[saucer_idx].alive = false;

            let score = if is_small {
                SCORE_SMALL_SAUCER
            } else {
                SCORE_LARGE_SAUCER
            };
            self.add_score(score);
        }
    }

    fn handle_ship_asteroid_collisions(&mut self) {
        let ship = &self.state.ship;

        if !ship.can_control || ship.invulnerable_timer > 0 {
            return;
        }

        for i in 0..self.state.asteroids.len() {
            if !self.state.asteroids[i].alive {
                continue;
            }

            let asteroid = &self.state.asteroids[i];
            let dist_sq = distance_sq_q12_4(ship.x, ship.y, asteroid.x, asteroid.y);

            // Fudge factor: 0.88x asteroid radius for ship collisions
            let asteroid_radius = ((asteroid.size.radius_q12_4() as u32 * 225) >> 8) as u16;
            let threshold = (SHIP_RADIUS_Q12_4 + asteroid_radius) as u32;
            let threshold_sq = threshold * threshold;

            if dist_sq < threshold_sq {
                self.destroy_ship();
                return;
            }
        }
    }

    fn handle_ship_saucer_collisions(&mut self) {
        let ship = &self.state.ship;

        if !ship.can_control || ship.invulnerable_timer > 0 {
            return;
        }

        for saucer in &self.state.saucers {
            if !saucer.alive {
                continue;
            }

            let radius = if saucer.small {
                SAUCER_RADIUS_SMALL_Q12_4
            } else {
                SAUCER_RADIUS_LARGE_Q12_4
            };

            let dist_sq = distance_sq_q12_4(ship.x, ship.y, saucer.x, saucer.y);
            let threshold = (SHIP_RADIUS_Q12_4 + radius) as u32;
            let threshold_sq = threshold * threshold;

            if dist_sq < threshold_sq {
                self.destroy_ship();
                return;
            }
        }
    }

    fn handle_ship_saucer_bullet_collisions(&mut self) {
        let ship = &self.state.ship;

        if !ship.can_control || ship.invulnerable_timer > 0 {
            return;
        }

        for bullet in &self.state.saucer_bullets {
            if bullet.life == 0 {
                continue;
            }

            let dist_sq = distance_sq_q12_4(ship.x, ship.y, bullet.x, bullet.y);
            let threshold = (SHIP_RADIUS_Q12_4 + BULLET_RADIUS_Q12_4) as u32;
            let threshold_sq = threshold * threshold;

            if dist_sq < threshold_sq {
                self.destroy_ship();
                return;
            }
        }
    }

    /// Handle saucer bullet-asteroid collisions using stack-allocated array
    /// Saucer bullets are more limited, max ~8
    fn handle_saucer_bullet_asteroid_collisions(&mut self) {
        // Stack-allocated array (saucer bullets typically limited, using 8 as safe upper bound)
        let mut collisions: [(usize, usize); 8] = [(0, 0); 8];
        let mut collision_count: usize = 0;

        for i in 0..self.state.saucer_bullets.len() {
            if self.state.saucer_bullets[i].life == 0 {
                continue;
            }

            for j in 0..self.state.asteroids.len() {
                if !self.state.asteroids[j].alive {
                    continue;
                }

                let bullet = &self.state.saucer_bullets[i];
                let asteroid = &self.state.asteroids[j];

                let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y);
                let threshold = (BULLET_RADIUS_Q12_4 + asteroid.size.radius_q12_4()) as u32;
                let threshold_sq = threshold * threshold;

                if dist_sq < threshold_sq {
                    if collision_count < 8 {
                        collisions[collision_count] = (i, j);
                        collision_count += 1;
                    }
                    break; // Bullet can only hit one asteroid
                }
            }
        }

        // Apply collisions
        for k in 0..collision_count {
            let (bullet_idx, asteroid_idx) = collisions[k];
            self.state.saucer_bullets[bullet_idx].life = 0;
            self.destroy_asteroid(asteroid_idx, false); // No score for saucer bullet hits
        }
    }

    /// Destroy asteroid and spawn children
    fn destroy_asteroid(&mut self, index: usize, score: bool) {
        let asteroid = &self.state.asteroids[index];
        let size = asteroid.size;
        let x = asteroid.x;
        let y = asteroid.y;
        let parent_vx = asteroid.vx;
        let parent_vy = asteroid.vy;

        // Mark as destroyed
        self.state.asteroids[index].alive = false;

        // Award score
        if score {
            self.add_score(size.score());
            // Reset lurk timer on player kills
            self.state.time_since_last_kill = 0;
        }

        // Spawn children if not smallest
        match size {
            AsteroidSize::Large => {
                self.spawn_asteroid_children(x, y, parent_vx, parent_vy, AsteroidSize::Medium);
            }
            AsteroidSize::Medium => {
                self.spawn_asteroid_children(x, y, parent_vx, parent_vy, AsteroidSize::Small);
            }
            AsteroidSize::Small => {
                // No children
            }
        }
    }

    /// Spawn children when asteroid splits
    fn spawn_asteroid_children(
        &mut self,
        x: u16,
        y: u16,
        parent_vx: i16,
        parent_vy: i16,
        child_size: AsteroidSize,
    ) {
        // Count alive asteroids
        let alive_count = self.state.asteroids.iter().filter(|a| a.alive).count();

        // If at cap, only spawn 1 child instead of 2
        let child_count = if alive_count >= ASTEROID_CAP as usize {
            1
        } else {
            2
        };

        for _ in 0..child_count {
            // Random angle for child velocity
            let angle = self.rng.next_angle();

            // Get speed based on size
            let base_speed = match child_size {
                AsteroidSize::Large => self.rng.next_range(
                    ASTEROID_SPEED_LARGE_Q8_8.0 as u32,
                    ASTEROID_SPEED_LARGE_Q8_8.1 as u32,
                ) as i16,
                AsteroidSize::Medium => self.rng.next_range(
                    ASTEROID_SPEED_MEDIUM_Q8_8.0 as u32,
                    ASTEROID_SPEED_MEDIUM_Q8_8.1 as u32,
                ) as i16,
                AsteroidSize::Small => self.rng.next_range(
                    ASTEROID_SPEED_SMALL_Q8_8.0 as u32,
                    ASTEROID_SPEED_SMALL_Q8_8.1 as u32,
                ) as i16,
            };

            // Apply wave speed multiplier
            let wave_bonus = ((self.state.wave - 1) * 15).min(128) as i32;
            let speed = base_speed + ((base_speed as i32 * wave_bonus) >> 8) as i16;

            let (vx, vy) = velocity_q8_8(angle, speed);

            // Inherit some velocity from parent: (parent.v * 46) >> 8 ~ 0.18
            let inherited_vx = ((parent_vx as i32 * 46) >> 8) as i16;
            let inherited_vy = ((parent_vy as i32 * 46) >> 8) as i16;
            let final_vx = vx.saturating_add(inherited_vx);
            let final_vy = vy.saturating_add(inherited_vy);

            self.state.asteroids.push(Asteroid {
                x,
                y,
                vx: final_vx,
                vy: final_vy,
                angle: self.rng.next_angle(),
                spin: self.rng.next_spin(),
                size: child_size,
                alive: true,
            });
        }
    }

    /// Destroy ship (player death)
    fn destroy_ship(&mut self) {
        let ship = &mut self.state.ship;
        ship.can_control = false;
        ship.respawn_timer = SHIP_RESPAWN_FRAMES;

        // Prevent underflow
        if self.state.lives > 0 {
            self.state.lives -= 1;
        }

        if self.state.lives == 0 {
            self.state.mode = GameMode::GameOver;
        }
    }

    /// Add score with extra life check
    fn add_score(&mut self, points: u32) {
        self.state.score += points;

        // Check for extra life
        while self.state.score >= self.state.next_extra_life_score {
            self.state.lives += 1;
            self.state.next_extra_life_score += EXTRA_LIFE_SCORE_STEP;
        }
    }

    /// Remove destroyed entities
    fn prune_destroyed(&mut self) {
        self.state.bullets.retain(|b| b.alive());
        self.state.asteroids.retain(|a| a.alive);
        self.state.saucers.retain(|s| s.alive);
        self.state.saucer_bullets.retain(|b| b.alive());
    }

    /// Verify final state matches expected values
    pub fn verify_final_state(&self, expected_score: u32, expected_rng_state: u32) -> bool {
        self.state.score == expected_score && self.rng.state() == expected_rng_state
    }

    /// Step simulation with rule checking
    /// Returns Ok(()) if no rules violated, Err(RuleViolation) otherwise
    pub fn step_with_rules_check(&mut self, input: FrameInput) -> Result<(), RuleViolation> {
        // Store previous state for comparison
        let prev_state = self.state.clone();

        // Execute the frame
        self.step(input);

        // Check invariants
        check_frame_invariants(&self.state, Some(&prev_state), self.state.frame_count)
    }

    /// Get current frame number
    pub fn frame_count(&self) -> u32 {
        self.state.frame_count
    }
}

impl Bullet {
    fn alive(&self) -> bool {
        self.life > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_initialization() {
        let engine = GameEngine::new(12345);
        let state = engine.state();

        assert_eq!(state.mode, GameMode::Playing);
        assert_eq!(state.score, 0);
        assert_eq!(state.lives, STARTING_LIVES);
        assert_eq!(state.wave, 1);
        assert_eq!(state.ship.x, SHIP_START_X_Q12_4);
        assert_eq!(state.ship.y, SHIP_START_Y_Q12_4);
        assert_eq!(state.asteroids.len(), 4); // Initial wave has 4 asteroids
    }

    #[test]
    fn test_ship_rotation() {
        let mut engine = GameEngine::new(12345);
        let start_angle = engine.state().ship.angle;

        // Turn right
        engine.step(FrameInput {
            left: false,
            right: true,
            thrust: false,
            fire: false,
        });
        assert_eq!(
            engine.state().ship.angle,
            add_bam(start_angle, SHIP_TURN_SPEED_BAM)
        );

        // Turn left
        engine.step(FrameInput {
            left: true,
            right: false,
            thrust: false,
            fire: false,
        });
        assert_eq!(engine.state().ship.angle, start_angle); // Back to original
    }

    #[test]
    fn test_bullet_limit() {
        let mut engine = GameEngine::new(12345);

        // Fire 4 bullets (the limit)
        for _ in 0..10 {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: true,
            });
            // Need to wait for cooldown
            for _ in 0..SHIP_BULLET_COOLDOWN_FRAMES {
                engine.step(FrameInput::default());
            }
        }

        // Should have at most 4 bullets
        assert!(engine.state().bullets.len() <= SHIP_BULLET_LIMIT as usize);
    }

    #[test]
    fn test_score_addition() {
        let mut engine = GameEngine::new(12345);
        let initial_score = engine.state().score;

        engine.add_score(SCORE_LARGE_ASTEROID);
        assert_eq!(engine.state().score, initial_score + SCORE_LARGE_ASTEROID);
    }

    #[test]
    fn test_extra_life() {
        let mut engine = GameEngine::new(12345);
        let initial_lives = engine.state().lives;

        // Score exactly 10000
        engine.add_score(EXTRA_LIFE_SCORE_STEP);
        assert_eq!(engine.state().lives, initial_lives + 1);
        assert_eq!(
            engine.state().next_extra_life_score,
            EXTRA_LIFE_SCORE_STEP * 2
        );
    }

    // =========================================================================
    // COLLISION DETECTION TESTS
    // =========================================================================

    #[test]
    fn test_bullet_asteroid_collision_detection() {
        // Test collision detection logic directly
        let asteroid = Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            size: AsteroidSize::Large,
            alive: true,
            ..Default::default()
        };

        let bullet = Bullet {
            x: 100 << 4, // Same position as asteroid
            y: 100 << 4,
            life: 30,
            ..Default::default()
        };

        // Calculate distance
        let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y);
        let threshold = (BULLET_RADIUS_Q12_4 + asteroid.size.radius_q12_4()) as u32;
        let threshold_sq = threshold * threshold;

        // Should collide (distance is 0, which is less than threshold)
        assert!(
            dist_sq < threshold_sq,
            "Bullet at same position should collide with asteroid"
        );

        // Test with bullet far away
        let bullet_far = Bullet {
            x: 500 << 4,
            y: 500 << 4,
            life: 30,
            ..Default::default()
        };

        let dist_sq_far = distance_sq_q12_4(bullet_far.x, bullet_far.y, asteroid.x, asteroid.y);
        assert!(
            dist_sq_far >= threshold_sq,
            "Bullet far away should not collide"
        );
    }

    #[test]
    fn test_ship_death_on_asteroid_collision() {
        let mut engine = GameEngine::new(12345);

        // Extract asteroid position first
        let asteroid_x = engine.state().asteroids[0].x;
        let asteroid_y = engine.state().asteroids[0].y;

        // Remove invulnerability and move ship
        engine.state_mut().ship.invulnerable_timer = 0;
        engine.state_mut().ship.can_control = true;
        engine.state_mut().ship.x = asteroid_x;
        engine.state_mut().ship.y = asteroid_y;

        // Step to process collision
        engine.step(FrameInput::default());

        // Ship should be dead
        assert!(
            !engine.state().ship.can_control,
            "Ship should lose control after collision"
        );
        assert!(
            engine.state().ship.respawn_timer > 0,
            "Ship should have respawn timer"
        );
    }

    #[test]
    fn test_invulnerability_prevents_death() {
        let mut engine = GameEngine::new(12345);

        // Extract asteroid position and lives first
        let asteroid_x = engine.state().asteroids[0].x;
        let asteroid_y = engine.state().asteroids[0].y;
        let lives_before = engine.state().lives;

        // Ensure ship is invulnerable and move to asteroid
        engine.state_mut().ship.invulnerable_timer = 60;
        engine.state_mut().ship.can_control = true;
        engine.state_mut().ship.x = asteroid_x;
        engine.state_mut().ship.y = asteroid_y;

        let lives_before = engine.state().lives;

        // Step to process collision
        engine.step(FrameInput::default());

        // Ship should still be alive
        assert!(
            engine.state().ship.can_control,
            "Invulnerable ship should survive collision"
        );
        assert_eq!(
            engine.state().lives,
            lives_before,
            "Lives should not decrease when invulnerable"
        );
    }

    // =========================================================================
    // WAVE PROGRESSION TESTS
    // =========================================================================

    #[test]
    fn test_wave_completion_spawns_next_wave() {
        let mut engine = GameEngine::new(12345);

        // Kill all asteroids
        for asteroid in engine.state_mut().asteroids.iter_mut() {
            asteroid.alive = false;
        }

        let wave_before = engine.state().wave;

        // Step to process wave completion
        engine.step(FrameInput::default());

        // Wave should increment
        assert_eq!(
            engine.state().wave,
            wave_before + 1,
            "Wave should increment after clearing"
        );
        // New asteroids should spawn
        let alive_count = engine.state().asteroids.iter().filter(|a| a.alive).count();
        assert!(alive_count > 0, "New asteroids should spawn");
    }

    #[test]
    fn test_wave_asteroid_count_increases() {
        // Test that higher waves spawn more asteroids (up to cap)
        let mut engine = GameEngine::new(12345);

        // Manually set to wave 5
        engine.state_mut().wave = 4; // Will become 5 after spawn

        // Clear and respawn
        engine.state_mut().asteroids.clear();
        engine.spawn_wave();

        let count = engine.state().asteroids.len();
        // Wave 5: min(16, 4 + (5-1)*2) = min(16, 12) = 12
        assert_eq!(count, 12, "Wave 5 should spawn 12 asteroids");
    }

    #[test]
    fn test_wave_cap_at_16_asteroids() {
        let mut engine = GameEngine::new(12345);

        // Set to wave 10 (would calculate to 4 + 18 = 22)
        engine.state_mut().wave = 9;
        engine.state_mut().asteroids.clear();
        engine.spawn_wave();

        let count = engine.state().asteroids.len();
        assert_eq!(count, 16, "Asteroid count should be capped at 16");
    }

    // =========================================================================
    // ASTEROID SPLITTING TESTS
    // =========================================================================

    #[test]
    fn test_large_asteroid_splits_into_medium() {
        let mut engine = GameEngine::new(12345);

        // Clear existing asteroids and add one large
        engine.state_mut().asteroids.clear();
        engine.state_mut().asteroids.push(Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            vx: 100,
            vy: 100,
            angle: 0,
            spin: 1,
            size: AsteroidSize::Large,
            alive: true,
        });

        let count_before = engine.state().asteroids.len();

        // Destroy the large asteroid
        engine.destroy_asteroid(0, false);

        // Should spawn 2 medium asteroids
        let new_asteroids: Vec<_> = engine
            .state()
            .asteroids
            .iter()
            .filter(|a| a.size == AsteroidSize::Medium && a.alive)
            .collect();

        assert_eq!(
            new_asteroids.len(),
            2,
            "Large asteroid should split into 2 medium"
        );
    }

    #[test]
    fn test_medium_asteroid_splits_into_small() {
        let mut engine = GameEngine::new(12345);

        // Add one medium asteroid
        engine.state_mut().asteroids.clear();
        engine.state_mut().asteroids.push(Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            vx: 100,
            vy: 100,
            angle: 0,
            spin: 1,
            size: AsteroidSize::Medium,
            alive: true,
        });

        // Destroy the medium asteroid
        engine.destroy_asteroid(0, false);

        // Should spawn 2 small asteroids
        let new_asteroids: Vec<_> = engine
            .state()
            .asteroids
            .iter()
            .filter(|a| a.size == AsteroidSize::Small && a.alive)
            .collect();

        assert_eq!(
            new_asteroids.len(),
            2,
            "Medium asteroid should split into 2 small"
        );
    }

    #[test]
    fn test_small_asteroid_does_not_split() {
        let mut engine = GameEngine::new(12345);

        // Add one small asteroid
        engine.state_mut().asteroids.clear();
        engine.state_mut().asteroids.push(Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            vx: 100,
            vy: 100,
            angle: 0,
            spin: 1,
            size: AsteroidSize::Small,
            alive: true,
        });

        let count_before = engine.state().asteroids.len();

        // Destroy the small asteroid
        engine.destroy_asteroid(0, false);

        // Should not spawn new asteroids
        let new_count = engine.state().asteroids.iter().filter(|a| a.alive).count();
        assert_eq!(new_count, 0, "Small asteroid should not split");
    }

    // =========================================================================
    // SCORING TESTS
    // =========================================================================

    #[test]
    fn test_score_large_asteroid() {
        let mut engine = GameEngine::new(12345);
        let score_before = engine.state().score;

        engine.add_score(SCORE_LARGE_ASTEROID);

        assert_eq!(engine.state().score, score_before + SCORE_LARGE_ASTEROID);
    }

    #[test]
    fn test_score_medium_asteroid() {
        let mut engine = GameEngine::new(12345);
        let score_before = engine.state().score;

        engine.add_score(SCORE_MEDIUM_ASTEROID);

        assert_eq!(engine.state().score, score_before + SCORE_MEDIUM_ASTEROID);
    }

    #[test]
    fn test_score_small_asteroid() {
        let mut engine = GameEngine::new(12345);
        let score_before = engine.state().score;

        engine.add_score(SCORE_SMALL_ASTEROID);

        assert_eq!(engine.state().score, score_before + SCORE_SMALL_ASTEROID);
    }

    #[test]
    fn test_score_saucers() {
        let mut engine = GameEngine::new(12345);

        // Test large saucer
        let score_before = engine.state().score;
        engine.add_score(SCORE_LARGE_SAUCER);
        assert_eq!(engine.state().score, score_before + SCORE_LARGE_SAUCER);

        // Test small saucer
        let score_before = engine.state().score;
        engine.add_score(SCORE_SMALL_SAUCER);
        assert_eq!(engine.state().score, score_before + SCORE_SMALL_SAUCER);
    }

    // =========================================================================
    // PHYSICS TESTS
    // =========================================================================

    #[test]
    fn test_ship_thrust_increases_velocity() {
        let mut engine = GameEngine::new(12345);

        let vx_before = engine.state().ship.vx;
        let vy_before = engine.state().ship.vy;

        // Apply thrust for several frames
        for _ in 0..10 {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: true,
                fire: false,
            });
        }

        // Velocity should have changed
        let vx_after = engine.state().ship.vx;
        let vy_after = engine.state().ship.vy;

        assert!(
            vx_after != vx_before || vy_after != vy_before,
            "Thrust should change ship velocity"
        );
    }

    #[test]
    fn test_ship_speed_clamped() {
        let mut engine = GameEngine::new(12345);

        // Apply thrust continuously
        for _ in 0..100 {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: true,
                fire: false,
            });
        }

        // Check speed is within limit
        let vx = engine.state().ship.vx as i32;
        let vy = engine.state().ship.vy as i32;
        let speed_sq = (vx * vx + vy * vy) as u32;

        assert!(
            speed_sq <= SHIP_MAX_SPEED_SQ_Q16_16,
            "Ship speed should be clamped to max"
        );
    }

    #[test]
    fn test_drag_reduces_velocity() {
        let mut engine = GameEngine::new(12345);

        // Give ship some velocity
        engine.state_mut().ship.vx = 500;
        engine.state_mut().ship.vy = 500;

        // Step without thrust
        for _ in 0..10 {
            engine.step(FrameInput::default());
        }

        // Velocity should decrease due to drag
        let vx = engine.state().ship.vx;
        let vy = engine.state().ship.vy;

        // Drag: v - (v >> 7) = v * 127/128 per frame
        // After 10 frames: v * (127/128)^10 ≈ v * 0.92
        assert!(vx < 500 || vy < 500, "Drag should reduce velocity");
    }

    // =========================================================================
    // EDGE CASE TESTS
    // =========================================================================

    #[test]
    fn test_bullet_wraps_around_world() {
        let mut engine = GameEngine::new(12345);

        // Create bullet at right edge moving right
        engine.state_mut().bullets.push(Bullet {
            x: WORLD_WIDTH_Q12_4 - 10,
            y: 100 << 4,
            vx: 500, // Fast moving right
            vy: 0,
            life: SHIP_BULLET_LIFETIME_FRAMES,
            from_saucer: false,
        });

        // Step several times
        for _ in 0..5 {
            engine.step(FrameInput::default());
        }

        // Bullet should have wrapped to left side
        let bullet = &engine.state().bullets[0];
        assert!(
            bullet.x < WORLD_WIDTH_Q12_4 / 2,
            "Bullet should wrap around world"
        );
    }

    #[test]
    fn test_asteroid_wraps_around_world() {
        let mut engine = GameEngine::new(12345);

        // Move asteroid to right edge
        engine.state_mut().asteroids[0].x = WORLD_WIDTH_Q12_4 - 10;
        engine.state_mut().asteroids[0].vx = 200; // Moving right

        // Step several times
        for _ in 0..5 {
            engine.step(FrameInput::default());
        }

        // Asteroid should have wrapped
        let asteroid = &engine.state().asteroids[0];
        assert!(
            asteroid.x < WORLD_WIDTH_Q12_4 / 2,
            "Asteroid should wrap around world"
        );
    }

    #[test]
    fn test_multiple_bullets_max_limit() {
        let mut engine = GameEngine::new(12345);

        // Fire many bullets rapidly
        for _ in 0..20 {
            engine.step(FrameInput {
                left: false,
                right: false,
                thrust: false,
                fire: true,
            });
            // Short cooldown to test limit enforcement
            for _ in 0..2 {
                engine.step(FrameInput::default());
            }
        }

        // Should never exceed limit
        assert!(
            engine.state().bullets.len() <= SHIP_BULLET_LIMIT as usize,
            "Should never exceed bullet limit"
        );
    }

    #[test]
    fn test_game_over_when_lives_depleted() {
        let mut engine = GameEngine::new(12345);

        // Set lives to 1
        engine.state_mut().lives = 1;
        engine.state_mut().ship.can_control = true;
        engine.state_mut().ship.invulnerable_timer = 0;

        // Move ship to asteroid to die
        let asteroid_pos = (engine.state().asteroids[0].x, engine.state().asteroids[0].y);
        engine.state_mut().ship.x = asteroid_pos.0;
        engine.state_mut().ship.y = asteroid_pos.1;

        // Die
        engine.step(FrameInput::default());

        // Should be game over
        assert_eq!(engine.state().lives, 0, "Should have 0 lives");
        // Ship should not respawn
        assert!(
            !engine.state().ship.can_control || engine.state().ship.respawn_timer == 0,
            "Should not respawn when out of lives"
        );
    }
}
