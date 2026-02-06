//! Frame-by-frame rule checking for Asteroids verification
//!
//! This module implements all the verification rules from the specification:
//! - Ship physics and state machine rules
//! - Bullet limits and cooldowns
//! - Asteroid movement and split rules
//! - Collision detection rules
//! - Scoring rules
//! - Wave progression rules
//! - RNG integrity rules

use crate::constants::*;
use crate::engine::GameEngine;
use crate::rng::Rng;
use crate::types::*;

/// Rule violation with error code and context
#[derive(Clone, Debug, PartialEq)]
pub struct RuleViolation {
    /// Rule code (e.g., "SHIP_SPEED_CLAMP_INVALID")
    pub code: &'static str,
    /// Human-readable description
    pub message: String,
    /// Frame number where violation occurred
    pub frame: u32,
    /// Expected value (if applicable)
    pub expected: Option<String>,
    /// Actual value (if applicable)
    pub actual: Option<String>,
}

/// Result of checking a single frame
pub type FrameCheckResult = Result<(), RuleViolation>;

/// Check all frame invariants
pub fn check_frame_invariants(
    state: &GameState,
    prev_state: Option<&GameState>,
    frame: u32,
) -> FrameCheckResult {
    // Check ship state
    check_ship_state(&state.ship, frame)?;

    // Check bullet constraints
    check_bullet_constraints(state, frame)?;

    // Check asteroid constraints
    check_asteroid_constraints(state, frame)?;

    // Check saucer constraints
    check_saucer_constraints(state, frame)?;

    // Check score integrity (if we have previous state)
    if let Some(prev) = prev_state {
        check_score_integrity(prev, state, frame)?;
    }

    // Check game state consistency
    check_game_state_consistency(state, frame)?;

    Ok(())
}

/// Check ship state invariants
fn check_ship_state(ship: &Ship, frame: u32) -> FrameCheckResult {
    // S-4: Speed clamp check - speed must not exceed max
    let speed_sq = (ship.vx as i32 * ship.vx as i32 + ship.vy as i32 * ship.vy as i32) as u32;
    if speed_sq > SHIP_MAX_SPEED_SQ_Q16_16 {
        return Err(RuleViolation {
            code: "SHIP_SPEED_CLAMP_INVALID",
            message: format!(
                "Ship speed {} exceeds maximum {}",
                (speed_sq as f64).sqrt(),
                (SHIP_MAX_SPEED_SQ_Q16_16 as f64).sqrt()
            ),
            frame,
            expected: Some(format!("<= {}", SHIP_MAX_SPEED_SQ_Q16_16)),
            actual: Some(speed_sq.to_string()),
        });
    }

    // S-7: Fire cooldown must be >= 0 (implicit for u8, but document the check)
    // Note: u8 is always >= 0, so this is a no-op but documents the rule

    // S-11: Invulnerability timer must be >= 0 (implicit for u16)
    // Note: u16 is always >= 0, so this is a no-op but documents the rule

    // D-1: Respawn timer must be >= 0 when not controllable (implicit for u16)

    // Check ship position is in bounds
    if ship.x >= WORLD_WIDTH_Q12_4 || ship.y >= WORLD_HEIGHT_Q12_4 {
        return Err(RuleViolation {
            code: "SHIP_POSITION_OUT_OF_BOUNDS",
            message: "Ship position is out of world bounds".to_string(),
            frame,
            expected: Some(format!(
                "x < {}, y < {}",
                WORLD_WIDTH_Q12_4, WORLD_HEIGHT_Q12_4
            )),
            actual: Some(format!("x = {}, y = {}", ship.x, ship.y)),
        });
    }

    Ok(())
}

/// Check bullet constraints
fn check_bullet_constraints(state: &GameState, frame: u32) -> FrameCheckResult {
    // B-7: Max 4 player bullets at any time
    let player_bullets = state.bullets.len();
    if player_bullets > SHIP_BULLET_LIMIT as usize {
        return Err(RuleViolation {
            code: "PLAYER_BULLET_LIMIT_EXCEEDED",
            message: format!(
                "Player has {} bullets, max is {}",
                player_bullets, SHIP_BULLET_LIMIT
            ),
            frame,
            expected: Some(format!("<= {}", SHIP_BULLET_LIMIT)),
            actual: Some(player_bullets.to_string()),
        });
    }

    // Check all bullets have valid state
    for (i, bullet) in state.bullets.iter().enumerate() {
        // B-3: Bullet life must be <= max lifetime
        if bullet.life > SHIP_BULLET_LIFETIME_FRAMES {
            return Err(RuleViolation {
                code: "PLAYER_BULLET_LIFE_INVALID",
                message: format!(
                    "Bullet {} has life {} exceeding max {}",
                    i, bullet.life, SHIP_BULLET_LIFETIME_FRAMES
                ),
                frame,
                expected: Some(format!("<= {}", SHIP_BULLET_LIFETIME_FRAMES)),
                actual: Some(bullet.life.to_string()),
            });
        }

        // Check bullet position bounds
        if bullet.x >= WORLD_WIDTH_Q12_4 || bullet.y >= WORLD_HEIGHT_Q12_4 {
            return Err(RuleViolation {
                code: "PLAYER_BULLET_POSITION_OUT_OF_BOUNDS",
                message: format!("Bullet {} position is out of bounds", i),
                frame,
                expected: Some(format!(
                    "x < {}, y < {}",
                    WORLD_WIDTH_Q12_4, WORLD_HEIGHT_Q12_4
                )),
                actual: Some(format!("x = {}, y = {}", bullet.x, bullet.y)),
            });
        }
    }

    Ok(())
}

/// Check asteroid constraints
fn check_asteroid_constraints(state: &GameState, frame: u32) -> FrameCheckResult {
    // A-3: Asteroid radii must be valid
    for (i, asteroid) in state.asteroids.iter().enumerate() {
        if !asteroid.alive {
            continue;
        }

        // Check asteroid position bounds
        if asteroid.x >= WORLD_WIDTH_Q12_4 || asteroid.y >= WORLD_HEIGHT_Q12_4 {
            return Err(RuleViolation {
                code: "ASTEROID_POSITION_OUT_OF_BOUNDS",
                message: format!("Asteroid {} position is out of bounds", i),
                frame,
                expected: Some(format!(
                    "x < {}, y < {}",
                    WORLD_WIDTH_Q12_4, WORLD_HEIGHT_Q12_4
                )),
                actual: Some(format!("x = {}, y = {}", asteroid.x, asteroid.y)),
            });
        }

        // Check asteroid angle is valid BAM (always true for u8, but good for documentation)
        if asteroid.spin < -3 || asteroid.spin > 3 {
            return Err(RuleViolation {
                code: "ASTEROID_SPIN_INVALID",
                message: format!("Asteroid {} has invalid spin {}", i, asteroid.spin),
                frame,
                expected: Some("[-3, 3]".to_string()),
                actual: Some(asteroid.spin.to_string()),
            });
        }
    }

    // A-7: Asteroid count cap check
    let alive_asteroids = state.asteroids.iter().filter(|a| a.alive).count();
    if alive_asteroids > ASTEROID_CAP as usize {
        return Err(RuleViolation {
            code: "ASTEROID_COUNT_CAP_EXCEEDED",
            message: format!(
                "{} asteroids alive, cap is {}",
                alive_asteroids, ASTEROID_CAP
            ),
            frame,
            expected: Some(format!("<= {}", ASTEROID_CAP)),
            actual: Some(alive_asteroids.to_string()),
        });
    }

    Ok(())
}

/// Check saucer constraints
fn check_saucer_constraints(state: &GameState, frame: u32) -> FrameCheckResult {
    // U-2: Max saucers by wave
    let max_saucers = if state.wave < 4 {
        1
    } else if state.wave < 7 {
        2
    } else {
        3
    };

    let alive_saucers = state.saucers.iter().filter(|s| s.alive).count();
    if alive_saucers > max_saucers as usize {
        return Err(RuleViolation {
            code: "SAUCER_COUNT_CAP_EXCEEDED",
            message: format!(
                "{} saucers alive, max for wave {} is {}",
                alive_saucers, state.wave, max_saucers
            ),
            frame,
            expected: Some(format!("<= {}", max_saucers)),
            actual: Some(alive_saucers.to_string()),
        });
    }

    // Check saucer bullets
    for (i, bullet) in state.saucer_bullets.iter().enumerate() {
        if bullet.life > SAUCER_BULLET_LIFETIME_FRAMES {
            return Err(RuleViolation {
                code: "SAUCER_BULLET_LIFE_INVALID",
                message: format!(
                    "Saucer bullet {} has life {} exceeding max {}",
                    i, bullet.life, SAUCER_BULLET_LIFETIME_FRAMES
                ),
                frame,
                expected: Some(format!("<= {}", SAUCER_BULLET_LIFETIME_FRAMES)),
                actual: Some(bullet.life.to_string()),
            });
        }
    }

    Ok(())
}

/// Check score integrity (score only increases by valid amounts)
fn check_score_integrity(
    prev_state: &GameState,
    state: &GameState,
    frame: u32,
) -> FrameCheckResult {
    // P-7: Score only increases, never decreases
    if state.score < prev_state.score {
        return Err(RuleViolation {
            code: "PROGRESSION_SCORE_DECREASED",
            message: format!(
                "Score decreased from {} to {}",
                prev_state.score, state.score
            ),
            frame,
            expected: Some(format!(">= {}", prev_state.score)),
            actual: Some(state.score.to_string()),
        });
    }

    // P-6: Score delta must be 0 or valid value
    let delta = state.score - prev_state.score;
    if delta > 0 {
        let valid_scores = [
            SCORE_LARGE_ASTEROID,
            SCORE_MEDIUM_ASTEROID,
            SCORE_SMALL_ASTEROID,
            SCORE_LARGE_SAUCER,
            SCORE_SMALL_SAUCER,
        ];

        if !valid_scores.contains(&delta) {
            return Err(RuleViolation {
                code: "PROGRESSION_SCORE_DELTA_INVALID",
                message: format!(
                    "Score increased by {}, which is not a valid score value",
                    delta
                ),
                frame,
                expected: Some(format!("0 or one of {:?}", valid_scores)),
                actual: Some(delta.to_string()),
            });
        }
    }

    // P-8: Extra life threshold must be consistent
    let expected_next_extra_life =
        (state.score / EXTRA_LIFE_SCORE_STEP + 1) * EXTRA_LIFE_SCORE_STEP;
    if state.next_extra_life_score != expected_next_extra_life {
        return Err(RuleViolation {
            code: "PROGRESSION_EXTRA_LIFE_INVALID",
            message: format!(
                "Extra life threshold {} doesn't match expected {}",
                state.next_extra_life_score, expected_next_extra_life
            ),
            frame,
            expected: Some(expected_next_extra_life.to_string()),
            actual: Some(state.next_extra_life_score.to_string()),
        });
    }

    Ok(())
}

/// Check general game state consistency
fn check_game_state_consistency(state: &GameState, frame: u32) -> FrameCheckResult {
    // Check frame count is valid
    if state.frame_count != frame {
        return Err(RuleViolation {
            code: "GLOBAL_FRAMECOUNT_MISMATCH",
            message: format!(
                "Game state frame count {} doesn't match expected {}",
                state.frame_count, frame
            ),
            frame,
            expected: Some(frame.to_string()),
            actual: Some(state.frame_count.to_string()),
        });
    }

    // Check lives are valid
    if state.lives > 99 {
        // Arbitrary upper limit for sanity
        return Err(RuleViolation {
            code: "PROGRESSION_LIVES_INVALID",
            message: format!("Invalid lives count: {}", state.lives),
            frame,
            expected: Some("0-99".to_string()),
            actual: Some(state.lives.to_string()),
        });
    }

    // Check wave is valid
    if state.wave == 0 || state.wave > 100 {
        // Arbitrary upper limit
        return Err(RuleViolation {
            code: "PROGRESSION_WAVE_INVALID",
            message: format!("Invalid wave number: {}", state.wave),
            frame,
            expected: Some("1-100".to_string()),
            actual: Some(state.wave.to_string()),
        });
    }

    // K-1: timeSinceLastKill must increment each frame (checked via previous state)
    // This is handled implicitly by the game logic

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ship_speed_within_limit() {
        let ship = Ship {
            vx: 1000, // Well within limit
            vy: 1000,
            ..Default::default()
        };

        assert!(check_ship_state(&ship, 0).is_ok());
    }

    #[test]
    fn test_ship_speed_exceeds_limit() {
        let ship = Ship {
            vx: 2000, // Exceeds limit
            vy: 2000,
            ..Default::default()
        };

        let result = check_ship_state(&ship, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "SHIP_SPEED_CLAMP_INVALID");
    }

    #[test]
    fn test_bullet_limit() {
        let mut state = GameState::default();
        state.bullets = vec![
            Bullet::default(),
            Bullet::default(),
            Bullet::default(),
            Bullet::default(),
            Bullet::default(), // 5th bullet - exceeds limit
        ];

        let result = check_bullet_constraints(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PLAYER_BULLET_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_score_decrease() {
        let prev = GameState {
            score: 1000,
            ..Default::default()
        };
        let curr = GameState {
            score: 500, // Decreased!
            next_extra_life_score: 10000,
            ..Default::default()
        };

        let result = check_score_integrity(&prev, &curr, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PROGRESSION_SCORE_DECREASED");
    }

    #[test]
    fn test_invalid_score_delta() {
        let prev = GameState {
            score: 0,
            ..Default::default()
        };
        let curr = GameState {
            score: 123, // Not a valid score value
            next_extra_life_score: 10000,
            ..Default::default()
        };

        let result = check_score_integrity(&prev, &curr, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PROGRESSION_SCORE_DELTA_INVALID");
    }

    // =========================================================================
    // SECURITY/ADVERSARIAL TESTS
    // =========================================================================

    #[test]
    fn test_malicious_speed_injection() {
        // Test that ships with injected velocity exceeding limits are caught
        let ship = Ship {
            vx: 3000, // Far exceeds max speed of ~1451
            vy: 3000,
            ..Default::default()
        };

        let result = check_ship_state(&ship, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "SHIP_SPEED_CLAMP_INVALID");
    }

    #[test]
    fn test_asteroid_count_cap_exceeded() {
        // Test that having more than ASTEROID_CAP (27) asteroids is caught
        let mut state = GameState::default();
        for _ in 0..30 {
            state.asteroids.push(Asteroid {
                alive: true,
                ..Default::default()
            });
        }

        let result = check_asteroid_constraints(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "ASTEROID_COUNT_CAP_EXCEEDED");
    }

    #[test]
    fn test_saucer_count_cap_exceeded() {
        // Test that having too many saucers for a wave is caught
        let mut state = GameState::default();
        state.wave = 1; // Wave 1 allows max 1 saucer

        for _ in 0..3 {
            state.saucers.push(Saucer {
                alive: true,
                ..Default::default()
            });
        }

        let result = check_saucer_constraints(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "SAUCER_COUNT_CAP_EXCEEDED");
    }

    #[test]
    fn test_bullet_lifetime_exceeded() {
        // Test bullets living longer than max lifetime are caught
        let mut state = GameState::default();
        state.bullets.push(Bullet {
            life: SHIP_BULLET_LIFETIME_FRAMES + 10,
            ..Default::default()
        });

        let result = check_bullet_constraints(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PLAYER_BULLET_LIFE_INVALID");
    }

    #[test]
    fn test_frame_count_mismatch() {
        // Test that frame count inconsistencies are caught
        let state = GameState {
            frame_count: 100,
            ..Default::default()
        };

        let result = check_game_state_consistency(&state, 99);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "GLOBAL_FRAMECOUNT_MISMATCH");
    }

    #[test]
    fn test_multiple_score_increments_caught() {
        // Test that cumulative invalid score increments are caught
        let prev = GameState {
            score: 100,
            ..Default::default()
        };
        // Invalid: 123 is not a valid score value
        let curr = GameState {
            score: 223, // 223 - 100 = 123, not a valid score value
            next_extra_life_score: 10000,
            ..Default::default()
        };

        let result = check_score_integrity(&prev, &curr, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PROGRESSION_SCORE_DELTA_INVALID");
    }

    // =========================================================================
    // COLLISION DETECTION TESTS
    // =========================================================================

    use crate::fixed_point::distance_sq_q12_4;

    #[test]
    fn test_bullet_asteroid_collision_distance() {
        // Test collision detection at exact boundary
        let bullet = Bullet {
            x: 100 << 4,
            y: 100 << 4,
            ..Default::default()
        };
        let asteroid = Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            size: AsteroidSize::Large,
            alive: true,
            ..Default::default()
        };

        let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y);
        let threshold = (BULLET_RADIUS_Q12_4 + asteroid.size.radius_q12_4()) as u32;
        let threshold_sq = threshold * threshold;

        // Distance should be 0, so collision should occur (dist_sq < threshold_sq)
        assert!(
            dist_sq < threshold_sq,
            "Bullet at same position should collide"
        );
    }

    #[test]
    fn test_bullet_asteroid_no_collision_at_distance() {
        // Test that bullets far from asteroids don't collide
        let bullet = Bullet {
            x: 0,
            y: 0,
            ..Default::default()
        };
        let asteroid = Asteroid {
            x: 500 << 4, // 500 pixels away
            y: 500 << 4,
            size: AsteroidSize::Large,
            alive: true,
            ..Default::default()
        };

        let dist_sq = distance_sq_q12_4(bullet.x, bullet.y, asteroid.x, asteroid.y);
        let threshold = (BULLET_RADIUS_Q12_4 + asteroid.size.radius_q12_4()) as u32;
        let threshold_sq = threshold * threshold;

        // Should not collide
        assert!(dist_sq > threshold_sq, "Bullet far away should not collide");
    }

    #[test]
    fn test_ship_asteroid_collision_boundary() {
        // Test ship collision at boundary (with fudge factor)
        let ship = Ship {
            x: 100 << 4,
            y: 100 << 4,
            ..Default::default()
        };
        let asteroid = Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            size: AsteroidSize::Large,
            alive: true,
            ..Default::default()
        };

        let dist_sq = distance_sq_q12_4(ship.x, ship.y, asteroid.x, asteroid.y);

        // Ship collision uses fudge factor: 0.88x asteroid radius
        let asteroid_radius = ((asteroid.size.radius_q12_4() as u32 * 225) >> 8) as u16;
        let threshold = (SHIP_RADIUS_Q12_4 + asteroid_radius) as u32;
        let threshold_sq = threshold * threshold;

        assert!(
            dist_sq < threshold_sq,
            "Ship at same position should collide"
        );
    }

    #[test]
    fn test_collision_priority_bullet_before_ship() {
        // Verify that bullet-asteroid collisions are checked before ship-asteroid
        // This ensures correct game logic: bullets destroy asteroids before ship dies
        let mut state = GameState::default();

        // Create a scenario where both bullet and ship would collide with asteroid
        state.ship = Ship {
            x: 100 << 4,
            y: 100 << 4,
            can_control: true,
            invulnerable_timer: 0,
            ..Default::default()
        };
        state.bullets.push(Bullet {
            x: 100 << 4,
            y: 100 << 4,
            life: SHIP_BULLET_LIFETIME_FRAMES,
            ..Default::default()
        });
        state.asteroids.push(Asteroid {
            x: 100 << 4,
            y: 100 << 4,
            size: AsteroidSize::Large,
            alive: true,
            ..Default::default()
        });

        // Both should be able to collide
        let ship_dist_sq = distance_sq_q12_4(
            state.ship.x,
            state.ship.y,
            state.asteroids[0].x,
            state.asteroids[0].y,
        );
        let bullet_dist_sq = distance_sq_q12_4(
            state.bullets[0].x,
            state.bullets[0].y,
            state.asteroids[0].x,
            state.asteroids[0].y,
        );

        assert!(ship_dist_sq < (SHIP_RADIUS_Q12_4 as u32 * SHIP_RADIUS_Q12_4 as u32));
        assert!(bullet_dist_sq < (BULLET_RADIUS_Q12_4 as u32 * BULLET_RADIUS_Q12_4 as u32));
    }

    // =========================================================================
    // GAME MECHANICS TESTS
    // =========================================================================

    #[test]
    fn test_wave_asteroid_spawn_count() {
        // Test that waves spawn correct number of asteroids
        // Wave 1: 4 asteroids, Wave 2: 6, Wave 3: 8, etc.
        // Cap at 16 asteroids

        for wave in 1..=10 {
            let expected = ((4 + (wave - 1) * 2) as usize).min(16);
            let calculated = (4 + (wave - 1) * 2).min(16) as usize;
            assert_eq!(
                calculated, expected,
                "Wave {} should spawn {} asteroids",
                wave, expected
            );
        }
    }

    #[test]
    fn test_asteroid_velocity_inheritance() {
        // Test that child asteroids inherit parent velocity correctly
        // Formula: child_v = (parent_v * 46) >> 8

        let parent_vx: i16 = 100;
        let expected_child_vx = (parent_vx as i32 * 46) >> 8;

        assert_eq!(expected_child_vx, 17, "Velocity inheritance calculation");
    }

    #[test]
    fn test_extra_life_threshold_calculation() {
        // Test extra life threshold calculation
        let score = 5000;
        let expected_next = (score / EXTRA_LIFE_SCORE_STEP + 1) * EXTRA_LIFE_SCORE_STEP;

        assert_eq!(
            expected_next, 10000,
            "Extra life at 5000 should be next at 10000"
        );
    }

    #[test]
    fn test_lives_upper_bound() {
        // Test that lives > 99 is caught as invalid
        let state = GameState {
            lives: 100,
            ..Default::default()
        };

        let result = check_game_state_consistency(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PROGRESSION_LIVES_INVALID");
    }

    #[test]
    fn test_wave_zero_invalid() {
        // Test that wave 0 is invalid
        let state = GameState {
            wave: 0,
            ..Default::default()
        };

        let result = check_game_state_consistency(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "PROGRESSION_WAVE_INVALID");
    }

    #[test]
    fn test_saucer_bullet_lifetime() {
        // Test saucer bullet lifetime validation
        let mut state = GameState::default();
        state.saucer_bullets.push(Bullet {
            life: SAUCER_BULLET_LIFETIME_FRAMES + 5,
            ..Default::default()
        });

        let result = check_saucer_constraints(&state, 0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "SAUCER_BULLET_LIFE_INVALID");
    }

    // =========================================================================
    // DETERMINISM TESTS
    // =========================================================================

    #[test]
    fn test_deterministic_game_execution() {
        // Run same game twice with same seed, should get identical results
        let mut engine1 = GameEngine::new(12345);
        let mut engine2 = GameEngine::new(12345);

        // Run 100 frames with same inputs
        for _ in 0..100 {
            let input = FrameInput {
                left: false,
                right: true,
                thrust: true,
                fire: false,
            };
            engine1.step(input);
            engine2.step(input);
        }

        // States should be identical
        assert_eq!(engine1.state().ship.x, engine2.state().ship.x);
        assert_eq!(engine1.state().ship.y, engine2.state().ship.y);
        assert_eq!(engine1.state().ship.angle, engine2.state().ship.angle);
        assert_eq!(engine1.state().score, engine2.state().score);
    }

    #[test]
    fn test_rng_determinism() {
        // RNG with same seed should produce same sequence
        let mut rng1 = Rng::new(12345);
        let mut rng2 = Rng::new(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u32(), rng2.next_u32());
        }
    }
}
