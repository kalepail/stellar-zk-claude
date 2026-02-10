use super::*;

#[derive(Clone)]
pub(super) struct Game {
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
    prune_mask: u8,
    saucer_spawn_timer: i32,
    ship_fire_latch: bool,
    time_since_last_kill: i32,
    frame_count: u32,
    rng: SeededRng,
}

const PRUNE_ASTEROIDS: u8 = 1 << 0;
const PRUNE_BULLETS: u8 = 1 << 1;
const PRUNE_SAUCERS: u8 = 1 << 2;
const PRUNE_SAUCER_BULLETS: u8 = 1 << 3;

impl Game {
    pub(super) fn new(seed: u32) -> Self {
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
            prune_mask: 0,
            saucer_spawn_timer: 0,
            ship_fire_latch: false,
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

    pub(super) fn checkpoint(&self) -> ReplayCheckpoint {
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

    pub(super) fn world_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            frame_count: self.frame_count,
            score: self.score,
            lives: self.lives,
            wave: self.wave,
            is_game_over: matches!(self.mode, GameMode::GameOver),
            rng_state: self.rng.state(),
            saucer_spawn_timer: self.saucer_spawn_timer,
            time_since_last_kill: self.time_since_last_kill,
            next_extra_life_score: self.next_extra_life_score,
            ship: Self::ship_snapshot(self.ship),
            asteroids: self
                .asteroids
                .iter()
                .map(|entry| Self::asteroid_snapshot(*entry))
                .collect(),
            bullets: self
                .bullets
                .iter()
                .map(|entry| Self::bullet_snapshot(*entry))
                .collect(),
            saucers: self
                .saucers
                .iter()
                .map(|entry| Self::saucer_snapshot(*entry))
                .collect(),
            saucer_bullets: self
                .saucer_bullets
                .iter()
                .map(|entry| Self::bullet_snapshot(*entry))
                .collect(),
        }
    }

    #[inline]
    fn ship_snapshot(ship: Ship) -> ShipSnapshot {
        ShipSnapshot {
            x: ship.x,
            y: ship.y,
            vx: ship.vx,
            vy: ship.vy,
            angle: ship.angle,
            radius: ship.radius,
            can_control: ship.can_control,
            fire_cooldown: ship.fire_cooldown,
            respawn_timer: ship.respawn_timer,
            invulnerable_timer: ship.invulnerable_timer,
        }
    }

    #[inline]
    fn asteroid_snapshot(asteroid: Asteroid) -> AsteroidSnapshot {
        let size = match asteroid.size {
            AsteroidSize::Large => AsteroidSizeSnapshot::Large,
            AsteroidSize::Medium => AsteroidSizeSnapshot::Medium,
            AsteroidSize::Small => AsteroidSizeSnapshot::Small,
        };

        AsteroidSnapshot {
            x: asteroid.x,
            y: asteroid.y,
            vx: asteroid.vx,
            vy: asteroid.vy,
            angle: asteroid.angle,
            alive: asteroid.alive,
            radius: asteroid.radius,
            size,
            spin: asteroid.spin,
        }
    }

    #[inline]
    fn bullet_snapshot(bullet: Bullet) -> BulletSnapshot {
        BulletSnapshot {
            x: bullet.x,
            y: bullet.y,
            vx: bullet.vx,
            vy: bullet.vy,
            alive: bullet.alive,
            radius: bullet.radius,
            life: bullet.life,
        }
    }

    #[inline]
    fn saucer_snapshot(saucer: Saucer) -> SaucerSnapshot {
        SaucerSnapshot {
            x: saucer.x,
            y: saucer.y,
            vx: saucer.vx,
            vy: saucer.vy,
            alive: saucer.alive,
            radius: saucer.radius,
            small: saucer.small,
            fire_cooldown: saucer.fire_cooldown,
            drift_timer: saucer.drift_timer,
        }
    }

    pub(super) fn transition_state(&self) -> TransitionState {
        TransitionState {
            frame_count: self.frame_count,
            score: self.score,
            wave: self.wave,
            asteroids: self.asteroids.len(),
            bullets: self.bullets.len(),
            saucers: self.saucers.len(),
            ship_x: self.ship.x,
            ship_y: self.ship.y,
            ship_vx: self.ship.vx,
            ship_vy: self.ship.vy,
            ship_angle: self.ship.angle,
            ship_can_control: self.ship.can_control,
            ship_fire_cooldown: self.ship.fire_cooldown,
            ship_fire_latch: self.ship_fire_latch,
            ship_respawn_timer: self.ship.respawn_timer,
        }
    }

    #[inline]
    pub(super) fn frame_count(&self) -> u32 {
        self.frame_count
    }

    #[inline]
    pub(super) fn result(&self) -> ReplayResult {
        ReplayResult {
            final_score: self.score,
            final_rng_state: self.rng.state(),
            frame_count: self.frame_count,
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

    pub(super) fn step(&mut self, input_byte: u8) {
        self.step_decoded(decode_input_byte(input_byte));
    }

    pub(super) fn step_decoded(&mut self, input: FrameInput) {
        self.frame_count += 1;

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

    pub(super) fn validate_invariants(&self) -> Result<(), RuleCode> {
        let world_width = WORLD_WIDTH_Q12_4 as u32;
        let world_height = WORLD_HEIGHT_Q12_4 as u32;

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
            && self.next_extra_life_score >= EXTRA_LIFE_SCORE_STEP;
        if !next_extra_life_valid {
            return Err(RuleCode::GlobalNextExtraLifeScore);
        }
        debug_assert!(self
            .next_extra_life_score
            .is_multiple_of(EXTRA_LIFE_SCORE_STEP));

        if (self.ship.x as u32) >= world_width || (self.ship.y as u32) >= world_height {
            return Err(RuleCode::ShipBounds);
        }

        if (self.ship.angle & !0xff) != 0 {
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

        if self.saucer_bullets.len() > SAUCER_BULLET_LIMIT {
            return Err(RuleCode::SaucerBulletLimit);
        }

        debug_assert!(self.bullets.iter().all(|entry| entry.alive));
        debug_assert!(self.saucer_bullets.iter().all(|entry| entry.alive));
        debug_assert!(self.asteroids.iter().all(|entry| entry.alive));
        debug_assert!(self.saucers.iter().all(|entry| entry.alive));

        for bullet in &self.bullets {
            let in_bounds = (bullet.x as u32) < world_width && (bullet.y as u32) < world_height;
            if bullet.life <= 0 || !in_bounds {
                return Err(RuleCode::PlayerBulletState);
            }
        }

        for bullet in &self.saucer_bullets {
            let in_bounds = (bullet.x as u32) < world_width && (bullet.y as u32) < world_height;
            if bullet.life <= 0 || !in_bounds {
                return Err(RuleCode::SaucerBulletState);
            }
        }

        for asteroid in &self.asteroids {
            let in_bounds = (asteroid.x as u32) < world_width && (asteroid.y as u32) < world_height;
            if !in_bounds || (asteroid.angle & !0xff) != 0 {
                return Err(RuleCode::AsteroidState);
            }
        }

        if self.saucers.len() > max_saucers_for_wave(self.wave) as usize {
            return Err(RuleCode::SaucerCap);
        }

        for saucer in &self.saucers {
            if saucer.x < SAUCER_CULL_MIN_X_Q12_4
                || saucer.x > SAUCER_CULL_MAX_X_Q12_4
                || (saucer.y as u32) >= world_height
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
        self.ship_fire_latch = false;
    }

    fn spawn_safety_score(&self, spawn_x: i32, spawn_y: i32, best_known_safety_score: i32) -> i32 {
        debug_assert!(self.asteroids.iter().all(|entry| entry.alive));
        debug_assert!(self.saucers.iter().all(|entry| entry.alive));
        debug_assert!(self.bullets.iter().all(|entry| entry.alive));
        debug_assert!(self.saucer_bullets.iter().all(|entry| entry.alive));

        let mut min_clearance_sq = i32::MAX;

        for asteroid in &self.asteroids {
            min_clearance_sq = core::cmp::min(
                min_clearance_sq,
                clearance_sq_q12_4(
                    asteroid.x,
                    asteroid.y,
                    asteroid.radius,
                    spawn_x,
                    spawn_y,
                    self.ship.radius,
                ),
            );
            if min_clearance_sq < best_known_safety_score {
                return min_clearance_sq;
            }
        }

        for saucer in &self.saucers {
            min_clearance_sq = core::cmp::min(
                min_clearance_sq,
                clearance_sq_q12_4(
                    saucer.x,
                    saucer.y,
                    saucer.radius,
                    spawn_x,
                    spawn_y,
                    self.ship.radius,
                ),
            );
            if min_clearance_sq < best_known_safety_score {
                return min_clearance_sq;
            }
        }

        for bullet in &self.bullets {
            min_clearance_sq = core::cmp::min(
                min_clearance_sq,
                clearance_sq_q12_4(
                    bullet.x,
                    bullet.y,
                    bullet.radius,
                    spawn_x,
                    spawn_y,
                    self.ship.radius,
                ),
            );
            if min_clearance_sq < best_known_safety_score {
                return min_clearance_sq;
            }
        }

        for bullet in &self.saucer_bullets {
            min_clearance_sq = core::cmp::min(
                min_clearance_sq,
                clearance_sq_q12_4(
                    bullet.x,
                    bullet.y,
                    bullet.radius,
                    spawn_x,
                    spawn_y,
                    self.ship.radius,
                ),
            );
            if min_clearance_sq < best_known_safety_score {
                return min_clearance_sq;
            }
        }

        min_clearance_sq
    }

    fn find_best_ship_spawn_point(&self) -> (i32, i32) {
        let min_x = SHIP_RESPAWN_EDGE_PADDING_Q12_4;
        let max_x = WORLD_WIDTH_Q12_4 - SHIP_RESPAWN_EDGE_PADDING_Q12_4;
        let min_y = SHIP_RESPAWN_EDGE_PADDING_Q12_4;
        let max_y = WORLD_HEIGHT_Q12_4 - SHIP_RESPAWN_EDGE_PADDING_Q12_4;
        let (center_x, center_y) = self.get_ship_spawn_point();

        let mut best_x = center_x;
        let mut best_y = center_y;
        let mut best_safety_score = i32::MIN;
        let mut best_center_distance = i32::MAX;
        let mut y = min_y;

        while y <= max_y {
            let mut x = min_x;
            while x <= max_x {
                let safety_score = self.spawn_safety_score(x, y, best_safety_score);
                let center_distance = collision_dist_sq_q12_4(x, y, center_x, center_y);
                if safety_score > best_safety_score
                    || (safety_score == best_safety_score && center_distance < best_center_distance)
                {
                    best_x = x;
                    best_y = y;
                    best_safety_score = safety_score;
                    best_center_distance = center_distance;
                }
                x += SHIP_RESPAWN_GRID_STEP_Q12_4;
            }
            y += SHIP_RESPAWN_GRID_STEP_Q12_4;
        }

        (best_x, best_y)
    }

    fn spawn_ship_at_best_open_point(&mut self) {
        let (spawn_x, spawn_y) = self.find_best_ship_spawn_point();

        self.ship.x = spawn_x;
        self.ship.y = spawn_y;
        self.ship.vx = 0;
        self.ship.vy = 0;
        self.ship.angle = 192;
        self.ship.can_control = true;
        self.ship.invulnerable_timer = SHIP_SPAWN_INVULNERABLE_FRAMES;
    }

    fn spawn_wave(&mut self) {
        self.wave += 1;
        self.time_since_last_kill = 0;

        let large_count = wave_asteroid_count(self.wave);
        let (avoid_x, avoid_y) = self.get_ship_spawn_point();

        for _ in 0..large_count {
            let mut x = self.random_int(0, WORLD_WIDTH_Q12_4);
            let mut y = self.random_int(0, WORLD_HEIGHT_Q12_4);
            let mut guard = 0;

            while guard < 20
                && collision_dist_sq_q12_4(x, y, avoid_x, avoid_y) < WAVE_SAFE_DIST_SQ_Q24_8
            {
                x = self.random_int(0, WORLD_WIDTH_Q12_4);
                y = self.random_int(0, WORLD_HEIGHT_Q12_4);
                guard += 1;
            }

            let asteroid = self.create_asteroid(AsteroidSize::Large, x, y);
            self.asteroids.push(asteroid);
        }

        self.queue_ship_respawn(0);
        self.spawn_ship_at_best_open_point();
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

        if !fire {
            self.ship_fire_latch = false;
        }

        if !self.ship.can_control {
            if self.ship.respawn_timer > 0 {
                self.ship.respawn_timer -= 1;
            }

            if self.ship.respawn_timer <= 0 {
                self.spawn_ship_at_best_open_point();
            }

            if fire {
                self.ship_fire_latch = true;
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

        let fire_pressed_this_frame = fire && !self.ship_fire_latch;
        if fire_pressed_this_frame
            && self.ship.fire_cooldown <= 0
            && self.bullets.len() < SHIP_BULLET_LIMIT
        {
            self.spawn_ship_bullet();
            self.ship.fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
        }
        if fire {
            self.ship_fire_latch = true;
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
        debug_assert!(self.asteroids.iter().all(|entry| entry.alive));
        for asteroid in &mut self.asteroids {
            asteroid.x = wrap_x_q12_4(asteroid.x + (asteroid.vx >> 4));
            asteroid.y = wrap_y_q12_4(asteroid.y + (asteroid.vy >> 4));
            asteroid.angle = (asteroid.angle + asteroid.spin) & 0xff;
        }
    }

    fn update_bullets(&mut self) {
        if Self::update_projectiles(&mut self.bullets) {
            self.prune_mask |= PRUNE_BULLETS;
        }
    }

    fn update_saucer_bullets(&mut self) {
        if Self::update_projectiles(&mut self.saucer_bullets) {
            self.prune_mask |= PRUNE_SAUCER_BULLETS;
        }
    }

    fn update_projectiles(projectiles: &mut [Bullet]) -> bool {
        debug_assert!(projectiles.iter().all(|entry| entry.alive));
        let mut killed_any = false;
        for bullet in projectiles {
            bullet.life -= 1;
            if bullet.life <= 0 {
                bullet.alive = false;
                killed_any = true;
                continue;
            }

            bullet.x = wrap_x_q12_4(bullet.x + (bullet.vx >> 4));
            bullet.y = wrap_y_q12_4(bullet.y + (bullet.vy >> 4));
        }

        killed_any
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

        if self.saucers.len() < max_saucers as usize && self.saucer_spawn_timer <= spawn_threshold {
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

        let saucer_count = self.saucers.len();
        for index in 0..saucer_count {
            let mut killed_this_frame = false;
            {
                let saucer = &mut self.saucers[index];
                saucer.x += saucer.vx >> 4;
                saucer.y = wrap_y_q12_4(saucer.y + (saucer.vy >> 4));

                if saucer.x < SAUCER_CULL_MIN_X_Q12_4 || saucer.x > SAUCER_CULL_MAX_X_Q12_4 {
                    saucer.alive = false;
                    killed_this_frame = true;
                } else if saucer.drift_timer > 0 {
                    saucer.drift_timer -= 1;
                }
            }

            if killed_this_frame {
                self.prune_mask |= PRUNE_SAUCERS;
                continue;
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
                let (sx, sy, sr, small) = {
                    let saucer = &self.saucers[index];
                    (saucer.x, saucer.y, saucer.radius, saucer.small)
                };
                self.spawn_saucer_bullet(sx, sy, sr, small);
                let (min_cooldown, max_cooldown) =
                    saucer_fire_cooldown_range(small, self.wave, self.time_since_last_kill);
                self.saucers[index].fire_cooldown = self.random_int(min_cooldown, max_cooldown + 1);
            }
        }
    }

    fn spawn_saucer(&mut self) {
        let enter_from_left = (self.rng.next() & 1) == 0;
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
        let (cooldown_min, cooldown_max) =
            saucer_fire_cooldown_range(small, self.wave, self.time_since_last_kill);
        let fire_cooldown = self.random_int(cooldown_min, cooldown_max + 1);
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

    fn spawn_saucer_bullet(
        &mut self,
        saucer_x: i32,
        saucer_y: i32,
        saucer_radius: i32,
        saucer_small: bool,
    ) {
        if self.saucer_bullets.len() >= SAUCER_BULLET_LIMIT {
            return;
        }

        let shot_angle = if saucer_small {
            let dx = shortest_delta_q12_4(saucer_x, self.ship.x, WORLD_WIDTH_Q12_4);
            let dy = shortest_delta_q12_4(saucer_y, self.ship.y, WORLD_HEIGHT_Q12_4);
            let target_angle = atan2_bam(dy, dx);
            let error_bam = small_saucer_aim_error_bam(self.wave, self.time_since_last_kill);
            (target_angle + self.random_int(-error_bam, error_bam + 1)) & 0xff
        } else {
            self.random_int(0, 256)
        };

        let (vx, vy) = velocity_q8_8(shot_angle, SAUCER_BULLET_SPEED_Q8_8);
        let (off_dx, off_dy) = displace_q12_4(shot_angle, saucer_radius + 4);
        let start_x = wrap_x_q12_4(saucer_x + off_dx);
        let start_y = wrap_y_q12_4(saucer_y + off_dy);

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
        let mut alive_asteroids = self.asteroids.len();
        debug_assert!(self.asteroids.iter().all(|entry| entry.alive));

        for bullet_index in 0..self.bullets.len() {
            if alive_asteroids == 0 {
                break;
            }
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

                let (ax, ay, ar) = {
                    let asteroid = &self.asteroids[asteroid_index];
                    (asteroid.x, asteroid.y, asteroid.radius)
                };
                if collides_q12_4(bx, by, br, ax, ay, ar) {
                    self.bullets[bullet_index].alive = false;
                    self.prune_mask |= PRUNE_BULLETS;
                    self.destroy_asteroid(asteroid_index, true, &mut alive_asteroids);
                    break;
                }
            }
        }

        for bullet_index in 0..self.saucer_bullets.len() {
            if alive_asteroids == 0 {
                break;
            }
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

                let (ax, ay, ar) = {
                    let asteroid = &self.asteroids[asteroid_index];
                    (asteroid.x, asteroid.y, asteroid.radius)
                };
                if collides_q12_4(bx, by, br, ax, ay, ar) {
                    self.saucer_bullets[bullet_index].alive = false;
                    self.prune_mask |= PRUNE_SAUCER_BULLETS;
                    self.destroy_asteroid(asteroid_index, false, &mut alive_asteroids);
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

                let (sx, sy, sr, small) = {
                    let saucer = &self.saucers[saucer_index];
                    (saucer.x, saucer.y, saucer.radius, saucer.small)
                };
                if collides_q12_4(bx, by, br, sx, sy, sr) {
                    self.bullets[bullet_index].alive = false;
                    self.saucers[saucer_index].alive = false;
                    self.prune_mask |= PRUNE_BULLETS | PRUNE_SAUCERS;
                    self.add_score(if small {
                        SCORE_SMALL_SAUCER
                    } else {
                        SCORE_LARGE_SAUCER
                    });
                    break;
                }
            }
        }

        if alive_asteroids > 0 {
            for saucer_index in 0..self.saucers.len() {
                if !self.saucers[saucer_index].alive {
                    continue;
                }

                let (sx, sy, sr) = {
                    let saucer = &self.saucers[saucer_index];
                    (saucer.x, saucer.y, saucer.radius)
                };
                for asteroid in &self.asteroids {
                    if !asteroid.alive {
                        continue;
                    }

                    if collides_q12_4(
                        sx,
                        sy,
                        sr,
                        asteroid.x,
                        asteroid.y,
                        asteroid.radius,
                    ) {
                        self.saucers[saucer_index].alive = false;
                        self.prune_mask |= PRUNE_SAUCERS;
                        break;
                    }
                }
            }
        }

        if !self.ship.can_control || self.ship.invulnerable_timer > 0 {
            return;
        }

        if alive_asteroids > 0 {
            for asteroid in &self.asteroids {
                if !asteroid.alive {
                    continue;
                }

                let adjusted_radius = (asteroid.radius * 225) >> 8;
                if collides_q12_4(
                    self.ship.x,
                    self.ship.y,
                    self.ship.radius,
                    asteroid.x,
                    asteroid.y,
                    adjusted_radius,
                ) {
                    self.destroy_ship();
                    return;
                }
            }
        }

        for bullet in &mut self.saucer_bullets {
            if !bullet.alive {
                continue;
            }

            if collides_q12_4(
                self.ship.x,
                self.ship.y,
                self.ship.radius,
                bullet.x,
                bullet.y,
                bullet.radius,
            ) {
                bullet.alive = false;
                self.prune_mask |= PRUNE_SAUCER_BULLETS;
                self.destroy_ship();
                return;
            }
        }

        for saucer in &mut self.saucers {
            if !saucer.alive {
                continue;
            }

            if collides_q12_4(
                self.ship.x,
                self.ship.y,
                self.ship.radius,
                saucer.x,
                saucer.y,
                saucer.radius,
            ) {
                saucer.alive = false;
                self.prune_mask |= PRUNE_SAUCERS;
                self.destroy_ship();
                return;
            }
        }
    }

    fn destroy_asteroid(
        &mut self,
        asteroid_index: usize,
        award_score: bool,
        alive_asteroids: &mut usize,
    ) {
        if asteroid_index >= self.asteroids.len() {
            return;
        }

        let (size, x, y, vx, vy) = {
            let asteroid = &mut self.asteroids[asteroid_index];
            if !asteroid.alive {
                return;
            }
            asteroid.alive = false;
            *alive_asteroids = alive_asteroids.saturating_sub(1);
            (
                asteroid.size,
                asteroid.x,
                asteroid.y,
                asteroid.vx,
                asteroid.vy,
            )
        };
        self.prune_mask |= PRUNE_ASTEROIDS;

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

        let free_slots = ASTEROID_CAP.saturating_sub(*alive_asteroids);
        let split_count = core::cmp::min(2, free_slots);

        for _ in 0..split_count {
            let mut child = self.create_asteroid(child_size, x, y);
            child.vx += (vx * 46) >> 8;
            child.vy += (vy * 46) >> 8;
            self.asteroids.push(child);
            *alive_asteroids += 1;
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
        if self.prune_mask == 0 {
            return;
        }

        if (self.prune_mask & PRUNE_ASTEROIDS) != 0 {
            self.asteroids.retain(|entry| entry.alive);
        }
        if (self.prune_mask & PRUNE_BULLETS) != 0 {
            self.bullets.retain(|entry| entry.alive);
        }
        if (self.prune_mask & PRUNE_SAUCERS) != 0 {
            self.saucers.retain(|entry| entry.alive);
        }
        if (self.prune_mask & PRUNE_SAUCER_BULLETS) != 0 {
            self.saucer_bullets.retain(|entry| entry.alive);
        }

        self.prune_mask = 0;
    }
}

#[inline]
fn collides_q12_4(ax: i32, ay: i32, ar: i32, bx: i32, by: i32, br: i32) -> bool {
    let hit_dist_q12_4 = (ar + br) << 4;
    let neg_hit_dist_q12_4 = -hit_dist_q12_4;
    let dx = shortest_delta_q12_4(ax, bx, WORLD_WIDTH_Q12_4);
    if dx < neg_hit_dist_q12_4 || dx > hit_dist_q12_4 {
        return false;
    }
    let dy = shortest_delta_q12_4(ay, by, WORLD_HEIGHT_Q12_4);
    if dy < neg_hit_dist_q12_4 || dy > hit_dist_q12_4 {
        return false;
    }
    let hit_dist_sq_q24_8 = hit_dist_q12_4 * hit_dist_q12_4;
    (dx * dx + dy * dy) <= hit_dist_sq_q24_8
}

#[inline]
fn clearance_sq_q12_4(hx: i32, hy: i32, hr: i32, sx: i32, sy: i32, sr: i32) -> i32 {
    let hit_dist_q12_4 = (hr + sr) << 4;
    let dx = shortest_delta_q12_4(hx, sx, WORLD_WIDTH_Q12_4);
    let dy = shortest_delta_q12_4(hy, sy, WORLD_HEIGHT_Q12_4);
    (dx * dx + dy * dy) - (hit_dist_q12_4 * hit_dist_q12_4)
}

#[cfg(test)]
mod tests;
