//! Asteroids core - deterministic game engine for ZK verification.
//!
//! This crate contains the complete game logic in integer-only math,
//! suitable for running inside the RISC Zero zkVM guest.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod constants;
pub mod fixed_point;
pub mod game;
pub mod rng;
pub mod tape;
pub mod types;

// Re-export key items
pub use game::{replay_tape, AsteroidsGame};
pub use rng::SeededRng;
#[cfg(feature = "std")]
pub use tape::{crc32, deserialize_tape};
pub use tape::{parse_tape, Tape, TapeError};
pub use types::FrameInput;
