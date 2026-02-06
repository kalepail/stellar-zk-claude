use serde::{Deserialize, Serialize};

/// Asteroid size enum
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AsteroidSize {
    #[default]
    Large = 0,
    Medium = 1,
    Small = 2,
}

impl AsteroidSize {
    pub fn radius_q12_4(&self) -> u16 {
        match self {
            AsteroidSize::Large => 768,  // 48 * 16
            AsteroidSize::Medium => 448, // 28 * 16
            AsteroidSize::Small => 256,  // 16 * 16
        }
    }

    pub fn score(&self) -> u32 {
        match self {
            AsteroidSize::Large => 20,
            AsteroidSize::Medium => 50,
            AsteroidSize::Small => 100,
        }
    }
}

/// Ship state
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ship {
    pub x: u16,    // Q12.4 position
    pub y: u16,    // Q12.4 position
    pub vx: i16,   // Q8.8 velocity
    pub vy: i16,   // Q8.8 velocity
    pub angle: u8, // BAM angle (0-255)
    pub can_control: bool,
    pub fire_cooldown: u8,
    pub invulnerable_timer: u16,
    pub respawn_timer: u16,
}

/// Bullet state
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Bullet {
    pub x: u16,   // Q12.4 position
    pub y: u16,   // Q12.4 position
    pub vx: i16,  // Q8.8 velocity
    pub vy: i16,  // Q8.8 velocity
    pub life: u8, // Frames remaining
    pub from_saucer: bool,
}

/// Asteroid state
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Asteroid {
    pub x: u16,    // Q12.4 position
    pub y: u16,    // Q12.4 position
    pub vx: i16,   // Q8.8 velocity
    pub vy: i16,   // Q8.8 velocity
    pub angle: u8, // BAM angle
    pub spin: i8,  // BAM spin per frame
    pub size: AsteroidSize,
    pub alive: bool,
}

/// Saucer state
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Saucer {
    pub x: u16,  // Q12.4 position
    pub y: u16,  // Q12.4 position
    pub vx: i16, // Q8.8 velocity
    pub vy: i16, // Q8.8 velocity
    pub small: bool,
    pub fire_cooldown: u8,
    pub drift_timer: u16,
    pub alive: bool,
}

/// Game mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GameMode {
    #[default]
    Menu,
    Playing,
    Paused,
    GameOver,
    Replay,
}

/// Frame input (4 bits)
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct FrameInput {
    pub left: bool,
    pub right: bool,
    pub thrust: bool,
    pub fire: bool,
}

impl FrameInput {
    pub fn from_byte(byte: u8) -> Self {
        FrameInput {
            left: (byte & 0x01) != 0,
            right: (byte & 0x02) != 0,
            thrust: (byte & 0x04) != 0,
            fire: (byte & 0x08) != 0,
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut byte = 0u8;
        if self.left {
            byte |= 0x01;
        }
        if self.right {
            byte |= 0x02;
        }
        if self.thrust {
            byte |= 0x04;
        }
        if self.fire {
            byte |= 0x08;
        }
        byte
    }
}

/// Full game state
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameState {
    pub mode: GameMode,
    pub frame_count: u32,
    pub score: u32,
    pub lives: u8,
    pub wave: u8,
    pub next_extra_life_score: u32,
    pub time_since_last_kill: u16,
    pub saucer_spawn_timer: u16,
    pub ship: Ship,
    pub bullets: Vec<Bullet>,
    pub asteroids: Vec<Asteroid>,
    pub saucers: Vec<Saucer>,
    pub saucer_bullets: Vec<Bullet>,
}

/// Public outputs committed to journal (legacy format for compatibility)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicOutput {
    pub seed: u32,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_rng_state: u32,
    pub rules_version_hash: u32,
    pub tape_crc: u32,
    pub verified: bool,
}

impl PublicOutput {
    /// Convert to new VerificationResult format
    pub fn to_result(&self) -> VerificationResult {
        VerificationResult {
            ok: self.verified,
            fail_frame: if self.verified {
                None
            } else {
                Some(self.frame_count)
            },
            error: if self.verified {
                None
            } else {
                Some(VerificationError {
                    frame: self.frame_count,
                    code: "VERIFICATION_FAILED".to_string(),
                    message: "Verification failed".to_string(),
                })
            },
            final_score: self.final_score,
            final_rng_state: self.final_rng_state,
        }
    }
}

/// Verification error with detailed information
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VerificationError {
    pub frame: u32,
    pub code: String,
    pub message: String,
}

/// Verification result with detailed error information
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VerificationResult {
    pub ok: bool,
    pub fail_frame: Option<u32>,
    pub error: Option<VerificationError>,
    pub final_score: u32,
    pub final_rng_state: u32,
}
