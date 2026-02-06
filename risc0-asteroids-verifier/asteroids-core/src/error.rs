use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleCode {
    GlobalModeLivesConsistency,
    GlobalWaveNonZero,
    GlobalNextExtraLifeScore,
    ShipBounds,
    ShipAngleRange,
    ShipCooldownRange,
    ShipRespawnTimerRange,
    ShipInvulnerabilityRange,
    PlayerBulletLimit,
    PlayerBulletState,
    SaucerBulletState,
    AsteroidState,
    SaucerState,
    SaucerCap,
}

impl fmt::Display for RuleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GlobalModeLivesConsistency => write!(f, "GLOBAL_MODE_LIVES_CONSISTENCY"),
            Self::GlobalWaveNonZero => write!(f, "GLOBAL_WAVE_NONZERO"),
            Self::GlobalNextExtraLifeScore => write!(f, "GLOBAL_NEXT_EXTRA_LIFE_SCORE"),
            Self::ShipBounds => write!(f, "SHIP_BOUNDS"),
            Self::ShipAngleRange => write!(f, "SHIP_ANGLE_RANGE"),
            Self::ShipCooldownRange => write!(f, "SHIP_COOLDOWN_RANGE"),
            Self::ShipRespawnTimerRange => write!(f, "SHIP_RESPAWN_TIMER_RANGE"),
            Self::ShipInvulnerabilityRange => write!(f, "SHIP_INVULNERABILITY_RANGE"),
            Self::PlayerBulletLimit => write!(f, "PLAYER_BULLET_LIMIT"),
            Self::PlayerBulletState => write!(f, "PLAYER_BULLET_STATE"),
            Self::SaucerBulletState => write!(f, "SAUCER_BULLET_STATE"),
            Self::AsteroidState => write!(f, "ASTEROID_STATE"),
            Self::SaucerState => write!(f, "SAUCER_STATE"),
            Self::SaucerCap => write!(f, "SAUCER_CAP"),
        }
    }
}

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
    RuleViolation { frame: u32, rule: RuleCode },
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
            Self::RuleViolation { frame, rule } => {
                write!(f, "rule violation at frame {frame}: {rule}")
            }
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
