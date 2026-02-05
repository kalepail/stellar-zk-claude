export const WORLD_WIDTH = 960;
export const WORLD_HEIGHT = 720;

export const FIXED_TIMESTEP = 1 / 60;
export const MAX_FRAME_DELTA = 0.25;
export const MAX_SUBSTEPS = 8;

export const STARTING_LIVES = 3;
export const EXTRA_LIFE_SCORE_STEP = 10000;

export const SHIP_RADIUS = 14;
export const SHIP_TURN_SPEED = 4.8;
export const SHIP_THRUST = 280;
export const SHIP_DRAG = 0.992;
export const SHIP_MAX_SPEED = 340;
export const SHIP_RESPAWN_DELAY = 1.25; // deprecated: use SHIP_RESPAWN_FRAMES
export const SHIP_SPAWN_INVULNERABLE = 2; // deprecated: use SHIP_SPAWN_INVULNERABLE_FRAMES

export const SHIP_BULLET_SPEED = 520;
export const SHIP_BULLET_LIFETIME = 0.85; // deprecated: use SHIP_BULLET_LIFETIME_FRAMES
export const SHIP_BULLET_COOLDOWN = 0.16; // deprecated: use SHIP_BULLET_COOLDOWN_FRAMES
export const SHIP_BULLET_LIMIT = 4;

export const SAUCER_BULLET_SPEED = 280;
export const SAUCER_BULLET_LIFETIME = 1.4; // deprecated: use SAUCER_BULLET_LIFETIME_FRAMES

export const SAUCER_SPAWN_MIN = 7; // deprecated: use SAUCER_SPAWN_MIN_FRAMES
export const SAUCER_SPAWN_MAX = 14; // deprecated: use SAUCER_SPAWN_MAX_FRAMES

// Frame-count timer constants (integer, ZK-friendly)
export const SHIP_RESPAWN_FRAMES = 75; // 1.25s * 60fps
export const SHIP_SPAWN_INVULNERABLE_FRAMES = 120; // 2s * 60fps
export const SHIP_BULLET_LIFETIME_FRAMES = 51; // 0.85s * 60fps
export const SHIP_BULLET_COOLDOWN_FRAMES = 10; // 0.16s * 60fps ≈ 9.6 → 10
export const SAUCER_BULLET_LIFETIME_FRAMES = 84; // 1.4s * 60fps
export const SAUCER_SPAWN_MIN_FRAMES = 420; // 7s * 60fps
export const SAUCER_SPAWN_MAX_FRAMES = 840; // 14s * 60fps

// Derived constant for autopilot bullet range (pixels)
export const SHIP_BULLET_RANGE = 442; // SHIP_BULLET_SPEED/60 * SHIP_BULLET_LIFETIME_FRAMES ≈ 442

export const ASTEROID_CAP = 27;

export const SCORE_LARGE_ASTEROID = 20;
export const SCORE_MEDIUM_ASTEROID = 50;
export const SCORE_SMALL_ASTEROID = 100;
export const SCORE_LARGE_SAUCER = 200;
export const SCORE_SMALL_SAUCER = 1000;

export const STORAGE_HIGH_SCORE_KEY = "asteroids.highScore";

// Particle system
export const MAX_PARTICLES = 300;
export const MAX_DEBRIS = 50;

// Screen shake
export const SHAKE_DECAY = 0.92;
export const SHAKE_INTENSITY_SMALL = 3;
export const SHAKE_INTENSITY_MEDIUM = 6;
export const SHAKE_INTENSITY_LARGE = 10;

// Visual effects
export const GLOW_ENABLED = true;
export const SCANLINE_OPACITY = 0.08;
export const CRT_CURVATURE = 0.02;

// Anti-lurking behavior
export const LURK_TIME_THRESHOLD = 6; // deprecated: use LURK_TIME_THRESHOLD_FRAMES
export const LURK_SAUCER_SPAWN_FAST = 3; // deprecated: use LURK_SAUCER_SPAWN_FAST_FRAMES
export const LURK_TIME_THRESHOLD_FRAMES = 360; // 6s * 60fps
export const LURK_SAUCER_SPAWN_FAST_FRAMES = 180; // 3s * 60fps

// === Q12.4 World dimensions ===
export const WORLD_WIDTH_Q12_4 = 15360; // 960 * 16
export const WORLD_HEIGHT_Q12_4 = 11520; // 720 * 16

// === Q8.8 Velocity constants (px/s -> px/frame -> x256) ===
export const SHIP_THRUST_Q8_8 = 20; // 280/3600 * 256 ~ 19.9
export const SHIP_MAX_SPEED_Q8_8 = 1451; // 340/60 * 256
export const SHIP_MAX_SPEED_SQ_Q16_16 = 1451 * 1451; // For clamp comparison

// === BAM angle constants ===
export const SHIP_TURN_SPEED_BAM = 3; // 4.8 rad/s / 60 / (2pi/256) ~ 3.26 -> 3
export const SHIP_FACING_UP_BAM = 192; // -90deg in BAM (0=right,64=down,128=left,192=up)

// === Q8.8 Bullet speeds ===
export const SHIP_BULLET_SPEED_Q8_8 = 2219; // 520/60 * 256
export const SAUCER_BULLET_SPEED_Q8_8 = 1195; // 280/60 * 256

// === Q8.8 Asteroid speed ranges [min, max] ===
export const ASTEROID_SPEED_Q8_8: Record<string, [number, number]> = {
  large: [145, 248], // [34/60*256, 58/60*256]
  medium: [265, 401], // [62/60*256, 94/60*256]
  small: [418, 606], // [98/60*256, 142/60*256]
};

// === Q8.8 Saucer speeds ===
export const SAUCER_SPEED_SMALL_Q8_8 = 405; // 95/60 * 256
export const SAUCER_SPEED_LARGE_Q8_8 = 299; // 70/60 * 256
