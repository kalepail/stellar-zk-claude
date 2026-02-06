//! Game constants - exact match to TypeScript constants.ts
//!
//! All values are verified against the TypeScript source.

use crate::types::AsteroidSize;

// World dimensions
pub const WORLD_WIDTH: i32 = 960;
pub const WORLD_HEIGHT: i32 = 720;

// Starting state
pub const STARTING_LIVES: i32 = 3;
pub const EXTRA_LIFE_SCORE_STEP: u32 = 10000;

// Ship
pub const SHIP_RADIUS: i32 = 14;

// Frame-count timer constants (integer, ZK-friendly)
pub const SHIP_RESPAWN_FRAMES: i32 = 75;           // 1.25s * 60fps
pub const SHIP_SPAWN_INVULNERABLE_FRAMES: i32 = 120; // 2s * 60fps
pub const SHIP_BULLET_LIFETIME_FRAMES: i32 = 51;   // 0.85s * 60fps
pub const SHIP_BULLET_COOLDOWN_FRAMES: i32 = 10;   // 0.16s * 60fps ≈ 9.6 → 10
pub const SHIP_BULLET_LIMIT: usize = 4;
pub const SAUCER_BULLET_LIFETIME_FRAMES: i32 = 84; // 1.4s * 60fps
pub const SAUCER_SPAWN_MIN_FRAMES: i32 = 420;      // 7s * 60fps
pub const SAUCER_SPAWN_MAX_FRAMES: i32 = 840;      // 14s * 60fps

// Q12.4 world dimensions
pub const WORLD_WIDTH_Q12_4: i32 = 15360;  // 960 * 16
pub const WORLD_HEIGHT_Q12_4: i32 = 11520; // 720 * 16

// Q8.8 velocity constants
pub const SHIP_THRUST_Q8_8: i32 = 20;      // 280/3600 * 256 ~ 19.9
pub const SHIP_MAX_SPEED_Q8_8: i32 = 1451; // 340/60 * 256
pub const SHIP_MAX_SPEED_SQ_Q16_16: i32 = 1451 * 1451;

// BAM angle constants
pub const SHIP_TURN_SPEED_BAM: i32 = 3;    // 4.8 rad/s / 60 / (2π/256) ~ 3.26 → 3
pub const SHIP_FACING_UP_BAM: u8 = 192;    // -90° in BAM

// Q8.8 bullet speeds
pub const SHIP_BULLET_SPEED_Q8_8: i32 = 2219;   // 520/60 * 256
pub const SAUCER_BULLET_SPEED_Q8_8: i32 = 1195;  // 280/60 * 256

// Q8.8 asteroid speed ranges [min, max)
pub const ASTEROID_SPEED_LARGE_Q8_8: (i32, i32) = (145, 248);
pub const ASTEROID_SPEED_MEDIUM_Q8_8: (i32, i32) = (265, 401);
pub const ASTEROID_SPEED_SMALL_Q8_8: (i32, i32) = (418, 606);

// Q8.8 saucer speeds
pub const SAUCER_SPEED_SMALL_Q8_8: i32 = 405;  // 95/60 * 256
pub const SAUCER_SPEED_LARGE_Q8_8: i32 = 299;  // 70/60 * 256

// Asteroid sizes (pixel radii)
pub const ASTEROID_RADIUS_LARGE: i32 = 48;
pub const ASTEROID_RADIUS_MEDIUM: i32 = 28;
pub const ASTEROID_RADIUS_SMALL: i32 = 16;

// Saucer sizes
pub const SAUCER_RADIUS_LARGE: i32 = 22;
pub const SAUCER_RADIUS_SMALL: i32 = 16;

// Scoring
pub const SCORE_LARGE_ASTEROID: u32 = 20;
pub const SCORE_MEDIUM_ASTEROID: u32 = 50;
pub const SCORE_SMALL_ASTEROID: u32 = 100;
pub const SCORE_LARGE_SAUCER: u32 = 200;
pub const SCORE_SMALL_SAUCER: u32 = 1000;

// Asteroid cap
pub const ASTEROID_CAP: usize = 27;

// Anti-lurking
pub const LURK_TIME_THRESHOLD_FRAMES: i32 = 360;         // 6s * 60fps
pub const LURK_SAUCER_SPAWN_FAST_FRAMES: i32 = 180;      // 3s * 60fps

/// Get asteroid speed range for a given size.
pub fn asteroid_speed_range(size: AsteroidSize) -> (i32, i32) {
    match size {
        AsteroidSize::Large => ASTEROID_SPEED_LARGE_Q8_8,
        AsteroidSize::Medium => ASTEROID_SPEED_MEDIUM_Q8_8,
        AsteroidSize::Small => ASTEROID_SPEED_SMALL_Q8_8,
    }
}

/// Get asteroid radius for a given size.
pub fn asteroid_radius(size: AsteroidSize) -> i32 {
    match size {
        AsteroidSize::Large => ASTEROID_RADIUS_LARGE,
        AsteroidSize::Medium => ASTEROID_RADIUS_MEDIUM,
        AsteroidSize::Small => ASTEROID_RADIUS_SMALL,
    }
}
