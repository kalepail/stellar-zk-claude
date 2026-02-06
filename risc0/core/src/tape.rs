//! Tape binary format parser and CRC-32 - exact match to TypeScript tape.ts
//!
//! Layout (little-endian):
//!   HEADER (16 bytes): magic(u32) version(u8) reserved(3) seed(u32) frameCount(u32)
//!   BODY (frameCount bytes): one input byte per frame
//!   FOOTER (12 bytes): finalScore(u32) finalRngState(u32) checksum(u32)

extern crate alloc;
use alloc::vec::Vec;

pub const TAPE_MAGIC: u32 = 0x5A4B5450; // "ZKTP"
pub const TAPE_VERSION: u8 = 1;
const HEADER_SIZE: usize = 16;
const FOOTER_SIZE: usize = 12;

#[derive(Debug, Clone)]
pub struct TapeHeader {
    pub magic: u32,
    pub version: u8,
    pub seed: u32,
    pub frame_count: u32,
}

#[derive(Debug, Clone)]
pub struct TapeFooter {
    pub final_score: u32,
    pub final_rng_state: u32,
    pub checksum: u32,
}

#[derive(Debug, Clone)]
pub struct Tape {
    pub header: TapeHeader,
    pub inputs: Vec<u8>,
    pub footer: TapeFooter,
}

/// Errors that can occur during tape deserialization.
#[derive(Debug)]
pub enum TapeError {
    TooShort,
    InvalidMagic(u32),
    UnsupportedVersion(u8),
    Truncated { expected: usize, got: usize },
    TrailingData { expected: usize, got: usize },
    CrcMismatch { stored: u32, computed: u32 },
    ReservedBitsSet { frame: u32, byte: u8 },
}

impl core::fmt::Display for TapeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TapeError::TooShort => write!(f, "Tape too short"),
            TapeError::InvalidMagic(m) => write!(f, "Invalid tape magic: 0x{m:08x}"),
            TapeError::UnsupportedVersion(v) => write!(f, "Unsupported tape version: {v}"),
            TapeError::Truncated { expected, got } => {
                write!(f, "Tape truncated: expected {expected} bytes, got {got}")
            }
            TapeError::TrailingData { expected, got } => {
                write!(f, "Tape has trailing data: expected {expected} bytes, got {got}")
            }
            TapeError::CrcMismatch { stored, computed } => {
                write!(f, "CRC mismatch: stored=0x{stored:08x}, computed=0x{computed:08x}")
            }
            TapeError::ReservedBitsSet { frame, byte } => {
                write!(f, "Reserved bits set in frame {frame}: 0x{byte:02x}")
            }
        }
    }
}

/// Read a little-endian u32 from a byte slice at the given offset.
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Parse a tape from raw bytes. Validates magic, version, length, and reserved bits,
/// but does NOT verify the CRC-32 checksum. Suitable for the ZK guest where CRC
/// verification is unnecessary (the deterministic replay itself is the integrity check).
pub fn parse_tape(data: &[u8]) -> Result<Tape, TapeError> {
    if data.len() < HEADER_SIZE + FOOTER_SIZE {
        return Err(TapeError::TooShort);
    }

    let magic = read_u32_le(data, 0);
    if magic != TAPE_MAGIC {
        return Err(TapeError::InvalidMagic(magic));
    }

    let version = data[4];
    if version != TAPE_VERSION {
        return Err(TapeError::UnsupportedVersion(version));
    }

    let seed = read_u32_le(data, 8);
    let frame_count = read_u32_le(data, 12);

    let expected_len = HEADER_SIZE + frame_count as usize + FOOTER_SIZE;
    if data.len() < expected_len {
        return Err(TapeError::Truncated {
            expected: expected_len,
            got: data.len(),
        });
    }
    if data.len() > expected_len {
        return Err(TapeError::TrailingData {
            expected: expected_len,
            got: data.len(),
        });
    }

    let input_slice = &data[HEADER_SIZE..HEADER_SIZE + frame_count as usize];

    // V-1: Validate reserved bits (upper 4 bits must be zero)
    for (i, &byte) in input_slice.iter().enumerate() {
        if byte & 0xF0 != 0 {
            return Err(TapeError::ReservedBitsSet { frame: i as u32, byte });
        }
    }

    let inputs = input_slice.to_vec();

    let footer_offset = HEADER_SIZE + frame_count as usize;
    let final_score = read_u32_le(data, footer_offset);
    let final_rng_state = read_u32_le(data, footer_offset + 4);
    let stored_checksum = read_u32_le(data, footer_offset + 8);

    Ok(Tape {
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
            checksum: stored_checksum,
        },
    })
}

// ============================================================================
// CRC-32 and full deserialization (host-side only, not needed in ZK guest)
// ============================================================================

/// Deserialize a tape from raw bytes. Validates magic, version, length, reserved bits,
/// and CRC-32 checksum. Use this for host-side validation where CRC integrity matters.
#[cfg(feature = "std")]
pub fn deserialize_tape(data: &[u8]) -> Result<Tape, TapeError> {
    let tape = parse_tape(data)?;

    // CRC-32 over header + body (everything before the footer)
    let footer_offset = HEADER_SIZE + tape.header.frame_count as usize;
    let computed = crc32(&data[..footer_offset]);
    if computed != tape.footer.checksum {
        return Err(TapeError::CrcMismatch {
            stored: tape.footer.checksum,
            computed,
        });
    }

    Ok(tape)
}

// CRC-32 (ISO 3309 / ITU-T V.42 polynomial 0xEDB88320, reflected)
// Exact match to the TypeScript implementation.
// Only compiled when `std` feature is enabled (host-side).

#[cfg(feature = "std")]
const CRC_TABLE: [u32; 256] = build_crc_table();

#[cfg(feature = "std")]
const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut c = i;
        let mut j = 0;
        while j < 8 {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            j += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
}

/// Compute CRC-32 checksum over a byte slice.
/// Matches the TypeScript crc32() function exactly.
#[cfg(feature = "std")]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc = CRC_TABLE[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(crc32(&[]), 0x00000000);
    }

    #[test]
    fn test_crc32_known() {
        // CRC-32 of "123456789" = 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_deserialize_roundtrip() {
        // Build a minimal valid tape
        let seed: u32 = 0xDEADBEEF;
        let inputs = vec![0x05u8, 0x0A, 0x00]; // 3 frames
        let frame_count: u32 = 3;
        let final_score: u32 = 42;
        let final_rng_state: u32 = 0x12345678;

        let mut data = Vec::new();

        // Header
        data.extend_from_slice(&TAPE_MAGIC.to_le_bytes());
        data.push(TAPE_VERSION);
        data.extend_from_slice(&[0, 0, 0]); // reserved
        data.extend_from_slice(&seed.to_le_bytes());
        data.extend_from_slice(&frame_count.to_le_bytes());

        // Body
        data.extend_from_slice(&inputs);

        // Compute CRC over header+body
        let checksum = crc32(&data);

        // Footer
        data.extend_from_slice(&final_score.to_le_bytes());
        data.extend_from_slice(&final_rng_state.to_le_bytes());
        data.extend_from_slice(&checksum.to_le_bytes());

        let tape = deserialize_tape(&data).expect("should parse");
        assert_eq!(tape.header.seed, seed);
        assert_eq!(tape.header.frame_count, frame_count);
        assert_eq!(tape.inputs, inputs);
        assert_eq!(tape.footer.final_score, final_score);
        assert_eq!(tape.footer.final_rng_state, final_rng_state);
    }

    #[test]
    fn test_bad_magic() {
        let data = vec![0; HEADER_SIZE + FOOTER_SIZE];
        assert!(matches!(
            deserialize_tape(&data),
            Err(TapeError::InvalidMagic(0))
        ));
    }

    #[test]
    fn test_reserved_bits_rejected() {
        let seed: u32 = 0xDEADBEEF;
        let inputs = vec![0x05u8, 0x1A, 0x00]; // 0x1A has bit 4 set (reserved)
        let frame_count: u32 = 3;

        let mut data = Vec::new();
        data.extend_from_slice(&TAPE_MAGIC.to_le_bytes());
        data.push(TAPE_VERSION);
        data.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&seed.to_le_bytes());
        data.extend_from_slice(&frame_count.to_le_bytes());
        data.extend_from_slice(&inputs);

        let checksum = crc32(&data);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&checksum.to_le_bytes());

        assert!(matches!(
            deserialize_tape(&data),
            Err(TapeError::ReservedBitsSet { frame: 1, byte: 0x1A })
        ));
    }
}
