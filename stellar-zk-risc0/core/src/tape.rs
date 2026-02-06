use crate::constants::*;
use crate::types::FrameInput;
use serde::{Deserialize, Serialize};

/// Tape header structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TapeHeader {
    pub magic: u32,
    pub version: u8,
    pub seed: u32,
    pub frame_count: u32,
}

impl TapeHeader {
    pub fn new(seed: u32, frame_count: u32) -> Self {
        TapeHeader {
            magic: TAPE_MAGIC,
            version: TAPE_VERSION,
            seed,
            frame_count,
        }
    }

    /// Serialize header to bytes (little-endian)
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.magic.to_le_bytes());
        bytes[4] = self.version;
        // bytes 5-7 reserved
        bytes[8..12].copy_from_slice(&self.seed.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.frame_count.to_le_bytes());
        bytes
    }

    /// Deserialize header from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TapeError> {
        if bytes.len() < HEADER_SIZE {
            return Err(TapeError::Truncated);
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let version = bytes[4];
        let seed = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let frame_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        Ok(TapeHeader {
            magic,
            version,
            seed,
            frame_count,
        })
    }

    /// Validate header
    pub fn validate(&self) -> Result<(), TapeError> {
        if self.magic != TAPE_MAGIC {
            return Err(TapeError::InvalidMagic(self.magic));
        }
        if self.version != TAPE_VERSION {
            return Err(TapeError::UnsupportedVersion(self.version));
        }
        if self.frame_count == 0 {
            return Err(TapeError::EmptyTape);
        }
        if self.frame_count > MAX_FRAMES {
            return Err(TapeError::TooManyFrames(self.frame_count));
        }
        Ok(())
    }
}

/// Tape footer structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TapeFooter {
    pub final_score: u32,
    pub final_rng_state: u32,
    pub checksum: u32,
}

impl TapeFooter {
    /// Serialize footer to bytes (little-endian)
    pub fn to_bytes(&self) -> [u8; FOOTER_SIZE] {
        let mut bytes = [0u8; FOOTER_SIZE];
        bytes[0..4].copy_from_slice(&self.final_score.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.final_rng_state.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.checksum.to_le_bytes());
        bytes
    }

    /// Deserialize footer from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TapeError> {
        if bytes.len() < FOOTER_SIZE {
            return Err(TapeError::Truncated);
        }

        let final_score = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let final_rng_state = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        Ok(TapeFooter {
            final_score,
            final_rng_state,
            checksum,
        })
    }
}

/// Full tape structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tape {
    pub header: TapeHeader,
    pub inputs: Vec<u8>,
    pub footer: TapeFooter,
}

impl Tape {
    /// Create new tape
    pub fn new(seed: u32, inputs: Vec<u8>, final_score: u32, final_rng_state: u32) -> Self {
        let frame_count = inputs.len() as u32;
        let header = TapeHeader::new(seed, frame_count);

        // Compute CRC32 over header + inputs
        let mut data_for_crc = Vec::with_capacity(HEADER_SIZE + inputs.len());
        data_for_crc.extend_from_slice(&header.to_bytes());
        data_for_crc.extend_from_slice(&inputs);
        let checksum = crc32(&data_for_crc);

        let footer = TapeFooter {
            final_score,
            final_rng_state,
            checksum,
        };

        Tape {
            header,
            inputs,
            footer,
        }
    }

    /// Deserialize tape from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, TapeError> {
        if data.len() < HEADER_SIZE + FOOTER_SIZE {
            return Err(TapeError::Truncated);
        }

        // Parse header
        let header = TapeHeader::from_bytes(&data[0..HEADER_SIZE])?;
        header.validate()?;

        let body_start = HEADER_SIZE;
        let body_end = HEADER_SIZE + header.frame_count as usize;
        let footer_start = body_end;

        if data.len() < footer_start + FOOTER_SIZE {
            return Err(TapeError::Truncated);
        }

        // Parse inputs
        let inputs = data[body_start..body_end].to_vec();

        // Parse footer
        let footer = TapeFooter::from_bytes(&data[footer_start..footer_start + FOOTER_SIZE])?;

        // Verify CRC32
        let data_for_crc = &data[0..footer_start];
        let computed_crc = crc32(data_for_crc);
        if computed_crc != footer.checksum {
            return Err(TapeError::CrcMismatch {
                expected: footer.checksum,
                computed: computed_crc,
            });
        }

        // Validate input bytes (reserved bits must be 0)
        for (i, &byte) in inputs.iter().enumerate() {
            if byte & 0xF0 != 0 {
                return Err(TapeError::ReservedBitsSet { frame: i as u32 });
            }
        }

        Ok(Tape {
            header,
            inputs,
            footer,
        })
    }

    /// Serialize tape to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(HEADER_SIZE + self.inputs.len() + FOOTER_SIZE);
        data.extend_from_slice(&self.header.to_bytes());
        data.extend_from_slice(&self.inputs);
        data.extend_from_slice(&self.footer.to_bytes());
        data
    }

    /// Get frame input at index
    pub fn get_input(&self, frame: usize) -> Option<FrameInput> {
        self.inputs.get(frame).map(|&b| FrameInput::from_byte(b))
    }

    /// Get total size in bytes
    pub fn total_size(&self) -> usize {
        HEADER_SIZE + self.inputs.len() + FOOTER_SIZE
    }
}

/// Tape parsing/validation errors
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TapeError {
    Truncated,
    InvalidMagic(u32),
    UnsupportedVersion(u8),
    EmptyTape,
    TooManyFrames(u32),
    CrcMismatch { expected: u32, computed: u32 },
    ReservedBitsSet { frame: u32 },
}

impl std::fmt::Display for TapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TapeError::Truncated => write!(f, "Tape truncated"),
            TapeError::InvalidMagic(m) => write!(f, "Invalid magic: 0x{:08x}", m),
            TapeError::UnsupportedVersion(v) => write!(f, "Unsupported version: {}", v),
            TapeError::EmptyTape => write!(f, "Empty tape (frame_count = 0)"),
            TapeError::TooManyFrames(n) => {
                write!(f, "Too many frames: {} (max: {})", n, MAX_FRAMES)
            }
            TapeError::CrcMismatch { expected, computed } => {
                write!(
                    f,
                    "CRC mismatch: expected 0x{:08x}, computed 0x{:08x}",
                    expected, computed
                )
            }
            TapeError::ReservedBitsSet { frame } => {
                write!(f, "Reserved bits set in input byte at frame {}", frame)
            }
        }
    }
}

impl std::error::Error for TapeError {}

/// CRC-32 implementation (ISO 3309 / ITU-T V.42)
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 {
            c = if c & 1 != 0 {
                0xedb88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            j += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// Compute CRC-32 of data
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffffffff;
    for &byte in data {
        crc = CRC_TABLE[((crc ^ byte as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xffffffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tape_roundtrip() {
        let inputs: Vec<u8> = vec![0x01, 0x02, 0x04, 0x08, 0x00];
        let tape = Tape::new(12345, inputs.clone(), 1000, 67890);

        let bytes = tape.to_bytes();
        let parsed = Tape::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.header.seed, 12345);
        assert_eq!(parsed.header.frame_count, 5);
        assert_eq!(parsed.inputs, inputs);
        assert_eq!(parsed.footer.final_score, 1000);
        assert_eq!(parsed.footer.final_rng_state, 67890);
    }

    #[test]
    fn test_crc32() {
        // Test vector: "123456789" should give 0xcbf43926
        let data = b"123456789";
        assert_eq!(crc32(data), 0xcbf43926);
    }

    #[test]
    fn test_invalid_magic() {
        let mut header = TapeHeader::new(12345, 1);
        header.magic = 0xDEADBEEF;

        let mut bytes = header.to_bytes().to_vec();
        bytes.push(0x00); // input
        bytes.extend_from_slice(&[0u8; FOOTER_SIZE]);

        let result = Tape::from_bytes(&bytes);
        assert!(matches!(result, Err(TapeError::InvalidMagic(0xDEADBEEF))));
    }

    #[test]
    fn test_reserved_bits() {
        let inputs: Vec<u8> = vec![0x10]; // Reserved bit set
        let tape = Tape::new(12345, inputs, 0, 0);
        let bytes = tape.to_bytes();

        let result = Tape::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(TapeError::ReservedBitsSet { frame: 0 })
        ));
    }

    #[test]
    fn test_crc_mismatch() {
        let inputs: Vec<u8> = vec![0x01, 0x02];
        let tape = Tape::new(12345, inputs, 100, 200);
        let mut bytes = tape.to_bytes();

        // Corrupt a byte
        bytes[HEADER_SIZE] = 0xFF;

        let result = Tape::from_bytes(&bytes);
        assert!(matches!(result, Err(TapeError::CrcMismatch { .. })));
    }
}
