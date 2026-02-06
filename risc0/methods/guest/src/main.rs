//! ZK Guest program: replays an Asteroids game tape inside the RISC Zero zkVM.
//!
//! INPUT (private, from host via env::read_slice):
//!   - tape_len: u32 (4 bytes LE)  — byte length of the tape
//!   - tape_bytes: [u8]            — raw .tape file bytes (word-padded)
//!
//! VERIFICATION (inside guest, NOT visible to verifier):
//!   - Parses tape (validates magic, version, CRC-32)
//!   - Replays the game deterministically using integer-only math
//!   - Asserts final score and RNG state match the tape footer
//!   - Any divergence => panic => no valid proof generated
//!
//! OUTPUT (public, committed to journal):
//!   - seed: u32         — the game seed (identifies the game instance)
//!   - score: u32        — the proven final score
//!   - frame_count: u32  — how many frames were played

use asteroids_core::{deserialize_tape, replay_tape};
use risc0_zkvm::guest::env;

/// When the `cycle-prof` feature is enabled, prints the cycle count delta
/// for each labeled section to stderr. Compiles to nothing when disabled.
#[cfg(feature = "cycle-prof")]
macro_rules! cycle_mark {
    ($label:expr, $prev:ident) => {
        let now = env::cycle_count();
        eprintln!("  [cycles] {}: {}", $label, now - $prev);
        $prev = now;
    };
}

#[cfg(not(feature = "cycle-prof"))]
macro_rules! cycle_mark {
    ($label:expr, $prev:ident) => {};
}

fn main() {
    #[cfg(feature = "cycle-prof")]
    let mut _t = env::cycle_count();
    #[cfg(not(feature = "cycle-prof"))]
    let mut _t = 0u64;

    // Read the raw tape bytes from the host (private input).
    // Using read_slice bypasses serde deserialization, which otherwise inflates
    // each u8 to a u32 word (4x memory and cycle overhead).
    let mut len_buf = [0u8; 4];
    env::read_slice(&mut len_buf);
    let tape_len = u32::from_le_bytes(len_buf) as usize;
    let padded_len = (tape_len + 3) & !3;
    let mut tape_bytes = vec![0u8; padded_len];
    env::read_slice(&mut tape_bytes);
    tape_bytes.truncate(tape_len);
    cycle_mark!("env::read_slice", _t);

    // Parse and validate the tape (magic, version, CRC-32 integrity)
    let tape = deserialize_tape(&tape_bytes)
        .expect("Invalid tape: failed to parse or CRC mismatch");
    cycle_mark!("deserialize_tape", _t);

    // Replay the game deterministically
    let (actual_score, actual_rng_state) = replay_tape(tape.header.seed, &tape.inputs);
    cycle_mark!("replay_tape", _t);

    // Verify the replay matches the tape's claimed results.
    // If these assertions fail, the guest panics and no proof is generated.
    // This is the core anti-cheat mechanism: a cheater cannot forge a tape
    // that claims a score they didn't legitimately earn.
    assert_eq!(
        actual_score, tape.footer.final_score,
        "Score mismatch: computed {}, tape claims {}",
        actual_score, tape.footer.final_score
    );
    assert_eq!(
        actual_rng_state, tape.footer.final_rng_state,
        "RNG state mismatch: computed 0x{:08x}, tape claims 0x{:08x}",
        actual_rng_state, tape.footer.final_rng_state
    );

    // Commit public outputs to the journal.
    // These are the only values visible to the verifier.
    // The tape contents (player inputs) remain private.
    env::commit(&tape.header.seed);
    env::commit(&actual_score);
    env::commit(&tape.header.frame_count);
    cycle_mark!("assert+commit", _t);
}
