use super::*;
use crate::tape::parse_tape;
use std::fs;

fn assert_invariant_violation(mutator: impl FnOnce(&mut Game), expected: RuleCode) {
    let mut game = Game::new(0xDEAD_BEEF);
    mutator(&mut game);
    assert_eq!(game.validate_invariants(), Err(expected));
}

fn assert_transition_violation_at_frame(
    inputs: &[u8],
    frame_to_mutate: u32,
    mutate: impl FnOnce(&mut TransitionState),
    expected: RuleCode,
) {
    let mut game = Game::new(0xDEAD_BEEF);
    game.validate_invariants()
        .expect("initial state must be valid");

    let mut mutate = Some(mutate);
    for input in inputs {
        let before_step = game.transition_state();
        game.step(*input);
        let mut after_step = game.transition_state();

        if after_step.frame_count == frame_to_mutate {
            if let Some(mutate_once) = mutate.take() {
                mutate_once(&mut after_step);
            }
        }

        if let Err(rule) = validate_transition(&before_step, &after_step, *input) {
            assert_eq!(after_step.frame_count, frame_to_mutate);
            assert_eq!(rule, expected);
            return;
        }

        game.validate_invariants()
            .expect("post-step state must satisfy invariants");
    }

    panic!("expected transition violation at frame {frame_to_mutate}");
}

fn valid_bullet() -> Bullet {
    Bullet {
        x: 100,
        y: 100,
        vx: 0,
        vy: 0,
        alive: true,
        radius: 2,
        life: 1,
    }
}

fn valid_saucer() -> Saucer {
    Saucer {
        x: 1_000,
        y: 1_000,
        vx: 0,
        vy: 0,
        alive: true,
        radius: SAUCER_RADIUS_LARGE,
        small: false,
        fire_cooldown: 0,
        drift_timer: 0,
    }
}

fn brute_force_legal_score_delta(delta: u32) -> bool {
    if delta > MAX_SCORE_DELTA_PER_FRAME {
        return false;
    }
    if delta == 0 {
        return true;
    }

    for a in SCORE_EVENT_VALUES {
        if a == delta {
            return true;
        }
        for b in SCORE_EVENT_VALUES {
            let two = a + b;
            if two == delta {
                return true;
            }
            for c in SCORE_EVENT_VALUES {
                let three = two + c;
                if three == delta {
                    return true;
                }
                for d in SCORE_EVENT_VALUES {
                    if three + d == delta {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[test]
fn legal_score_delta_lookup_matches_bruteforce() {
    for delta in 0..=(MAX_SCORE_DELTA_PER_FRAME + 500) {
        assert_eq!(
            is_legal_score_delta(delta),
            brute_force_legal_score_delta(delta),
            "delta {} mismatch",
            delta
        );
    }
}

#[test]
fn same_seed_and_inputs_are_deterministic() {
    let inputs = [0x00u8, 0x01, 0x04, 0x0C, 0x00, 0x08, 0x02, 0x00];
    let a = replay(0x1234_5678, &inputs);
    let b = replay(0x1234_5678, &inputs);
    assert_eq!(a, b);
}

#[test]
fn strict_replay_matches_regular_replay_on_random_inputs() {
    let mut rng = SeededRng::new(0xC0FF_EE00);

    for _ in 0..64 {
        let seed = rng.next();
        let len = (rng.next() % 128 + 1) as usize;
        let mut inputs = vec![0u8; len];
        for input in &mut inputs {
            *input = (rng.next() & 0x0F) as u8;
        }

        let regular = replay(seed, &inputs);
        let strict = replay_strict(seed, &inputs).expect("strict replay should succeed");
        assert_eq!(regular, strict);
    }
}

#[test]
fn live_game_result_matches_replay_for_same_inputs() {
    let seed = 0xA11C_E123;
    let inputs = [0x00u8, 0x08, 0x08, 0x01, 0x04, 0x02, 0x00, 0x0C, 0x00, 0x03];
    let expected = replay(seed, &inputs);

    let mut live = LiveGame::new(seed);
    for input in inputs {
        live.step(input);
    }

    assert_eq!(live.result(), expected);
    live.validate().expect("live game must remain valid");
}

#[test]
fn live_game_snapshot_counts_match_initial_checkpoint() {
    let seed = 0xDEAD_BEEF;
    let snapshot = LiveGame::new(seed).snapshot();
    let checkpoints = replay_with_checkpoints(seed, &[], 1);
    let initial = checkpoints.first().expect("initial checkpoint exists");

    assert_eq!(snapshot.frame_count, initial.frame_count);
    assert_eq!(snapshot.score, initial.score);
    assert_eq!(snapshot.lives, initial.lives);
    assert_eq!(snapshot.wave, initial.wave);
    assert_eq!(snapshot.rng_state, initial.rng_state);
    assert_eq!(snapshot.asteroids.len(), initial.asteroids);
    assert_eq!(snapshot.bullets.len(), initial.bullets);
    assert_eq!(snapshot.saucers.len(), initial.saucers);
    assert_eq!(snapshot.saucer_bullets.len(), initial.saucer_bullets);
    assert_eq!(snapshot.ship.x, initial.ship_x);
    assert_eq!(snapshot.ship.y, initial.ship_y);
    assert_eq!(snapshot.ship.vx, initial.ship_vx);
    assert_eq!(snapshot.ship.vy, initial.ship_vy);
    assert_eq!(snapshot.ship.angle, initial.ship_angle);
}

#[test]
fn checked_step_accepts_verified_fixture_inputs() {
    let bytes = fs::read("../../test-fixtures/test-medium.tape")
        .expect("fixture should load for checked-step test");
    let tape = parse_tape(&bytes, 18_000).expect("fixture should parse for checked-step test");

    let mut live = LiveGame::new(tape.header.seed);
    for input in tape.inputs {
        live.step_checked(*input)
            .expect("fixture transitions should pass checked-step");
    }

    let strict =
        replay_strict(tape.header.seed, tape.inputs).expect("fixture should pass strict replay");
    assert_eq!(live.result(), strict);
}

#[test]
fn invariant_checks_report_expected_rule_codes() {
    assert_invariant_violation(|game| game.wave = 0, RuleCode::GlobalWaveNonZero);
    assert_invariant_violation(
        |game| {
            game.mode = GameMode::GameOver;
            game.lives = 1;
        },
        RuleCode::GlobalModeLivesConsistency,
    );
    assert_invariant_violation(
        |game| game.next_extra_life_score = game.score,
        RuleCode::GlobalNextExtraLifeScore,
    );
    assert_invariant_violation(|game| game.ship.x = -1, RuleCode::ShipBounds);
    assert_invariant_violation(|game| game.ship.angle = 256, RuleCode::ShipAngleRange);
    assert_invariant_violation(
        |game| game.ship.fire_cooldown = -1,
        RuleCode::ShipCooldownRange,
    );
    assert_invariant_violation(
        |game| game.ship.respawn_timer = -1,
        RuleCode::ShipRespawnTimerRange,
    );
    assert_invariant_violation(
        |game| game.ship.invulnerable_timer = -1,
        RuleCode::ShipInvulnerabilityRange,
    );

    assert_invariant_violation(
        |game| {
            game.bullets.clear();
            for _ in 0..(SHIP_BULLET_LIMIT + 1) {
                game.bullets.push(valid_bullet());
            }
        },
        RuleCode::PlayerBulletLimit,
    );
    assert_invariant_violation(
        |game| {
            game.bullets.clear();
            let mut bullet = valid_bullet();
            bullet.life = 0;
            game.bullets.push(bullet);
        },
        RuleCode::PlayerBulletState,
    );
    assert_invariant_violation(
        |game| {
            game.saucer_bullets.clear();
            let mut bullet = valid_bullet();
            bullet.x = -1;
            game.saucer_bullets.push(bullet);
        },
        RuleCode::SaucerBulletState,
    );
    assert_invariant_violation(
        |game| game.asteroids[0].angle = 256,
        RuleCode::AsteroidState,
    );
    assert_invariant_violation(
        |game| {
            game.wave = 1;
            game.saucers.clear();
            game.saucers.push(valid_saucer());
            game.saucers.push(valid_saucer());
        },
        RuleCode::SaucerCap,
    );
    assert_invariant_violation(
        |game| {
            game.wave = 7;
            game.saucers.clear();
            let mut saucer = valid_saucer();
            saucer.fire_cooldown = -1;
            game.saucers.push(saucer);
        },
        RuleCode::SaucerState,
    );
}

#[test]
fn strict_replay_detects_forced_ship_teleport() {
    assert_transition_violation_at_frame(
        &[0x00],
        1,
        |checkpoint| {
            checkpoint.ship_x = wrap_x_q12_4(checkpoint.ship_x + 512);
        },
        RuleCode::ShipPositionStep,
    );
}

#[test]
fn strict_replay_detects_forced_turn_rate_jump() {
    assert_transition_violation_at_frame(
        &[0x00],
        1,
        |checkpoint| {
            checkpoint.ship_angle = (checkpoint.ship_angle + 17) & 0xff;
        },
        RuleCode::ShipTurnRateStep,
    );
}

#[test]
fn strict_replay_detects_forced_speed_clamp_bypass() {
    assert_transition_violation_at_frame(
        &[0x00],
        1,
        |checkpoint| {
            checkpoint.ship_vx = 5_000;
        },
        RuleCode::ShipSpeedClamp,
    );
}

#[test]
fn strict_replay_detects_forced_cooldown_bypass() {
    assert_transition_violation_at_frame(
        &[0x08, 0x08],
        2,
        |checkpoint| {
            checkpoint.ship_fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES;
        },
        RuleCode::PlayerBulletCooldownBypass,
    );
}

#[test]
fn strict_replay_detects_forced_illegal_score_increment() {
    assert_transition_violation_at_frame(
        &[0x00],
        1,
        |checkpoint| {
            checkpoint.score += 30;
        },
        RuleCode::ProgressionScoreDelta,
    );
}

#[test]
fn strict_replay_detects_forced_illegal_wave_advance() {
    assert_transition_violation_at_frame(
        &[0x00],
        1,
        |checkpoint| {
            checkpoint.wave += 1;
        },
        RuleCode::ProgressionWaveAdvance,
    );
}

#[test]
fn strict_transition_validator_matches_downloads_fixture() {
    let bytes =
        fs::read("../../test-fixtures/test-real-game.tape").expect("downloads fixture should load");
    let tape = parse_tape(&bytes, 18_000).expect("downloads fixture should parse");

    let mut game = Game::new(tape.header.seed);
    game.validate_invariants()
        .expect("initial state must be valid");

    for (idx, input) in tape.inputs.iter().enumerate() {
        let before_step = game.transition_state();
        game.step(*input);
        let after_step = game.transition_state();
        if let Err(rule) = validate_transition(&before_step, &after_step, *input) {
            panic!(
                "transition violation at frame {} rule {:?} input=0x{:02x} before={:?} after={:?}",
                idx + 1,
                rule,
                input,
                before_step,
                after_step
            );
        }

        game.validate_invariants()
            .expect("post-step state must satisfy invariants");
    }
}

#[test]
fn wave_does_not_advance_with_saucers_alive() {
    let mut game = Game::new(0xDEAD_BEEF);
    let initial_wave = game.wave;

    game.asteroids.clear();
    game.saucers.clear();
    game.saucers.push(valid_saucer());

    game.step(0x00);
    assert_eq!(game.wave, initial_wave);
}

#[test]
fn no_scoring_after_game_over() {
    let mut game = Game::new(0xDEAD_BEEF);
    game.asteroids.clear();
    game.saucers.clear();
    game.saucer_bullets.clear();
    game.bullets.clear();
    game.score = 0;
    game.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;

    game.mode = GameMode::GameOver;
    game.ship.can_control = false;
    game.ship.respawn_timer = 99_999;
    game.lives = 0;

    game.asteroids.push(Asteroid {
        x: 1_000,
        y: 1_000,
        vx: 0,
        vy: 0,
        angle: 0,
        alive: true,
        radius: ASTEROID_RADIUS_LARGE,
        size: AsteroidSize::Large,
        spin: 0,
    });

    for _ in 0..20 {
        game.step(0x08);
    }

    assert_eq!(game.score, 0);
    assert!(game.bullets.is_empty());
}

#[test]
fn game_over_prevents_wave_spawn() {
    let mut game = Game::new(0xDEAD_BEEF);
    let initial_wave = game.wave;

    game.mode = GameMode::GameOver;
    game.ship.can_control = false;
    game.ship.respawn_timer = 99_999;
    game.lives = 0;
    game.asteroids.clear();
    game.saucers.clear();

    for _ in 0..20 {
        game.step(0x00);
    }

    assert!(game.asteroids.is_empty());
    assert_eq!(game.wave, initial_wave);
}

#[test]
fn wave_large_asteroid_ramp_matches_locked_table() {
    let expected = [4usize, 6, 8, 10, 11, 12, 13, 14, 15, 16, 16, 16];
    for (index, count) in expected.iter().enumerate() {
        let wave = (index + 1) as i32;
        assert_eq!(wave_asteroid_count(wave), *count, "wave={wave}");
    }
}

#[test]
fn anti_autofire_requires_release_between_shots() {
    let mut game = Game::new(0xDEAD_BEEF);
    game.mode = GameMode::GameOver;
    game.lives = 0;
    game.asteroids.clear();
    game.saucers.clear();
    game.saucer_bullets.clear();
    game.ship.fire_cooldown = 0;

    for _ in 0..40 {
        game.step(0x08);
    }
    assert_eq!(
        game.bullets.len(),
        1,
        "hold-fire should only create one bullet"
    );

    game.step(0x00);
    game.step(0x08);
    assert_eq!(
        game.bullets.len(),
        2,
        "release+press should allow the next shot"
    );
}

#[test]
fn respawn_uses_open_area_not_center_wait() {
    let mut game = Game::new(0xDEAD_BEEF);
    let (center_x, center_y) = game.get_ship_spawn_point();

    game.asteroids.clear();
    game.saucers.clear();
    game.saucer_bullets.clear();
    game.bullets.clear();
    game.asteroids.push(Asteroid {
        x: center_x,
        y: center_y,
        vx: 0,
        vy: 0,
        angle: 0,
        alive: true,
        radius: ASTEROID_RADIUS_LARGE,
        size: AsteroidSize::Large,
        spin: 0,
    });

    game.queue_ship_respawn(0);
    game.update_ship(false, false, false, false);

    assert!(game.ship.can_control);
    assert_eq!(game.ship.invulnerable_timer, SHIP_SPAWN_INVULNERABLE_FRAMES);
    assert_ne!((game.ship.x, game.ship.y), (center_x, center_y));
    assert!((SHIP_RESPAWN_EDGE_PADDING_Q12_4
        ..=WORLD_WIDTH_Q12_4 - SHIP_RESPAWN_EDGE_PADDING_Q12_4)
        .contains(&game.ship.x));
    assert!((SHIP_RESPAWN_EDGE_PADDING_Q12_4
        ..=WORLD_HEIGHT_Q12_4 - SHIP_RESPAWN_EDGE_PADDING_Q12_4)
        .contains(&game.ship.y));
}

#[test]
fn spawn_point_solver_is_deterministic_for_same_state() {
    let mut a = Game::new(0xBEEF_FACE);
    let mut b = a.clone();

    a.queue_ship_respawn(0);
    b.queue_ship_respawn(0);
    a.spawn_ship_at_best_open_point();
    b.spawn_ship_at_best_open_point();

    assert_eq!(a.ship.x, b.ship.x);
    assert_eq!(a.ship.y, b.ship.y);
}

#[test]
fn saucer_asteroid_collision_destroys_saucer() {
    let mut game = Game::new(0xDEAD_BEEF);
    game.asteroids.clear();
    game.saucers.clear();
    game.saucer_bullets.clear();

    game.asteroids.push(Asteroid {
        x: 4_000,
        y: 4_000,
        vx: 0,
        vy: 0,
        angle: 0,
        alive: true,
        radius: ASTEROID_RADIUS_LARGE,
        size: AsteroidSize::Large,
        spin: 0,
    });
    game.saucers.push(Saucer {
        x: 4_000,
        y: 4_000,
        vx: 0,
        vy: 0,
        alive: true,
        radius: SAUCER_RADIUS_LARGE,
        small: false,
        fire_cooldown: 0,
        drift_timer: 0,
    });

    game.handle_collisions();
    assert!(!game.saucers[0].alive);
}

#[test]
fn saucer_accuracy_tightens_with_pressure() {
    let low = small_saucer_aim_error_bam(1, 0);
    let mid = small_saucer_aim_error_bam(8, LURK_TIME_THRESHOLD_FRAMES);
    let high = small_saucer_aim_error_bam(20, LURK_TIME_THRESHOLD_FRAMES * 3);
    assert!(low >= mid);
    assert!(mid >= high);
    assert!(high >= 3);
}

#[test]
fn saucer_fire_cadence_accelerates_with_pressure() {
    let (small_low_min, small_low_max) = saucer_fire_cooldown_range(true, 1, 0);
    let (small_high_min, small_high_max) =
        saucer_fire_cooldown_range(true, 20, LURK_TIME_THRESHOLD_FRAMES * 3);
    assert!(small_high_min <= small_low_min);
    assert!(small_high_max <= small_low_max);

    let (large_low_min, large_low_max) = saucer_fire_cooldown_range(false, 1, 0);
    let (large_high_min, large_high_max) =
        saucer_fire_cooldown_range(false, 20, LURK_TIME_THRESHOLD_FRAMES * 3);
    assert!(large_high_min <= large_low_min);
    assert!(large_high_max <= large_low_max);
}

#[test]
fn asteroid_cap_is_never_exceeded_after_split() {
    let mut game = Game::new(0xDEAD_BEEF);
    game.asteroids.clear();
    game.saucers.clear();
    game.saucer_bullets.clear();
    game.bullets.clear();
    game.score = 0;
    game.next_extra_life_score = EXTRA_LIFE_SCORE_STEP;

    for i in 0..ASTEROID_CAP {
        game.asteroids.push(Asteroid {
            x: ((i as i32 * 480) % WORLD_WIDTH_Q12_4),
            y: 800,
            vx: 0,
            vy: 0,
            angle: 0,
            alive: true,
            radius: ASTEROID_RADIUS_SMALL,
            size: AsteroidSize::Small,
            spin: 0,
        });
    }

    let ax = 3_200;
    let ay = 3_200;
    game.asteroids.push(Asteroid {
        x: ax,
        y: ay,
        vx: 0,
        vy: 0,
        angle: 0,
        alive: true,
        radius: ASTEROID_RADIUS_LARGE,
        size: AsteroidSize::Large,
        spin: 0,
    });
    game.bullets.push(Bullet {
        x: ax,
        y: ay,
        vx: 0,
        vy: 0,
        alive: true,
        radius: 2,
        life: SHIP_BULLET_LIFETIME_FRAMES,
    });

    game.handle_collisions();
    game.prune_destroyed_entities();

    let alive_count = game.asteroids.iter().filter(|entry| entry.alive).count();
    let medium_count = game
        .asteroids
        .iter()
        .filter(|entry| entry.alive && matches!(entry.size, AsteroidSize::Medium))
        .count();

    assert_eq!(medium_count, 0);
    assert_eq!(alive_count, ASTEROID_CAP);
}
