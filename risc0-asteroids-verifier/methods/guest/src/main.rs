#![no_main]
#![no_std]

extern crate alloc;

use asteroids_verifier_core::{verify_guest_input, GuestInput};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let mut max_frames_bytes = [0u8; 4];
    env::read_slice(&mut max_frames_bytes);
    let max_frames = u32::from_le_bytes(max_frames_bytes);

    let mut tape_len_bytes = [0u8; 4];
    env::read_slice(&mut tape_len_bytes);
    let tape_len = u32::from_le_bytes(tape_len_bytes) as usize;

    let padded_tape_len = (tape_len + 3) & !3;
    let mut tape = alloc::vec![0u8; padded_tape_len];
    env::read_slice(&mut tape);
    tape.truncate(tape_len);

    let guest_input = GuestInput { tape, max_frames };

    let journal = verify_guest_input(&guest_input).unwrap_or_else(|err| {
        panic!("guest verification failed: {err}");
    });

    env::commit(&journal);
}
