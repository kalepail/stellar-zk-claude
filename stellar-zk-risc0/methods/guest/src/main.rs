#![no_main]

use asteroids_core::{
    FrameInput, GameEngine, PublicOutput, Tape, VerificationError, VerificationResult,
};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    // Read the tape from the host
    let tape: Tape = env::read();

    // Verify the tape and produce public output
    let result = verify_tape(tape);

    // Commit the public output to the journal
    env::commit(&result);
}

/// Verify a tape by replaying it through the game engine
fn verify_tape(tape: Tape) -> PublicOutput {
    // Validate tape header
    if let Err(e) = tape.header.validate() {
        return PublicOutput {
            seed: tape.header.seed,
            frame_count: tape.header.frame_count,
            final_score: 0,
            final_rng_state: 0,
            rules_version_hash: RULES_VERSION_HASH,
            tape_crc: tape.footer.checksum,
            verified: false,
        };
    }

    // Create game engine with tape seed
    let mut engine = GameEngine::new(tape.header.seed);

    // Replay each frame
    for frame in 0..tape.header.frame_count as usize {
        let input = tape.get_input(frame).unwrap_or_default();

        // Verify input byte format (reserved bits must be 0)
        // This is already checked during tape parsing, but double-check here
        let input_byte = input.to_byte();
        if input_byte & 0xF0 != 0 {
            return PublicOutput {
                seed: tape.header.seed,
                frame_count: frame as u32,
                final_score: engine.state().score,
                final_rng_state: engine.rng_state(),
                rules_version_hash: RULES_VERSION_HASH,
                tape_crc: tape.footer.checksum,
                verified: false,
            };
        }

        // Step the simulation
        engine.step(input);
    }

    // Verify final state matches tape footer
    let verified = engine.verify_final_state(tape.footer.final_score, tape.footer.final_rng_state);

    PublicOutput {
        seed: tape.header.seed,
        frame_count: tape.header.frame_count,
        final_score: engine.state().score,
        final_rng_state: engine.rng_state(),
        rules_version_hash: RULES_VERSION_HASH,
        tape_crc: tape.footer.checksum,
        verified,
    }
}

/// Version hash for rules (commit hash or version number)
const RULES_VERSION_HASH: u32 = 0x0001; // Version 1
