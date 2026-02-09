use std::fs;

use asteroids_verifier_core::verify_tape;

fn load(path: &str) -> Vec<u8> {
    let full = format!("../../{path}");
    fs::read(&full).unwrap_or_else(|err| panic!("failed reading {full}: {err}"))
}

#[test]
fn verifies_short_fixture() {
    let bytes = load("test-fixtures/test-short.tape");
    let journal = verify_tape(&bytes, 18_000).expect("short fixture must verify");

    assert_eq!(journal.seed, 0xDEAD_BEEF);
    assert_eq!(journal.frame_count, 500);
    assert_eq!(journal.final_score, 0);
    assert_eq!(journal.final_rng_state, 0xDDEC_443F);
}

#[test]
fn verifies_medium_fixture() {
    let bytes = load("test-fixtures/test-medium.tape");
    let journal = verify_tape(&bytes, 18_000).expect("medium fixture must verify");

    assert_eq!(journal.seed, 0xDEAD_BEEF);
    assert_eq!(journal.frame_count, 3980);
    assert_eq!(journal.final_score, 90);
    assert_eq!(journal.final_rng_state, 0xEB07_19CE);
}

#[test]
fn verifies_downloads_fixture() {
    let bytes = load("test-fixtures/test-real-game.tape");
    let journal = verify_tape(&bytes, 18_000).expect("downloads fixture must verify");

    assert!(journal.frame_count > 0);
    assert!(journal.final_rng_state != 0);
}
