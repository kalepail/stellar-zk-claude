//! Integration tests: replay real .tape files and verify against TS output.

use asteroids_core::{deserialize_tape, replay_tape};
use std::fs;

/// Load a tape file from the test-fixtures directory.
fn load_tape(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../test-fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    fs::read(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"))
}

#[test]
fn test_replay_short_tape() {
    // test-short.tape: seed=0xDEADBEEF, 500 frames, score=0
    let data = load_tape("test-short.tape");
    let tape = deserialize_tape(&data).expect("tape should parse");

    assert_eq!(tape.header.seed, 0xDEADBEEF);
    assert_eq!(tape.header.frame_count, 500);
    assert_eq!(tape.footer.final_score, 0);

    let (score, rng_state) = replay_tape(tape.header.seed, &tape.inputs);

    assert_eq!(
        score, tape.footer.final_score,
        "Score mismatch: Rust={score}, TS={}",
        tape.footer.final_score
    );
    assert_eq!(
        rng_state, tape.footer.final_rng_state,
        "RNG mismatch: Rust=0x{rng_state:08x}, TS=0x{:08x}",
        tape.footer.final_rng_state
    );
}

#[test]
fn test_replay_medium_tape() {
    // test-medium.tape: seed=0xDEADBEEF, 3980 frames, score=2040
    let data = load_tape("test-medium.tape");
    let tape = deserialize_tape(&data).expect("tape should parse");

    assert_eq!(tape.header.seed, 0xDEADBEEF);
    assert_eq!(tape.header.frame_count, 3980);
    assert_eq!(tape.footer.final_score, 2040);

    let (score, rng_state) = replay_tape(tape.header.seed, &tape.inputs);

    assert_eq!(
        score, tape.footer.final_score,
        "Score mismatch: Rust={score}, TS={}",
        tape.footer.final_score
    );
    assert_eq!(
        rng_state, tape.footer.final_rng_state,
        "RNG mismatch: Rust=0x{rng_state:08x}, TS=0x{:08x}",
        tape.footer.final_rng_state
    );
}

/// Verify intermediate checkpoints match TypeScript dump.
/// Reference values from: bun run scripts/dump-tape-state.ts test-medium.tape --every 500
#[test]
fn test_medium_tape_checkpoints() {
    let data = load_tape("test-medium.tape");
    let tape = deserialize_tape(&data).expect("tape should parse");

    // Expected checkpoints from TypeScript:
    // frame 0:    rng=4160380745, score=0,    lives=3, wave=1
    // frame 500:  rng=3482026331, score=0,    lives=3, wave=1
    // frame 1000: rng=1412112207, score=20,   lives=3, wave=1
    // frame 1500: rng=3974679983, score=40,   lives=2, wave=1
    // frame 2000: rng=2535921159, score=140,  lives=2, wave=1
    // frame 2500: rng=128860544,  score=1390, lives=2, wave=1
    // frame 3000: rng=2942077878, score=1690, lives=1, wave=1
    // frame 3500: rng=4136080194, score=1940, lives=1, wave=1
    // frame 3980: rng=557700556,  score=2040, lives=0, wave=1

    let checkpoints: Vec<(u32, u32, u32)> = vec![
        // (frame, expected_rng, expected_score)
        (0, 4160380745, 0),
        (500, 3482026331, 0),
        (1000, 1412112207, 20),
        (1500, 3974679983, 40),
        (2000, 2535921159, 140),
        (2500, 128860544, 1390),
        (3000, 2942077878, 1690),
        (3500, 4136080194, 1940),
        (3980, 557700556, 2040),
    ];

    let mut game = asteroids_core::AsteroidsGame::new(tape.header.seed);

    // Check frame 0 (before any simulation)
    assert_eq!(game.rng_state(), checkpoints[0].1, "Frame 0 RNG mismatch");
    assert_eq!(game.score(), checkpoints[0].2, "Frame 0 score mismatch");

    let mut checkpoint_idx = 1;

    for i in 0..tape.header.frame_count as usize {
        let input = asteroids_core::FrameInput::from_byte(tape.inputs[i]);
        game.step(input);

        let frame = (i + 1) as u32;
        if checkpoint_idx < checkpoints.len() && frame == checkpoints[checkpoint_idx].0 {
            let (_, expected_rng, expected_score) = checkpoints[checkpoint_idx];
            assert_eq!(
                game.rng_state(),
                expected_rng,
                "Frame {frame} RNG mismatch: got 0x{:08x}, expected 0x{expected_rng:08x}",
                game.rng_state()
            );
            assert_eq!(
                game.score(),
                expected_score,
                "Frame {frame} score mismatch: got {}, expected {expected_score}",
                game.score()
            );
            checkpoint_idx += 1;
        }
    }

    assert_eq!(checkpoint_idx, checkpoints.len(), "Not all checkpoints verified");
}

#[test]
fn test_replay_real_tape() {
    // Real gameplay tape from Downloads: seed=0x2fc80c3b, 6894 frames, score=16270
    let data = load_tape("test-real.tape");
    let tape = deserialize_tape(&data).expect("tape should parse");

    assert_eq!(tape.header.seed, 0x2fc80c3b);
    assert_eq!(tape.header.frame_count, 6894);
    assert_eq!(tape.footer.final_score, 16270);
    assert_eq!(tape.footer.final_rng_state, 0x919db2b7);

    let (score, rng_state) = replay_tape(tape.header.seed, &tape.inputs);

    assert_eq!(
        score, tape.footer.final_score,
        "Score mismatch: Rust={score}, TS={}",
        tape.footer.final_score
    );
    assert_eq!(
        rng_state, tape.footer.final_rng_state,
        "RNG mismatch: Rust=0x{rng_state:08x}, TS=0x{:08x}",
        tape.footer.final_rng_state
    );
}
