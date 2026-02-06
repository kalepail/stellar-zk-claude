#![no_main]
#![no_std]

extern crate alloc;

use asteroids_verifier_core::{verify_guest_input, GuestInput};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let guest_input: GuestInput = env::read();

    let journal = verify_guest_input(&guest_input).unwrap_or_else(|err| {
        panic!("guest verification failed: {err}");
    });

    env::commit(&journal);
}
