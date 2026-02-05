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
export const SHIP_RESPAWN_DELAY = 1.25;
export const SHIP_SPAWN_INVULNERABLE = 2;

export const SHIP_BULLET_SPEED = 520;
export const SHIP_BULLET_LIFETIME = 0.85;
export const SHIP_BULLET_COOLDOWN = 0.16;
export const SHIP_BULLET_LIMIT = 4;

export const SAUCER_BULLET_SPEED = 280;
export const SAUCER_BULLET_LIFETIME = 1.4;

export const HYPERSPACE_COOLDOWN = 1.2;

export const SAUCER_SPAWN_MIN = 7;
export const SAUCER_SPAWN_MAX = 14;

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
export const LURK_TIME_THRESHOLD = 6; // seconds without destroying asteroids
export const LURK_SAUCER_SPAWN_FAST = 3; // faster saucer spawn when lurking
