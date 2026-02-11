use asteroids_verifier_core::rng::SeededRng;
use asteroids_verifier_core::sim::replay;
use asteroids_verifier_core::tape::serialize_tape;
use asteroids_verifier_core::verify_tape;

fn generate_tape(seed: u32, input_seed: u32, frame_count: usize) -> Vec<u8> {
    let mut rng = SeededRng::new(input_seed);
    let mut inputs = vec![0u8; frame_count];
    for input in &mut inputs {
        *input = (rng.next() & 0x0F) as u8;
    }
    let replay_result = replay(seed, &inputs);
    serialize_tape(
        seed,
        &inputs,
        replay_result.final_score,
        replay_result.final_rng_state,
    )
}

#[test]
fn verifies_short_fixture() {
    let bytes = generate_tape(0xDEAD_BEEF, 0xA0A0_0101, 500);
    let journal = verify_tape(&bytes, 18_000).expect("short fixture must verify");

    assert_eq!(journal.seed, 0xDEAD_BEEF);
    assert_eq!(journal.frame_count, 500);
    assert_eq!(journal.final_score, 660);
    assert_eq!(journal.final_rng_state, 4_235_867_870);
}

#[test]
fn verifies_medium_fixture() {
    let bytes = generate_tape(0xDEAD_BEEF, 0xA0A0_0202, 3_980);
    let journal = verify_tape(&bytes, 18_000).expect("medium fixture must verify");

    assert_eq!(journal.seed, 0xDEAD_BEEF);
    assert_eq!(journal.frame_count, 3980);
    assert_eq!(journal.final_score, 10_430);
    assert_eq!(journal.final_rng_state, 2_972_599_750);
}

#[test]
fn verifies_downloads_fixture() {
    let bytes = generate_tape(0x9432_C6CD, 0xA0A0_0303, 13_829);
    let journal = verify_tape(&bytes, 18_000).expect("downloads fixture must verify");

    assert_eq!(journal.seed, 0x9432_C6CD);
    assert_eq!(journal.frame_count, 13_829);
    assert_eq!(journal.final_score, 2_180);
    assert_eq!(journal.final_rng_state, 1_602_398_462);
}
