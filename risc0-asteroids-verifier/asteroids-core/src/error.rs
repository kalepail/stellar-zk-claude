use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleCode {
    GlobalModeLivesConsistency,
    GlobalWaveNonZero,
    GlobalNextExtraLifeScore,
    ProgressionScoreDelta,
    ProgressionWaveAdvance,
    ShipTurnRateStep,
    ShipSpeedClamp,
    ShipPositionStep,
    ShipBounds,
    ShipAngleRange,
    ShipCooldownRange,
    ShipRespawnTimerRange,
    ShipInvulnerabilityRange,
    PlayerBulletCooldownBypass,
    PlayerBulletLimit,
    PlayerBulletState,
    SaucerBulletLimit,
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
            Self::ProgressionScoreDelta => write!(f, "PROGRESSION_SCORE_DELTA"),
            Self::ProgressionWaveAdvance => write!(f, "PROGRESSION_WAVE_ADVANCE"),
            Self::ShipTurnRateStep => write!(f, "SHIP_TURN_RATE_STEP"),
            Self::ShipSpeedClamp => write!(f, "SHIP_SPEED_CLAMP"),
            Self::ShipPositionStep => write!(f, "SHIP_POSITION_STEP"),
            Self::ShipBounds => write!(f, "SHIP_BOUNDS"),
            Self::ShipAngleRange => write!(f, "SHIP_ANGLE_RANGE"),
            Self::ShipCooldownRange => write!(f, "SHIP_COOLDOWN_RANGE"),
            Self::ShipRespawnTimerRange => write!(f, "SHIP_RESPAWN_TIMER_RANGE"),
            Self::ShipInvulnerabilityRange => write!(f, "SHIP_INVULNERABILITY_RANGE"),
            Self::PlayerBulletCooldownBypass => write!(f, "PLAYER_BULLET_COOLDOWN_BYPASS"),
            Self::PlayerBulletLimit => write!(f, "PLAYER_BULLET_LIMIT"),
            Self::PlayerBulletState => write!(f, "PLAYER_BULLET_STATE"),
            Self::SaucerBulletLimit => write!(f, "SAUCER_BULLET_LIMIT"),
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
    UnknownRulesTag { found: u8 },
    HeaderReservedNonZero,
    FrameCountOutOfRange { frame_count: u32, max_frames: u32 },
    TapeLengthMismatch { expected: usize, actual: usize },
    ReservedInputBitsNonZero { frame: u32, byte: u8 },
    CrcMismatch { stored: u32, computed: u32 },
    RuleViolation { frame: u32, rule: RuleCode },
    FrameCountMismatch { claimed: u32, computed: u32 },
    ScoreMismatch { claimed: u32, computed: u32 },
    RngMismatch { claimed: u32, computed: u32 },
    InvalidClaimantAddressUtf8,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TapeTooShort { actual, min } => {
                write!(f, "tape too short: got {actual} bytes, need at least {min}")
            }
            Self::InvalidMagic { found } => write!(f, "invalid tape magic: 0x{found:08x}"),
            Self::UnsupportedVersion { found } => write!(f, "unsupported tape version: {found}"),
            Self::UnknownRulesTag { found } => write!(f, "unknown rules tag: {found}"),
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
            Self::InvalidClaimantAddressUtf8 => {
                write!(f, "claimant address is not valid UTF-8")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VerifyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_code_display_is_stable() {
        assert_eq!(
            RuleCode::GlobalModeLivesConsistency.to_string(),
            "GLOBAL_MODE_LIVES_CONSISTENCY"
        );
        assert_eq!(
            RuleCode::GlobalWaveNonZero.to_string(),
            "GLOBAL_WAVE_NONZERO"
        );
        assert_eq!(
            RuleCode::GlobalNextExtraLifeScore.to_string(),
            "GLOBAL_NEXT_EXTRA_LIFE_SCORE"
        );
        assert_eq!(
            RuleCode::ProgressionScoreDelta.to_string(),
            "PROGRESSION_SCORE_DELTA"
        );
        assert_eq!(
            RuleCode::ProgressionWaveAdvance.to_string(),
            "PROGRESSION_WAVE_ADVANCE"
        );
        assert_eq!(
            RuleCode::ShipTurnRateStep.to_string(),
            "SHIP_TURN_RATE_STEP"
        );
        assert_eq!(RuleCode::ShipSpeedClamp.to_string(), "SHIP_SPEED_CLAMP");
        assert_eq!(RuleCode::ShipPositionStep.to_string(), "SHIP_POSITION_STEP");
        assert_eq!(RuleCode::ShipBounds.to_string(), "SHIP_BOUNDS");
        assert_eq!(RuleCode::ShipAngleRange.to_string(), "SHIP_ANGLE_RANGE");
        assert_eq!(
            RuleCode::ShipCooldownRange.to_string(),
            "SHIP_COOLDOWN_RANGE"
        );
        assert_eq!(
            RuleCode::ShipRespawnTimerRange.to_string(),
            "SHIP_RESPAWN_TIMER_RANGE"
        );
        assert_eq!(
            RuleCode::ShipInvulnerabilityRange.to_string(),
            "SHIP_INVULNERABILITY_RANGE"
        );
        assert_eq!(
            RuleCode::PlayerBulletCooldownBypass.to_string(),
            "PLAYER_BULLET_COOLDOWN_BYPASS"
        );
        assert_eq!(
            RuleCode::PlayerBulletLimit.to_string(),
            "PLAYER_BULLET_LIMIT"
        );
        assert_eq!(
            RuleCode::PlayerBulletState.to_string(),
            "PLAYER_BULLET_STATE"
        );
        assert_eq!(
            RuleCode::SaucerBulletLimit.to_string(),
            "SAUCER_BULLET_LIMIT"
        );
        assert_eq!(
            RuleCode::SaucerBulletState.to_string(),
            "SAUCER_BULLET_STATE"
        );
        assert_eq!(RuleCode::AsteroidState.to_string(), "ASTEROID_STATE");
        assert_eq!(RuleCode::SaucerState.to_string(), "SAUCER_STATE");
        assert_eq!(RuleCode::SaucerCap.to_string(), "SAUCER_CAP");
    }

    #[test]
    fn verify_error_display_includes_context() {
        assert!(VerifyError::TapeTooShort { actual: 7, min: 28 }
            .to_string()
            .contains("need at least 28"));
        assert!(VerifyError::InvalidMagic { found: 0xDEAD_BEEF }
            .to_string()
            .contains("0xdeadbeef"));
        assert!(VerifyError::UnsupportedVersion { found: 9 }
            .to_string()
            .contains("unsupported tape version"));
        assert!(VerifyError::UnknownRulesTag { found: 99 }
            .to_string()
            .contains("unknown rules tag"));
        assert_eq!(
            VerifyError::HeaderReservedNonZero.to_string(),
            "header reserved bytes are non-zero"
        );
        assert!(VerifyError::FrameCountOutOfRange {
            frame_count: 20_001,
            max_frames: 18_000
        }
        .to_string()
        .contains("allowed 1..=18000"));
        assert!(VerifyError::TapeLengthMismatch {
            expected: 32,
            actual: 31
        }
        .to_string()
        .contains("expected 32 bytes"));
        assert!(VerifyError::ReservedInputBitsNonZero {
            frame: 3,
            byte: 0xF0
        }
        .to_string()
        .contains("frame 3"));
        assert!(VerifyError::CrcMismatch {
            stored: 0x1234_5678,
            computed: 0xDEAD_BEEF
        }
        .to_string()
        .contains("stored=0x12345678"));
        assert!(VerifyError::RuleViolation {
            frame: 12,
            rule: RuleCode::ShipBounds
        }
        .to_string()
        .contains("rule violation at frame 12"));
        assert!(VerifyError::FrameCountMismatch {
            claimed: 10,
            computed: 9
        }
        .to_string()
        .contains("claimed=10"));
        assert!(VerifyError::ScoreMismatch {
            claimed: 100,
            computed: 99
        }
        .to_string()
        .contains("score mismatch"));
        assert!(VerifyError::RngMismatch {
            claimed: 0xABCD_EF01,
            computed: 0x1020_3040
        }
        .to_string()
        .contains("claimed=0xabcdef01"));
        assert!(VerifyError::InvalidClaimantAddressUtf8
            .to_string()
            .contains("UTF-8"));
    }

    #[cfg(feature = "std")]
    #[test]
    fn verify_error_implements_std_error() {
        fn assert_is_std_error<T: std::error::Error>() {}
        assert_is_std_error::<VerifyError>();
    }
}
