use alloc::{vec, vec::Vec};
use serde::{Deserialize, Serialize};

use crate::constants::{
    CLAIMANT_ADDRESS_SIZE, RULES_TAG, TAPE_FOOTER_SIZE, TAPE_HEADER_SIZE, TAPE_MAGIC, TAPE_VERSION,
};
use crate::error::VerifyError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeHeader {
    pub magic: u32,
    pub version: u8,
    pub rules_tag: u8,
    pub seed: u32,
    pub frame_count: u32,
    pub claimant_address: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeFooter {
    pub final_score: u32,
    pub final_rng_state: u32,
    pub checksum: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

    let rules_tag = bytes[5];
    if rules_tag != 0 && rules_tag != RULES_TAG {
        return Err(VerifyError::UnknownRulesTag { found: rules_tag });
    }
    if bytes[6] != 0 || bytes[7] != 0 {
        return Err(VerifyError::HeaderReservedNonZero);
    }

    let seed = read_u32_le(bytes, 8);
    let frame_count = read_u32_le(bytes, 12);

    // Read claimant address: 56 bytes at offset 16, trim trailing zeros
    let claimant_raw = &bytes[16..16 + CLAIMANT_ADDRESS_SIZE];
    let claimant_end = claimant_raw
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    let claimant_address = claimant_raw[..claimant_end].to_vec();

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

    let final_score = read_u32_le(bytes, inputs_end);
    let final_rng_state = read_u32_le(bytes, inputs_end + 4);
    let checksum = read_u32_le(bytes, inputs_end + 8);

    let computed = crc32_and_validate_inputs(bytes, inputs_start, inputs_end)?;
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
            rules_tag,
            seed,
            frame_count,
            claimant_address,
        },
        inputs,
        footer: TapeFooter {
            final_score,
            final_rng_state,
            checksum,
        },
    })
}

pub fn serialize_tape(
    seed: u32,
    inputs: &[u8],
    final_score: u32,
    final_rng_state: u32,
    claimant_address: &[u8],
) -> Vec<u8> {
    let total_len = TAPE_HEADER_SIZE + inputs.len() + TAPE_FOOTER_SIZE;
    let mut data = vec![0u8; total_len];

    write_u32_le(&mut data, 0, TAPE_MAGIC);
    data[4] = TAPE_VERSION;
    data[5] = RULES_TAG;
    data[6] = 0;
    data[7] = 0;
    write_u32_le(&mut data, 8, seed);
    write_u32_le(&mut data, 12, inputs.len() as u32);

    // Claimant address: 56 bytes at offset 16, zero-padded
    let claimant_len = claimant_address.len().min(CLAIMANT_ADDRESS_SIZE);
    data[16..16 + claimant_len].copy_from_slice(&claimant_address[..claimant_len]);

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

fn crc32_and_validate_inputs(
    bytes: &[u8],
    inputs_start: usize,
    inputs_end: usize,
) -> Result<u32, VerifyError> {
    let mut crc = 0xFFFF_FFFFu32;
    let mut i = 0usize;

    while i < inputs_end {
        let byte = bytes[i];
        if i >= inputs_start && (byte & 0xF0) != 0 {
            return Err(VerifyError::ReservedInputBitsNonZero {
                frame: (i - inputs_start) as u32,
                byte,
            });
        }

        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC_TABLE[idx] ^ (crc >> 8);
        i += 1;
    }

    Ok(crc ^ 0xFFFF_FFFFu32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn footer_offset(frame_count: usize) -> usize {
        TAPE_HEADER_SIZE + frame_count
    }

    #[test]
    fn crc_matches_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn input_byte_roundtrip_for_all_valid_bit_patterns() {
        for byte in 0u8..=0x0F {
            assert_eq!(encode_input_byte(decode_input_byte(byte)), byte);
        }
    }

    #[test]
    fn roundtrip_small_tape() {
        let inputs = [0x00u8, 0x09u8, 0x06u8];
        let bytes = serialize_tape(0xABCD_1234, &inputs, 777, 0x1111_2222, b"");
        let tape = parse_tape(&bytes, 100).unwrap();

        assert_eq!(tape.header.seed, 0xABCD_1234);
        assert_eq!(tape.header.frame_count, 3);
        assert_eq!(tape.inputs, inputs);
        assert_eq!(tape.footer.final_score, 777);
        assert_eq!(tape.footer.final_rng_state, 0x1111_2222);
    }

    #[test]
    fn rejects_tape_too_short() {
        let bytes = [0u8; TAPE_HEADER_SIZE + TAPE_FOOTER_SIZE - 1];
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::TapeTooShort { .. })
        ));
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[0] ^= 0x01;
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[4] = TAPE_VERSION + 1;
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn rejects_unknown_rules_tag() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[5] = 255; // unknown tag
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::UnknownRulesTag { found: 255 })
        ));
    }

    #[test]
    fn accepts_legacy_zero_rules_tag() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[5] = 0; // legacy tape
        // Must recompute CRC since we changed the header
        let footer_off = TAPE_HEADER_SIZE + 1;
        let checksum = crc32(&bytes[..footer_off]);
        write_u32_le(&mut bytes, footer_off + 8, checksum);
        assert!(parse_tape(&bytes, 100).is_ok());
    }

    #[test]
    fn rejects_nonzero_header_reserved_bytes() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[6] = 1; // reserved byte [6]
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::HeaderReservedNonZero)
        ));
    }

    #[test]
    fn rejects_zero_frame_count() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[12..16].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::FrameCountOutOfRange {
                frame_count: 0,
                max_frames: 100
            })
        ));
    }

    #[test]
    fn rejects_frame_count_above_max() {
        let bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        assert!(matches!(
            parse_tape(&bytes, 0),
            Err(VerifyError::FrameCountOutOfRange {
                frame_count: 1,
                max_frames: 0
            })
        ));
    }

    #[test]
    fn rejects_trailing_bytes_beyond_declared_frame_count() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes.push(0);
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::TapeLengthMismatch { .. })
        ));
    }

    #[test]
    fn rejects_shorter_than_declared_frame_count() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8, 0x00u8], 0, 0x1111_2222, b"");
        bytes.pop();
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::TapeLengthMismatch { .. })
        ));
    }

    #[test]
    fn rejects_reserved_input_bits_nonzero() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        bytes[TAPE_HEADER_SIZE] = 0x80;
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::ReservedInputBitsNonZero {
                frame: 0,
                byte: 0x80
            })
        ));
    }

    #[test]
    fn rejects_crc_mismatch() {
        let mut bytes = serialize_tape(0xABCD_1234, &[0x00u8], 0, 0x1111_2222, b"");
        let checksum_offset = footer_offset(1) + 8;
        bytes[checksum_offset] ^= 0x01;
        assert!(matches!(
            parse_tape(&bytes, 100),
            Err(VerifyError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn claimant_g_address_roundtrips() {
        let g_addr = b"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert_eq!(g_addr.len(), 56);
        let inputs = [0x00u8, 0x01];
        let bytes = serialize_tape(0x1111, &inputs, 42, 0x2222, g_addr);
        let tape = parse_tape(&bytes, 100).unwrap();
        assert_eq!(tape.header.claimant_address, g_addr.to_vec());
    }

    #[test]
    fn claimant_c_address_roundtrips() {
        let c_addr = b"CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert_eq!(c_addr.len(), 56);
        let inputs = [0x00u8, 0x01];
        let bytes = serialize_tape(0x1111, &inputs, 42, 0x2222, c_addr);
        let tape = parse_tape(&bytes, 100).unwrap();
        assert_eq!(tape.header.claimant_address, c_addr.to_vec());
    }

    #[test]
    fn claimant_empty_roundtrips_as_empty() {
        let inputs = [0x00u8, 0x01];
        let bytes = serialize_tape(0x1111, &inputs, 42, 0x2222, b"");
        let tape = parse_tape(&bytes, 100).unwrap();
        assert!(tape.header.claimant_address.is_empty());
    }

    #[test]
    fn claimant_shorter_than_56_bytes_is_zero_padded() {
        let short = b"GSHORT";
        let inputs = [0x00u8];
        let bytes = serialize_tape(0x1111, &inputs, 0, 0x2222, short);
        // Verify the raw bytes: 6 bytes of "GSHORT" then 50 zeros
        assert_eq!(&bytes[16..22], b"GSHORT");
        assert!(bytes[22..72].iter().all(|&b| b == 0));
        // Parse trims trailing zeros
        let tape = parse_tape(&bytes, 100).unwrap();
        assert_eq!(tape.header.claimant_address, b"GSHORT".to_vec());
    }

    #[test]
    fn claimant_exactly_56_bytes_no_padding() {
        let full = b"GABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVW";
        assert_eq!(full.len(), 56);
        let inputs = [0x00u8];
        let bytes = serialize_tape(0x1111, &inputs, 0, 0x2222, full);
        let tape = parse_tape(&bytes, 100).unwrap();
        assert_eq!(tape.header.claimant_address, full.to_vec());
    }

    #[test]
    fn claimant_longer_than_56_bytes_is_truncated() {
        let long = b"GABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ_EXTRA";
        assert!(long.len() > 56);
        let inputs = [0x00u8];
        let bytes = serialize_tape(0x1111, &inputs, 0, 0x2222, long);
        let tape = parse_tape(&bytes, 100).unwrap();
        assert_eq!(tape.header.claimant_address.len(), 56);
        assert_eq!(tape.header.claimant_address, long[..56].to_vec());
    }

    #[test]
    fn serialize_tape_writes_crc_over_header_and_body() {
        let inputs = [0x01u8, 0x02u8, 0x04u8, 0x08u8];
        let bytes = serialize_tape(0xABCD_1234, &inputs, 77, 0xCAFEBABE, b"");
        let checksum_offset = footer_offset(inputs.len()) + 8;
        let stored = u32::from_le_bytes([
            bytes[checksum_offset],
            bytes[checksum_offset + 1],
            bytes[checksum_offset + 2],
            bytes[checksum_offset + 3],
        ]);
        assert_eq!(stored, crc32(&bytes[..footer_offset(inputs.len())]));
    }
}
