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
pub use tape::{deserialize_tape, Tape, TapeError};
pub use types::FrameInput;
