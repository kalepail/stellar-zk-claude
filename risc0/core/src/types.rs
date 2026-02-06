//! Game entity types - exact match to TypeScript types.ts
//!
//! Only gameplay-relevant fields are included (no visual-only fields like
//! prevX/prevY, vertices, particles, etc.)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsteroidSize {
    Large,
    Medium,
    Small,
}

impl AsteroidSize {
    /// The child size when an asteroid splits.
    pub fn child_size(self) -> Option<AsteroidSize> {
        match self {
            AsteroidSize::Large => Some(AsteroidSize::Medium),
            AsteroidSize::Medium => Some(AsteroidSize::Small),
            AsteroidSize::Small => None,
        }
    }
}

/// Frame input: 4 boolean buttons packed as the low 4 bits of a byte.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FrameInput {
    pub left: bool,
    pub right: bool,
    pub thrust: bool,
    pub fire: bool,
}

impl FrameInput {
    /// Decode from a tape byte (bit 0=left, 1=right, 2=thrust, 3=fire).
    pub fn from_byte(byte: u8) -> Self {
        Self {
            left: byte & 0x01 != 0,
            right: byte & 0x02 != 0,
            thrust: byte & 0x04 != 0,
            fire: byte & 0x08 != 0,
        }
    }

    /// Encode to a tape byte.
    pub fn to_byte(self) -> u8 {
        (if self.left { 0x01 } else { 0 })
            | (if self.right { 0x02 } else { 0 })
            | (if self.thrust { 0x04 } else { 0 })
            | (if self.fire { 0x08 } else { 0 })
    }
}

/// Ship entity.
#[derive(Debug, Clone)]
pub struct Ship {
    pub x: i32,            // Q12.4
    pub y: i32,            // Q12.4
    pub vx: i32,           // Q8.8
    pub vy: i32,           // Q8.8
    pub angle: u8,         // BAM
    pub radius: i32,       // pixels
    pub can_control: bool,
    pub fire_cooldown: i32,
    pub respawn_timer: i32,
    pub invulnerable_timer: i32,
}

/// Asteroid entity.
#[derive(Debug, Clone)]
pub struct Asteroid {
    pub x: i32,            // Q12.4
    pub y: i32,            // Q12.4
    pub vx: i32,           // Q8.8
    pub vy: i32,           // Q8.8
    pub angle: i32,        // BAM (stored as i32 for spin arithmetic)
    pub alive: bool,
    pub radius: i32,       // pixels
    pub size: AsteroidSize,
    pub spin: i32,         // BAM per frame
}

/// Bullet entity.
#[derive(Debug, Clone)]
pub struct Bullet {
    pub x: i32,            // Q12.4
    pub y: i32,            // Q12.4
    pub vx: i32,           // Q8.8
    pub vy: i32,           // Q8.8
    pub angle: u8,         // BAM
    pub alive: bool,
    pub radius: i32,       // pixels
    pub life: i32,         // frames remaining
    pub from_saucer: bool,
}

/// Saucer entity.
#[derive(Debug, Clone)]
pub struct Saucer {
    pub x: i32,            // Q12.4
    pub y: i32,            // Q12.4
    pub vx: i32,           // Q8.8
    pub vy: i32,           // Q8.8
    pub alive: bool,
    pub radius: i32,       // pixels
    pub small: bool,
    pub fire_cooldown: i32,
    pub drift_timer: i32,
}
