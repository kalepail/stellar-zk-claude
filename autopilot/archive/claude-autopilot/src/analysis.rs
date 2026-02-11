use anyhow::{anyhow, Result};
use asteroids_verifier_core::constants::{
    SCORE_LARGE_ASTEROID, SCORE_LARGE_SAUCER, SCORE_MEDIUM_ASTEROID, SCORE_SMALL_ASTEROID,
    SCORE_SMALL_SAUCER, SHIP_BULLET_LIFETIME_FRAMES, SHIP_BULLET_LIMIT, WORLD_HEIGHT_Q12_4,
    WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::shortest_delta_q12_4;
use asteroids_verifier_core::sim::{LiveGame, WorldSnapshot};
use asteroids_verifier_core::tape::decode_input_byte;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::runner::RunMetrics;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    Asteroid,
    Saucer,
    SaucerBullet,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShotOutcome {
    Hit,
    Miss,
    Unresolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeathEvent {
    pub frame: u32,
    pub lives_after: i32,
    pub wave: i32,
    pub score: u32,
    pub cause: DeathCause,
    pub cause_distance_px: f64,
    pub nearest_threat_px: f64,
    pub min_edge_distance_px: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotSummary {
    pub total_fired: u32,
    pub total_hit: u32,
    pub total_miss: u32,
    pub total_unresolved: u32,
    pub hit_rate: f64,
    pub miss_rate: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunAnalysis {
    pub metrics: RunMetrics,
    pub deaths: Vec<DeathEvent>,
    pub shot_summary: ShotSummary,
}

struct ShotRecord {
    fired_frame: u32,
    resolved_frame: Option<u32>,
    outcome: ShotOutcome,
}

struct ThreatSample {
    cause: DeathCause,
    distance_px: f64,
}

pub fn analyze_run(
    metrics: RunMetrics,
    inputs: &[u8],
    seed: u32,
    max_frames: u32,
) -> Result<RunAnalysis> {
    let mut game = LiveGame::new(seed);
    game.validate()
        .map_err(|rule| anyhow!("initial invariant failure for analysis: {rule:?}"))?;

    let mut snapshot = game.snapshot();
    let mut shots = Vec::<ShotRecord>::new();
    let mut unresolved_shots = VecDeque::<usize>::new();
    let mut deaths = Vec::<DeathEvent>::new();

    for (idx, input_byte) in inputs.iter().copied().enumerate() {
        let before = snapshot.clone();
        let frame_number = before.frame_count + 1;

        let nearest_before = nearest_threat_distance_px(&before);

        let decoded = decode_input_byte(input_byte);
        let fired_now = decoded.fire
            && before.ship.can_control
            && !before.is_game_over
            && before.ship.fire_cooldown <= 0
            && before.bullets.len() < SHIP_BULLET_LIMIT;

        if fired_now {
            shots.push(ShotRecord {
                fired_frame: frame_number,
                resolved_frame: None,
                outcome: ShotOutcome::Unresolved,
            });
            unresolved_shots.push_back(shots.len() - 1);
        }

        game.step(input_byte);
        snapshot = game.snapshot();

        let score_delta = snapshot.score.saturating_sub(before.score);
        let mut inferred_hits = estimate_hit_events(score_delta);
        while inferred_hits > 0 {
            if let Some(shot_index) = unresolved_shots.pop_front() {
                let shot = &mut shots[shot_index];
                shot.resolved_frame = Some(frame_number);
                shot.outcome = ShotOutcome::Hit;
            }
            inferred_hits -= 1;
        }

        while unresolved_shots.len() > snapshot.bullets.len() {
            let Some(shot_index) = unresolved_shots.pop_front() else {
                break;
            };
            let shot = &mut shots[shot_index];
            shot.resolved_frame = Some(frame_number);
            shot.outcome = ShotOutcome::Miss;
        }

        if snapshot.lives < before.lives {
            let killer = probable_killer(&before);
            deaths.push(DeathEvent {
                frame: frame_number,
                lives_after: snapshot.lives,
                wave: snapshot.wave,
                score: snapshot.score,
                cause: killer.cause,
                cause_distance_px: killer.distance_px,
                nearest_threat_px: nearest_before,
                min_edge_distance_px: min_edge_distance_px(&before),
            });
        }

        while let Some(front) = unresolved_shots.front().copied() {
            let age = frame_number.saturating_sub(shots[front].fired_frame);
            if age <= SHIP_BULLET_LIFETIME_FRAMES as u32 + 2 {
                break;
            }
            unresolved_shots.pop_front();
            let shot = &mut shots[front];
            shot.resolved_frame = Some(frame_number);
            shot.outcome = ShotOutcome::Miss;
        }

        if idx + 1 >= max_frames as usize {
            break;
        }
    }

    for shot_index in unresolved_shots {
        shots[shot_index].outcome = ShotOutcome::Unresolved;
    }

    let mut total_hit = 0u32;
    let mut total_miss = 0u32;
    let mut total_unresolved = 0u32;
    for shot in &shots {
        match shot.outcome {
            ShotOutcome::Hit => total_hit += 1,
            ShotOutcome::Miss => total_miss += 1,
            ShotOutcome::Unresolved => total_unresolved += 1,
        }
    }
    let total_fired = shots.len() as u32;
    let hit_rate = if total_fired == 0 {
        0.0
    } else {
        total_hit as f64 / total_fired as f64
    };
    let miss_rate = if total_fired == 0 {
        0.0
    } else {
        total_miss as f64 / total_fired as f64
    };

    Ok(RunAnalysis {
        metrics,
        deaths,
        shot_summary: ShotSummary {
            total_fired,
            total_hit,
            total_miss,
            total_unresolved,
            hit_rate,
            miss_rate,
        },
    })
}

fn probable_killer(world: &WorldSnapshot) -> ThreatSample {
    let ship_x = world.ship.x;
    let ship_y = world.ship.y;
    let ship_radius = world.ship.radius as f64 / 16.0;

    let mut best = ThreatSample {
        cause: DeathCause::Unknown,
        distance_px: f64::MAX,
    };

    let mut consider = |x: i32, y: i32, radius_q12_4: i32, cause: DeathCause| {
        let dx = shortest_delta_q12_4(ship_x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let dy = shortest_delta_q12_4(ship_y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        let dist = (dx * dx + dy * dy).sqrt();
        let threshold = ship_radius + radius_q12_4 as f64 / 16.0;
        let gap = dist - threshold;
        if gap < best.distance_px {
            best = ThreatSample {
                cause,
                distance_px: gap,
            };
        }
    };

    for asteroid in &world.asteroids {
        consider(
            asteroid.x,
            asteroid.y,
            asteroid.radius,
            DeathCause::Asteroid,
        );
    }
    for saucer in &world.saucers {
        consider(saucer.x, saucer.y, saucer.radius, DeathCause::Saucer);
    }
    for bullet in &world.saucer_bullets {
        consider(bullet.x, bullet.y, bullet.radius, DeathCause::SaucerBullet);
    }

    if !best.distance_px.is_finite() {
        ThreatSample {
            cause: DeathCause::Unknown,
            distance_px: 9_999.0,
        }
    } else {
        best
    }
}

fn nearest_threat_distance_px(world: &WorldSnapshot) -> f64 {
    let ship_x = world.ship.x;
    let ship_y = world.ship.y;
    let mut best = f64::MAX;

    let mut consider = |x: i32, y: i32| {
        let dx = shortest_delta_q12_4(ship_x, x, WORLD_WIDTH_Q12_4) as f64 / 16.0;
        let dy = shortest_delta_q12_4(ship_y, y, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
        best = best.min((dx * dx + dy * dy).sqrt());
    };

    for asteroid in &world.asteroids {
        consider(asteroid.x, asteroid.y);
    }
    for saucer in &world.saucers {
        consider(saucer.x, saucer.y);
    }
    for bullet in &world.saucer_bullets {
        consider(bullet.x, bullet.y);
    }

    if best == f64::MAX {
        9_999.0
    } else {
        best
    }
}

fn min_edge_distance_px(world: &WorldSnapshot) -> f64 {
    let left = world.ship.x as f64 / 16.0;
    let right = (WORLD_WIDTH_Q12_4 - world.ship.x) as f64 / 16.0;
    let top = world.ship.y as f64 / 16.0;
    let bottom = (WORLD_HEIGHT_Q12_4 - world.ship.y) as f64 / 16.0;
    left.min(right).min(top).min(bottom)
}

fn estimate_hit_events(score_delta: u32) -> u32 {
    if score_delta == 0 {
        return 0;
    }

    const EVENTS: [u32; 5] = [
        SCORE_LARGE_ASTEROID,
        SCORE_MEDIUM_ASTEROID,
        SCORE_SMALL_ASTEROID,
        SCORE_LARGE_SAUCER,
        SCORE_SMALL_SAUCER,
    ];
    for count in 1..=4u32 {
        if can_make_score_delta(score_delta, count as usize, &EVENTS) {
            return count;
        }
    }

    1
}

fn can_make_score_delta(delta: u32, depth: usize, events: &[u32]) -> bool {
    if depth == 0 {
        return delta == 0;
    }

    for event in events {
        if *event > delta {
            continue;
        }
        if can_make_score_delta(delta - *event, depth - 1, events) {
            return true;
        }
    }

    false
}
