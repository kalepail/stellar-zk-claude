use alloc::{vec, vec::Vec};
use serde::{Deserialize, Serialize};

use crate::constants::{TAPE_FOOTER_SIZE, TAPE_HEADER_SIZE, TAPE_MAGIC, TAPE_VERSION};
use crate::error::VerifyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeHeader {
    pub magic: u32,
    pub version: u8,
    pub seed: u32,
    pub frame_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeFooter {
    pub final_score: u32,
    pub final_rng_state: u32,
    pub checksum: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapeView<'a> {
    pub header: TapeHeader,
    pub inputs: &'a [u8],
    pub footer: TapeFooter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInput {
    pub left: bool,
    pub right: bool,
    pub thrust: bool,
    pub fire: bool,
}

#[inline]
pub fn encode_input_byte(input: FrameInput) -> u8 {
    (if input.left { 0x01 } else { 0 })
        | (if input.right { 0x02 } else { 0 })
        | (if input.thrust { 0x04 } else { 0 })
        | (if input.fire { 0x08 } else { 0 })
}

#[inline]
pub fn decode_input_byte(byte: u8) -> FrameInput {
    FrameInput {
        left: (byte & 0x01) != 0,
        right: (byte & 0x02) != 0,
        thrust: (byte & 0x04) != 0,
        fire: (byte & 0x08) != 0,
    }
}

pub fn parse_tape(bytes: &[u8], max_frames: u32) -> Result<TapeView<'_>, VerifyError> {
    let min_len = TAPE_HEADER_SIZE + TAPE_FOOTER_SIZE;
    if bytes.len() < min_len {
        return Err(VerifyError::TapeTooShort {
            actual: bytes.len(),
            min: min_len,
        });
    }

    let magic = read_u32_le(bytes, 0);
    if magic != TAPE_MAGIC {
        return Err(VerifyError::InvalidMagic { found: magic });
    }

    let version = bytes[4];
    if version != TAPE_VERSION {
        return Err(VerifyError::UnsupportedVersion { found: version });
    }

    if bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0 {
        return Err(VerifyError::HeaderReservedNonZero);
    }

    let seed = read_u32_le(bytes, 8);
    let frame_count = read_u32_le(bytes, 12);

    if frame_count == 0 || frame_count > max_frames {
        return Err(VerifyError::FrameCountOutOfRange {
            frame_count,
            max_frames,
        });
    }

    let expected_len = TAPE_HEADER_SIZE + frame_count as usize + TAPE_FOOTER_SIZE;
    if bytes.len() != expected_len {
        return Err(VerifyError::TapeLengthMismatch {
            expected: expected_len,
            actual: bytes.len(),
        });
    }

    let inputs_start = TAPE_HEADER_SIZE;
    let inputs_end = inputs_start + frame_count as usize;
    let inputs = &bytes[inputs_start..inputs_end];

    for (frame, byte) in inputs.iter().enumerate() {
        if (byte & 0xF0) != 0 {
            return Err(VerifyError::ReservedInputBitsNonZero {
                frame: frame as u32,
                byte: *byte,
            });
        }
    }

    let final_score = read_u32_le(bytes, inputs_end);
    let final_rng_state = read_u32_le(bytes, inputs_end + 4);
    let checksum = read_u32_le(bytes, inputs_end + 8);

    let computed = crc32(&bytes[..inputs_end]);
    if checksum != computed {
        return Err(VerifyError::CrcMismatch {
            stored: checksum,
            computed,
        });
    }

    Ok(TapeView {
        header: TapeHeader {
            magic,
            version,
            seed,
            frame_count,
        },
        inputs,
        footer: TapeFooter {
            final_score,
            final_rng_state,
            checksum,
        },
    })
}

pub fn serialize_tape(seed: u32, inputs: &[u8], final_score: u32, final_rng_state: u32) -> Vec<u8> {
    let total_len = TAPE_HEADER_SIZE + inputs.len() + TAPE_FOOTER_SIZE;
    let mut data = vec![0u8; total_len];

    write_u32_le(&mut data, 0, TAPE_MAGIC);
    data[4] = TAPE_VERSION;
    data[5] = 0;
    data[6] = 0;
    data[7] = 0;
    write_u32_le(&mut data, 8, seed);
    write_u32_le(&mut data, 12, inputs.len() as u32);

    let body_start = TAPE_HEADER_SIZE;
    let body_end = body_start + inputs.len();
    data[body_start..body_end].copy_from_slice(inputs);

    write_u32_le(&mut data, body_end, final_score);
    write_u32_le(&mut data, body_end + 4, final_rng_state);

    let checksum = crc32(&data[..body_end]);
    write_u32_le(&mut data, body_end + 8, checksum);

    data
}

#[inline]
fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[inline]
fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;

        while j < 8 {
            c = if (c & 1) != 0 {
                0xEDB8_8320u32 ^ (c >> 1)
            } else {
                c >> 1
            };
            j += 1;
        }

        table[i] = c;
        i += 1;
    }

    table
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    for byte in data {
        let idx = ((crc ^ (*byte as u32)) & 0xFF) as usize;
        crc = CRC_TABLE[idx] ^ (crc >> 8);
    }

    crc ^ 0xFFFF_FFFFu32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn roundtrip_small_tape() {
        let inputs = [0x00u8, 0x09u8, 0x06u8];
        let bytes = serialize_tape(0xABCD_1234, &inputs, 777, 0x1111_2222);
        let tape = parse_tape(&bytes, 100).unwrap();

        assert_eq!(tape.header.seed, 0xABCD_1234);
        assert_eq!(tape.header.frame_count, 3);
        assert_eq!(tape.inputs, inputs);
        assert_eq!(tape.footer.final_score, 777);
        assert_eq!(tape.footer.final_rng_state, 0x1111_2222);
    }

    #[test]
    fn rejects_nonzero_header_reserved_bytes() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222);
        bytes[5] = 1;
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::HeaderReservedNonZero)
        ));
    }

    #[test]
    fn rejects_trailing_bytes_beyond_declared_frame_count() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222);
        bytes.push(0);
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::TapeLengthMismatch { .. })
        ));
    }
}
