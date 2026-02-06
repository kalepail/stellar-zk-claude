use crate::constants::*;
use crate::fixed_point::*;
use crate::rng::Rng;
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
                let speed = match size {
                    AsteroidSize::Large => self.rng.next_range(145, 248) as i16,
                    AsteroidSize::Medium => self.rng.next_range(265, 401) as i16,
                    AsteroidSize::Small => self.rng.next_range(418, 606) as i16,
                };

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
        let speed = self.rng.next_range(145, 248) as i16;
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
        let ship = &mut self.state.ship;

        if ship.can_control {
            // Handle rotation
            let mut turn = 0i8;
            if input.left && !input.right {
                turn = -SHIP_TURN_SPEED_BAM;
            } else if input.right && !input.left {
                turn = SHIP_TURN_SPEED_BAM;
            }
            ship.angle = add_bam(ship.angle, turn);

            // Handle thrust
            if input.thrust {
                let cos_val = cos_bam(ship.angle);
                let sin_val = sin_bam(ship.angle);

                // Add thrust acceleration (Q8.8)
                let ax = mul_q8_8_by_q0_14(SHIP_THRUST_Q8_8, cos_val);
                let ay = mul_q8_8_by_q0_14(SHIP_THRUST_Q8_8, sin_val);

                ship.vx = ship.vx.saturating_add(ax);
                ship.vy = ship.vy.saturating_add(ay);
            }

            // Apply drag
            ship.vx = apply_drag_q8_8(ship.vx);
            ship.vy = apply_drag_q8_8(ship.vy);

            // Clamp speed
            let (vx, vy) = clamp_speed_q8_8(ship.vx, ship.vy);
            ship.vx = vx;
            ship.vy = vy;

            // Update position
            let dx = vel_to_pos_delta(ship.vx);
            let dy = vel_to_pos_delta(ship.vy);

            ship.x = wrap_q12_4(add_q12_4(ship.x, dx as u16), WORLD_WIDTH_Q12_4);
            ship.y = wrap_q12_4(add_q12_4(ship.y, dy as u16), WORLD_HEIGHT_Q12_4);

            // Handle firing
            let should_fire = if ship.fire_cooldown > 0 {
                ship.fire_cooldown -= 1;
                false
            } else {
                input.fire && self.state.bullets.len() < SHIP_BULLET_LIMIT as usize
            };

            if should_fire {
                let ship_for_bullet = self.state.ship.clone();
                self.spawn_bullet_from_ship(&ship_for_bullet);
                self.state.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
            }

            // Update invulnerability timer
            if ship.invulnerable_timer > 0 {
                ship.invulnerable_timer -= 1;
            }
        } else {
            // Ship is dead, handle respawn
            if ship.respawn_timer > 0 {
                ship.respawn_timer -= 1;

                if ship.respawn_timer == 0 {
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
        let offset = (SHIP_RADIUS_Q12_4 + (6 << 4)) as u16;
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

        // Collect bullets to spawn
        let mut bullets_to_spawn: Vec<(u16, u16, i16, i16, bool)> = Vec::new();

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
                let new_vy = self.rng.next_range(0, 256) as i16 - 128;
                saucer.vy = (new_vy * 3) >> 2; // Scale to reasonable speed
                saucer.drift_timer = self.rng.next_range(48, 120) as u16;
            }

            // Handle firing
            if saucer.fire_cooldown > 0 {
                saucer.fire_cooldown -= 1;
            } else {
                // Collect bullet info to spawn after loop
                let is_small = saucer.small;
                let saucer_x = saucer.x;
                let saucer_y = saucer.y;
                let (vx, vy) = self.calc_saucer_bullet_velocity(saucer_x, saucer_y, is_small);
                bullets_to_spawn.push((saucer_x, saucer_y, vx, vy, true));

                // Reset cooldown based on saucer size and lurk state
                let base_cooldown = if is_small { 30 } else { 60 };
                let lurk_factor = if self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES {
                    2
                } else {
                    1
                };
                saucer.fire_cooldown = (base_cooldown / lurk_factor) as u8;
            }
        }

        // Spawn collected bullets
        for (x, y, vx, vy, from_saucer) in bullets_to_spawn {
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

    /// Calculate saucer bullet velocity
    fn calc_saucer_bullet_velocity(
        &mut self,
        saucer_x: u16,
        saucer_y: u16,
        is_small: bool,
    ) -> (i16, i16) {
        if is_small {
            // Small saucer: aim at ship with some error
            let dx = shortest_delta_q12_4(saucer_x, self.state.ship.x, WORLD_WIDTH_Q12_4);
            let dy = shortest_delta_q12_4(saucer_y, self.state.ship.y, WORLD_HEIGHT_Q12_4);

            let base_angle = atan2_bam(dy, dx);

            // Add error based on lurk state
            let error_range = if self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES {
                8 // More accurate when lurking
            } else {
                24 // Less accurate normally
            };

            let error = self.rng.next_int(error_range * 2) as i16 - error_range as i16;
            let angle = add_bam(base_angle, error as i8);

            velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
        } else {
            // Large saucer: random direction
            let angle = self.rng.next_angle();
            velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
        }
    }

    /// Spawn a saucer
    fn spawn_saucer(&mut self) {
        // Determine size based on wave and lurk state
        let is_small = if self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES {
            // 90% small saucer when lurking
            self.rng.next_int(100) >= 10
        } else {
            // 20% small saucer normally
            self.rng.next_int(100) >= 80
        };

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
        let is_lurking = self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES;
        let min_frames = if is_lurking {
            LURK_SAUCER_SPAWN_FAST_FRAMES
        } else {
            SAUCER_SPAWN_MIN_FRAMES
        };

        self.state.saucer_spawn_timer =
            self.rng
                .next_range(min_frames as u32, SAUCER_SPAWN_MAX_FRAMES as u32) as u16;
    }

    /// Spawn saucer bullet
    fn spawn_saucer_bullet(&mut self, saucer: &Saucer) {
        let bullet_x = saucer.x;
        let bullet_y = saucer.y;

        let (bullet_vx, bullet_vy) = if saucer.small {
            // Small saucer: aim at ship with some error
            let dx = shortest_delta_q12_4(saucer.x, self.state.ship.x, WORLD_WIDTH_Q12_4);
            let dy = shortest_delta_q12_4(saucer.y, self.state.ship.y, WORLD_HEIGHT_Q12_4);

            let base_angle = atan2_bam(dy as i16, dx as i16);

            // Add error based on lurk state
            let error_range = if self.state.time_since_last_kill > LURK_TIME_THRESHOLD_FRAMES {
                8 // More accurate when lurking
            } else {
                24 // Less accurate normally
            };

            let error = self.rng.next_int(error_range * 2) as i16 - error_range as i16;
            let angle = add_bam(base_angle, error as i8);

            velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
        } else {
            // Large saucer: random direction
            let angle = self.rng.next_angle();
            velocity_q8_8(angle, SAUCER_BULLET_SPEED_Q8_8)
        };

        self.state.saucer_bullets.push(Bullet {
            x: bullet_x,
            y: bullet_y,
            vx: bullet_vx,
            vy: bullet_vy,
            life: SAUCER_BULLET_LIFETIME_FRAMES,
            from_saucer: true,
        });
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

    fn handle_bullet_asteroid_collisions(&mut self) {
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
                    // Collision!
                    self.state.bullets[i].life = 0; // Kill bullet
                    self.destroy_asteroid(j, true); // Score for player
                    break;
                }
            }
        }
    }

    fn handle_bullet_saucer_collisions(&mut self) {
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
                    self.state.bullets[i].life = 0;
                    self.state.saucers[j].alive = false;

                    let score = if saucer.small {
                        SCORE_SMALL_SAUCER
                    } else {
                        SCORE_LARGE_SAUCER
                    };
                    self.add_score(score);
                }
            }
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
            let asteroid_radius = (asteroid.size.radius_q12_4() * 225) >> 8;
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

    fn handle_saucer_bullet_asteroid_collisions(&mut self) {
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
                    self.state.saucer_bullets[i].life = 0;
                    self.destroy_asteroid(j, false); // No score for saucer bullet hits
                    break;
                }
            }
        }
    }

    /// Destroy asteroid and spawn children
    fn destroy_asteroid(&mut self, index: usize, score: bool) {
        let asteroid = &self.state.asteroids[index];
        let size = asteroid.size;
        let x = asteroid.x;
        let y = asteroid.y;

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
                self.spawn_asteroid_children(x, y, AsteroidSize::Medium);
            }
            AsteroidSize::Medium => {
                self.spawn_asteroid_children(x, y, AsteroidSize::Small);
            }
            AsteroidSize::Small => {
                // No children
            }
        }
    }

    /// Spawn children when asteroid splits
    fn spawn_asteroid_children(&mut self, x: u16, y: u16, child_size: AsteroidSize) {
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
            let speed = match child_size {
                AsteroidSize::Large => self.rng.next_range(145, 248) as i16,
                AsteroidSize::Medium => self.rng.next_range(265, 401) as i16,
                AsteroidSize::Small => self.rng.next_range(418, 606) as i16,
            };

            let (vx, vy) = velocity_q8_8(angle, speed);

            // Inherit some velocity from parent
            let parent = self
                .state
                .asteroids
                .iter()
                .find(|a| !a.alive)
                .map(|a| (a.vx, a.vy));
            let (final_vx, final_vy) = if let Some((pvx, pvy)) = parent {
                // Child velocity = parent velocity * 46/256 + random velocity
                let inherited_vx = ((pvx as i32 * 46) >> 8) as i16;
                let inherited_vy = ((pvy as i32 * 46) >> 8) as i16;
                (
                    vx.saturating_add(inherited_vx),
                    vy.saturating_add(inherited_vy),
                )
            } else {
                (vx, vy)
            };

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

        self.state.lives -= 1;

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
}
