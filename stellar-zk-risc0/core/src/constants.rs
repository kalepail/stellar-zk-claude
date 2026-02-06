// Constants for Asteroids ZK Verification
// Based on docs/games/asteroids/verification-rules.md and integer-math-reference.md

// Tape format constants
pub const TAPE_MAGIC: u32 = 0x5A4B5450; // "ZKTP"
pub const TAPE_VERSION: u8 = 1;
pub const HEADER_SIZE: usize = 16;
pub const FOOTER_SIZE: usize = 12;

// World dimensions
pub const WORLD_WIDTH: u16 = 960;
pub const WORLD_HEIGHT: u16 = 720;

// Q12.4 world dimensions (positions stored in Q12.4 format)
pub const WORLD_WIDTH_Q12_4: u16 = 15360; // 960 * 16
pub const WORLD_HEIGHT_Q12_4: u16 = 11520; // 720 * 16

// Ship constants
pub const SHIP_RADIUS_Q12_4: u16 = 224; // 14 * 16
pub const SHIP_TURN_SPEED_BAM: i8 = 3; // BAM units per frame
pub const SHIP_THRUST_Q8_8: i16 = 20;
pub const SHIP_MAX_SPEED_Q8_8: i16 = 1451; // 340/60 * 256
pub const SHIP_MAX_SPEED_SQ_Q16_16: u32 = 1451u32 * 1451u32;
pub const SHIP_RESPAWN_FRAMES: u16 = 75;
pub const SHIP_SPAWN_INVULNERABLE_FRAMES: u16 = 120;
pub const SHIP_BULLET_LIMIT: u8 = 4;
pub const SHIP_BULLET_COOLDOWN_FRAMES: u8 = 10;
pub const SHIP_BULLET_LIFETIME_FRAMES: u8 = 51;
pub const SHIP_BULLET_SPEED_Q8_8: i16 = 2219; // 520/60 * 256

// Starting ship position (center of world in Q12.4)
pub const SHIP_START_X_Q12_4: u16 = 7680; // 480 * 16
pub const SHIP_START_Y_Q12_4: u16 = 5760; // 360 * 16
pub const SHIP_START_ANGLE_BAM: u8 = 192; // Facing up (-90 degrees)

// Asteroid constants
pub const ASTEROID_CAP: u8 = 27;
pub const ASTEROID_RADIUS_LARGE_Q12_4: u16 = 768; // 48 * 16
pub const ASTEROID_RADIUS_MEDIUM_Q12_4: u16 = 448; // 28 * 16
pub const ASTEROID_RADIUS_SMALL_Q12_4: u16 = 256; // 16 * 16

// Saucer constants
pub const SAUCER_RADIUS_LARGE_Q12_4: u16 = 352; // 22 * 16
pub const SAUCER_RADIUS_SMALL_Q12_4: u16 = 256; // 16 * 16
pub const SAUCER_SPAWN_MIN_FRAMES: u16 = 420;
pub const SAUCER_SPAWN_MAX_FRAMES: u16 = 840;
pub const SAUCER_BULLET_LIFETIME_FRAMES: u8 = 84;
pub const SAUCER_BULLET_SPEED_Q8_8: i16 = 1195; // 280/60 * 256
pub const SAUCER_SPEED_SMALL_Q8_8: i16 = 405; // 95/60 * 256
pub const SAUCER_SPEED_LARGE_Q8_8: i16 = 299; // 70/60 * 256

// Bullet constants
pub const BULLET_RADIUS_Q12_4: u16 = 32; // 2 * 16

// Scoring
pub const SCORE_LARGE_ASTEROID: u32 = 20;
pub const SCORE_MEDIUM_ASTEROID: u32 = 50;
pub const SCORE_SMALL_ASTEROID: u32 = 100;
pub const SCORE_LARGE_SAUCER: u32 = 200;
pub const SCORE_SMALL_SAUCER: u32 = 1000;

// Game constants
pub const STARTING_LIVES: u8 = 3;
pub const EXTRA_LIFE_SCORE_STEP: u32 = 10000;

// Anti-lurking
pub const LURK_TIME_THRESHOLD_FRAMES: u16 = 360; // 6 seconds
pub const LURK_SAUCER_SPAWN_FAST_FRAMES: u16 = 180; // 3 seconds

// Spawn safe distance (120 pixels in Q12.4)
pub const SPAWN_SAFE_DISTANCE_Q12_4: u16 = 1920; // 120 * 16

// Initial wave asteroid count
pub const INITIAL_WAVE_ASTEROID_COUNT: u8 = 4;

// Max frames for verification (5 minutes at 60fps)
pub const MAX_FRAMES: u32 = 18000;
