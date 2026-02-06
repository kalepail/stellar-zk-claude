use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyError {
    TapeTooShort { actual: usize, min: usize },
    InvalidMagic { found: u32 },
    UnsupportedVersion { found: u8 },
    HeaderReservedNonZero,
    FrameCountOutOfRange { frame_count: u32, max_frames: u32 },
    TapeLengthMismatch { expected: usize, actual: usize },
    ReservedInputBitsNonZero { frame: u32, byte: u8 },
    CrcMismatch { stored: u32, computed: u32 },
    FrameCountMismatch { claimed: u32, computed: u32 },
    ScoreMismatch { claimed: u32, computed: u32 },
    RngMismatch { claimed: u32, computed: u32 },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TapeTooShort { actual, min } => {
                write!(f, "tape too short: got {actual} bytes, need at least {min}")
            }
            Self::InvalidMagic { found } => write!(f, "invalid tape magic: 0x{found:08x}"),
            Self::UnsupportedVersion { found } => write!(f, "unsupported tape version: {found}"),
            Self::HeaderReservedNonZero => write!(f, "header reserved bytes are non-zero"),
            Self::FrameCountOutOfRange {
                frame_count,
                max_frames,
            } => write!(
                f,
                "frame count out of range: {frame_count} (allowed 1..={max_frames})"
            ),
            Self::TapeLengthMismatch { expected, actual } => write!(
                f,
                "tape length mismatch: expected {expected} bytes, got {actual}"
            ),
            Self::ReservedInputBitsNonZero { frame, byte } => write!(
                f,
                "input byte reserved bits set at frame {frame}: 0x{byte:02x}"
            ),
            Self::CrcMismatch { stored, computed } => write!(
                f,
                "crc mismatch: stored=0x{stored:08x}, computed=0x{computed:08x}"
            ),
            Self::FrameCountMismatch { claimed, computed } => {
                write!(
                    f,
                    "frame-count mismatch: claimed={claimed}, computed={computed}"
                )
            }
            Self::ScoreMismatch { claimed, computed } => {
                write!(f, "score mismatch: claimed={claimed}, computed={computed}")
            }
            Self::RngMismatch { claimed, computed } => {
                write!(
                    f,
                    "rng mismatch: claimed=0x{claimed:08x}, computed=0x{computed:08x}"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VerifyError {}
