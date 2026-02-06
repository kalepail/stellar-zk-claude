use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::constants::{MAX_FRAMES_DEFAULT, RULES_DIGEST_V1};
use crate::error::VerifyError;
use crate::sim::replay_strict;
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
    let tape = parse_tape(bytes, max_frames)?;
    let replay_result =
        replay_strict(tape.header.seed, tape.inputs).map_err(|err| VerifyError::RuleViolation {
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
    use crate::constants::TAPE_HEADER_SIZE;
    use crate::sim::replay;
    use crate::tape::{serialize_tape, TapeFooter};

    fn footer_offset(frame_count: usize) -> usize {
        TAPE_HEADER_SIZE + frame_count
    }

    #[test]
    fn rejects_reserved_input_bits() {
        let mut tape = serialize_tape(0xAABB_CCDD, &[0x10], 0, 0xAABB_CCDD);
        let offset = footer_offset(1);
        let footer = TapeFooter {
            final_score: 0,
            final_rng_state: 0xAABB_CCDD,
            checksum: crate::tape::crc32(&tape[..offset]),
        };
        tape[offset..offset + 4].copy_from_slice(&footer.final_score.to_le_bytes());
        tape[offset + 4..offset + 8].copy_from_slice(&footer.final_rng_state.to_le_bytes());
        tape[offset + 8..offset + 12].copy_from_slice(&footer.checksum.to_le_bytes());

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
    fn detects_footer_tampering() {
        let inputs = [0x00u8; 60];
        let seed = 0x1234_5678;
        let replay_result = replay(seed, &inputs);
        let mut good_tape = serialize_tape(
            seed,
            &inputs,
            replay_result.final_score,
            replay_result.final_rng_state,
        );
        let journal = verify_tape(&good_tape, 10_000).unwrap();

        let offset = footer_offset(inputs.len());
        let tampered_score = journal.final_score + 1;
        good_tape[offset..offset + 4].copy_from_slice(&tampered_score.to_le_bytes());

        let err = verify_tape(&good_tape, 10_000).unwrap_err();
        assert!(matches!(err, VerifyError::ScoreMismatch { .. }));
    }
}
