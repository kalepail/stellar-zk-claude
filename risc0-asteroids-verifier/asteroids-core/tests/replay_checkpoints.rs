use std::fs;

use asteroids_verifier_core::sim::{replay_with_checkpoints, ReplayCheckpoint};
use asteroids_verifier_core::tape::parse_tape;

fn load(path: &str) -> Vec<u8> {
    let full = format!("../../{path}");
    fs::read(&full).unwrap_or_else(|err| panic!("failed reading {full}: {err}"))
}

fn mix_u64(hash: u64, value: u64) -> u64 {
    // FNV-1a style mix for stable fixture fingerprinting.
    hash.wrapping_mul(0x0000_0100_0000_01B3) ^ value
}

fn checkpoint_fingerprint(checkpoints: &[ReplayCheckpoint]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325u64;

    for checkpoint in checkpoints {
        hash = mix_u64(hash, checkpoint.frame_count as u64);
        hash = mix_u64(hash, checkpoint.rng_state as u64);
        hash = mix_u64(hash, checkpoint.score as u64);
        hash = mix_u64(hash, checkpoint.lives as i64 as u64);
        hash = mix_u64(hash, checkpoint.wave as i64 as u64);
        hash = mix_u64(hash, checkpoint.asteroids as u64);
        hash = mix_u64(hash, checkpoint.bullets as u64);
        hash = mix_u64(hash, checkpoint.saucers as u64);
        hash = mix_u64(hash, checkpoint.saucer_bullets as u64);
        hash = mix_u64(hash, checkpoint.ship_x as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_y as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_vx as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_vy as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_angle as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_can_control as u64);
        hash = mix_u64(hash, checkpoint.ship_fire_cooldown as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_respawn_timer as i64 as u64);
        hash = mix_u64(hash, checkpoint.ship_invulnerable_timer as i64 as u64);
    }

    hash
}

#[test]
fn short_fixture_checkpoint_fingerprint_stable() {
    let bytes = load("test-fixtures/test-short.tape");
    let tape = parse_tape(&bytes, 18_000).expect("fixture tape should parse");
    let checkpoints = replay_with_checkpoints(tape.header.seed, tape.inputs, 50);

    assert_eq!(checkpoints.first().expect("checkpoint").frame_count, 0);
    assert_eq!(
        checkpoints.last().expect("checkpoint").frame_count,
        tape.header.frame_count
    );

    // Updated golden fingerprint for AST3 ruleset.
    assert_eq!(
        checkpoint_fingerprint(&checkpoints),
        10_036_430_449_149_217_048
    );
}

#[test]
fn medium_fixture_checkpoint_fingerprint_stable() {
    let bytes = load("test-fixtures/test-medium.tape");
    let tape = parse_tape(&bytes, 18_000).expect("fixture tape should parse");
    let checkpoints = replay_with_checkpoints(tape.header.seed, tape.inputs, 200);

    assert_eq!(checkpoints.first().expect("checkpoint").frame_count, 0);
    assert_eq!(
        checkpoints.last().expect("checkpoint").frame_count,
        tape.header.frame_count
    );

    // Updated golden fingerprint for AST3 ruleset.
    assert_eq!(
        checkpoint_fingerprint(&checkpoints),
        11_214_824_663_140_276_594
    );
}
