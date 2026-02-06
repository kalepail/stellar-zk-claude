#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod constants;
pub mod error;
pub mod fixed_point;
pub mod rng;
pub mod sim;
pub mod tape;
pub mod verify;

pub use error::{RuleCode, VerifyError};
pub use verify::{verify_guest_input, verify_tape, GuestInput, VerificationJournal};
