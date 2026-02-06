#![no_main]

use asteroids_core::{
    FrameInput, GameEngine, PublicOutput, RuleViolation, Tape, VerificationError,
    VerificationResult,
};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    // Read the tape from the host
    let tape: Tape = env::read();

    // Verify the tape and produce public output
    let result = verify_tape_with_rules(tape);

    // Commit the public output to the journal
    env::commit(&result);
}

/// Verify a tape by replaying it through the game engine with rule checking
fn verify_tape_with_rules(tape: Tape) -> VerificationResult {
    // Validate tape header
    if let Err(e) = tape.header.validate() {
        return VerificationResult {
            ok: false,
            fail_frame: Some(0),
            error: Some(VerificationError {
                frame: 0,
                code: format!("TAPE_{:?}", e),
                message: e.to_string(),
            }),
            final_score: 0,
            final_rng_state: 0,
        };
    }

    // Create game engine with tape seed
    let mut engine = GameEngine::new(tape.header.seed);

    // Replay each frame with rule checking
    for frame in 0..tape.header.frame_count as usize {
        let input = tape.get_input(frame).unwrap_or_default();

        // Verify input byte format (reserved bits must be 0)
        let input_byte = input.to_byte();
        if input_byte & 0xF0 != 0 {
            return VerificationResult {
                ok: false,
                fail_frame: Some(frame as u32),
                error: Some(VerificationError {
                    frame: frame as u32,
                    code: "TAPE_RESERVED_BITS_NONZERO".to_string(),
                    message: format!(
                        "Input byte at frame {} has reserved bits set: 0x{:02x}",
                        frame, input_byte
                    ),
                }),
                final_score: engine.state().score,
                final_rng_state: engine.rng_state(),
            };
        }

        // Step the simulation with rule checking
        if let Err(violation) = engine.step_with_rules_check(input) {
            return VerificationResult {
                ok: false,
                fail_frame: Some(frame as u32),
                error: Some(VerificationError {
                    frame: frame as u32,
                    code: violation.code.to_string(),
                    message: violation.message,
                }),
                final_score: engine.state().score,
                final_rng_state: engine.rng_state(),
            };
        }
    }

    // Verify final state matches tape footer
    if !engine.verify_final_state(tape.footer.final_score, tape.footer.final_rng_state) {
        let score_match = engine.state().score == tape.footer.final_score;
        let rng_match = engine.rng_state() == tape.footer.final_rng_state;

        let error_msg =
            if !score_match && !rng_match {
                format!(
                "Final state mismatch: expected score={}, rng=0x{:08x}; got score={}, rng=0x{:08x}",
                tape.footer.final_score, tape.footer.final_rng_state,
                engine.state().score, engine.rng_state()
            )
            } else if !score_match {
                format!(
                    "Final score mismatch: expected {}, got {}",
                    tape.footer.final_score,
                    engine.state().score
                )
            } else {
                format!(
                    "Final RNG state mismatch: expected 0x{:08x}, got 0x{:08x}",
                    tape.footer.final_rng_state,
                    engine.rng_state()
                )
            };

        return VerificationResult {
            ok: false,
            fail_frame: Some(tape.header.frame_count),
            error: Some(VerificationError {
                frame: tape.header.frame_count,
                code: "GLOBAL_STATE_MISMATCH".to_string(),
                message: error_msg,
            }),
            final_score: engine.state().score,
            final_rng_state: engine.rng_state(),
        };
    }

    // Success!
    VerificationResult {
        ok: true,
        fail_frame: None,
        error: None,
        final_score: engine.state().score,
        final_rng_state: engine.rng_state(),
    }
}

/// Version hash for rules (commit hash or version number)
const RULES_VERSION_HASH: u32 = 0x0001; // Version 1
