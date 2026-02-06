use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::constants::{MAX_FRAMES_DEFAULT, RULES_DIGEST_V1};
use crate::error::VerifyError;
use crate::sim::{replay_strict, ReplayResult, ReplayViolation};
use crate::tape::parse_tape;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuestInput {
    pub tape: Vec<u8>,
    pub max_frames: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationJournal {
    pub seed: u32,
    pub frame_count: u32,
    pub final_score: u32,
    pub final_rng_state: u32,
    pub tape_checksum: u32,
    pub rules_digest: u32,
}

pub fn verify_guest_input(input: &GuestInput) -> Result<VerificationJournal, VerifyError> {
    let max_frames = if input.max_frames == 0 {
        MAX_FRAMES_DEFAULT
    } else {
        input.max_frames
    };
    verify_tape(&input.tape, max_frames)
}

pub fn verify_tape(bytes: &[u8], max_frames: u32) -> Result<VerificationJournal, VerifyError> {
    verify_tape_with_replay(bytes, max_frames, replay_strict)
}

fn verify_tape_with_replay<F>(
    bytes: &[u8],
    max_frames: u32,
    replay_fn: F,
) -> Result<VerificationJournal, VerifyError>
where
    F: FnOnce(u32, &[u8]) -> Result<ReplayResult, ReplayViolation>,
{
    let tape = parse_tape(bytes, max_frames)?;
    let replay_result =
        replay_fn(tape.header.seed, tape.inputs).map_err(|err| VerifyError::RuleViolation {
            frame: err.frame_count,
            rule: err.rule,
        })?;

    if replay_result.frame_count != tape.header.frame_count {
        return Err(VerifyError::FrameCountMismatch {
            claimed: tape.header.frame_count,
            computed: replay_result.frame_count,
        });
    }

    if replay_result.final_score != tape.footer.final_score {
        return Err(VerifyError::ScoreMismatch {
            claimed: tape.footer.final_score,
            computed: replay_result.final_score,
        });
    }

    if replay_result.final_rng_state != tape.footer.final_rng_state {
        return Err(VerifyError::RngMismatch {
            claimed: tape.footer.final_rng_state,
            computed: replay_result.final_rng_state,
        });
    }

    Ok(VerificationJournal {
        seed: tape.header.seed,
        frame_count: tape.header.frame_count,
        final_score: replay_result.final_score,
        final_rng_state: replay_result.final_rng_state,
        tape_checksum: tape.footer.checksum,
        rules_digest: RULES_DIGEST_V1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{TAPE_HEADER_SIZE, TAPE_MAGIC, TAPE_VERSION};
    use crate::error::RuleCode;
    use crate::sim::replay;
    use crate::tape::{crc32, serialize_tape};

    fn footer_offset(frame_count: usize) -> usize {
        TAPE_HEADER_SIZE + frame_count
    }

    fn write_footer(bytes: &mut [u8], frame_count: usize, final_score: u32, final_rng_state: u32) {
        let offset = footer_offset(frame_count);
        let checksum = crc32(&bytes[..offset]);
        bytes[offset..offset + 4].copy_from_slice(&final_score.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&final_rng_state.to_le_bytes());
        bytes[offset + 8..offset + 12].copy_from_slice(&checksum.to_le_bytes());
    }

    fn valid_tape(seed: u32, inputs: &[u8]) -> Vec<u8> {
        let replay_result = replay(seed, inputs);
        serialize_tape(
            seed,
            inputs,
            replay_result.final_score,
            replay_result.final_rng_state,
        )
    }

    #[test]
    fn rejects_reserved_input_bits() {
        let mut tape = serialize_tape(0xAABB_CCDD, &[0x10], 0, 0xAABB_CCDD);
        write_footer(&mut tape, 1, 0, 0xAABB_CCDD);

        let err = verify_tape(&tape, 10).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::ReservedInputBitsNonZero {
                frame: 0,
                byte: 0x10
            }
        ));
    }

    #[test]
    fn detects_score_tampering() {
        let inputs = [0x00u8; 60];
        let seed = 0x1234_5678;
        let mut good_tape = valid_tape(seed, &inputs);
        let journal = verify_tape(&good_tape, 10_000).unwrap();

        let offset = footer_offset(inputs.len());
        let tampered_score = journal.final_score + 1;
        good_tape[offset..offset + 4].copy_from_slice(&tampered_score.to_le_bytes());

        let err = verify_tape(&good_tape, 10_000).unwrap_err();
        assert!(matches!(err, VerifyError::ScoreMismatch { .. }));
    }

    #[test]
    fn detects_rng_tampering() {
        let inputs = [0x00u8; 48];
        let seed = 0x1234_5678;
        let mut tape = valid_tape(seed, &inputs);
        let offset = footer_offset(inputs.len());
        let tampered_rng = 0xFFFF_FFFFu32;
        tape[offset + 4..offset + 8].copy_from_slice(&tampered_rng.to_le_bytes());

        let err = verify_tape(&tape, 10_000).unwrap_err();
        assert!(matches!(err, VerifyError::RngMismatch { .. }));
    }

    #[test]
    fn guest_input_uses_default_max_frames_when_zero() {
        let inputs = [0x00u8; 32];
        let tape = valid_tape(0x4455_6677, &inputs);
        let guest_input = GuestInput {
            tape,
            max_frames: 0,
        };

        let journal = verify_guest_input(&guest_input).unwrap();
        assert_eq!(journal.frame_count, inputs.len() as u32);
        assert_eq!(journal.rules_digest, RULES_DIGEST_V1);
    }

    #[test]
    fn guest_input_honors_explicit_max_frames() {
        let inputs = [0x00u8; 32];
        let tape = valid_tape(0x1122_3344, &inputs);
        let guest_input = GuestInput {
            tape,
            max_frames: 8,
        };

        let err = verify_guest_input(&guest_input).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::FrameCountOutOfRange {
                frame_count: 32,
                max_frames: 8
            }
        ));
    }

    #[test]
    fn maps_replay_violation_to_verify_error() {
        let inputs = [0x00u8; 4];
        let tape = valid_tape(0xDEAD_BEEF, &inputs);
        let err = verify_tape_with_replay(&tape, 100, |_seed, _inputs| {
            Err(ReplayViolation {
                frame_count: 3,
                rule: RuleCode::ShipBounds,
            })
        })
        .unwrap_err();

        assert!(matches!(
            err,
            VerifyError::RuleViolation {
                frame: 3,
                rule: RuleCode::ShipBounds
            }
        ));
    }

    #[test]
    fn detects_frame_count_mismatch_when_replay_disagrees() {
        let inputs = [0x00u8; 4];
        let tape = valid_tape(0xDEAD_BEEF, &inputs);
        let expected = replay(0xDEAD_BEEF, &inputs);
        let err = verify_tape_with_replay(&tape, 100, |_seed, _inputs| {
            Ok(ReplayResult {
                frame_count: expected.frame_count + 1,
                ..expected
            })
        })
        .unwrap_err();

        assert!(matches!(
            err,
            VerifyError::FrameCountMismatch {
                claimed: 4,
                computed: 5
            }
        ));
    }

    #[test]
    fn single_byte_tampering_is_rejected() {
        let inputs = [0x01u8, 0x02, 0x04, 0x08, 0x03, 0x0C, 0x00, 0x07];
        let good_tape = valid_tape(0xFEED_BEEF, &inputs);
        assert!(verify_tape(&good_tape, 100).is_ok());

        for idx in 0..good_tape.len() {
            let mut tampered = good_tape.clone();
            tampered[idx] ^= 0x01;
            assert!(
                verify_tape(&tampered, 100).is_err(),
                "tampering byte index {idx} must fail verification"
            );
        }
    }

    #[test]
    fn parse_checks_happen_before_replay() {
        let mut tape = valid_tape(0xDEAD_BEEF, &[0x00u8; 4]);
        tape[0..4].copy_from_slice(&TAPE_MAGIC.wrapping_add(1).to_le_bytes());
        tape[4] = TAPE_VERSION + 1;

        let err = verify_tape_with_replay(&tape, 10, |_seed, _inputs| {
            panic!("replay must not run when parse fails")
        })
        .unwrap_err();

        assert!(matches!(err, VerifyError::InvalidMagic { .. }));
    }
}
