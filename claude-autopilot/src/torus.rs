use asteroids_verifier_core::constants::{
    SHIP_BULLET_LIFETIME_FRAMES, SHIP_BULLET_SPEED_Q8_8, SHIP_MAX_SPEED_SQ_Q16_16,
    SHIP_THRUST_Q8_8, SHIP_TURN_SPEED_BAM, WORLD_HEIGHT_Q12_4, WORLD_WIDTH_Q12_4,
};
use asteroids_verifier_core::fixed_point::{
    apply_drag, atan2_bam, clamp_speed_q8_8, cos_bam, displace_q12_4, shortest_delta_q12_4,
    sin_bam, velocity_q8_8, wrap_x_q12_4, wrap_y_q12_4,
};
use asteroids_verifier_core::sim::{AsteroidSizeSnapshot, BulletSnapshot, WorldSnapshot};
use asteroids_verifier_core::tape::decode_input_byte;

// ── Ship prediction ─────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct PredictedShip {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub angle: i32,
    pub radius: i32,
    pub fire_cooldown: i32,
}

impl PredictedShip {
    pub fn speed_px(&self) -> f64 {
        ((self.vx as f64 / 256.0).powi(2) + (self.vy as f64 / 256.0).powi(2)).sqrt()
    }
}

pub fn predict_ship(world: &WorldSnapshot, input_byte: u8) -> PredictedShip {
    let ship = &world.ship;
    let input = decode_input_byte(input_byte);

    let mut angle = ship.angle;
    if input.left {
        angle = (angle - SHIP_TURN_SPEED_BAM) & 0xff;
    }
    if input.right {
        angle = (angle + SHIP_TURN_SPEED_BAM) & 0xff;
    }

    let mut vx = ship.vx;
    let mut vy = ship.vy;
    if input.thrust {
        vx += (cos_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
        vy += (sin_bam(angle) * SHIP_THRUST_Q8_8) >> 14;
    }
    vx = apply_drag(vx);
    vy = apply_drag(vy);
    (vx, vy) = clamp_speed_q8_8(vx, vy, SHIP_MAX_SPEED_SQ_Q16_16);

    let x = wrap_x_q12_4(ship.x + (vx >> 4));
    let y = wrap_y_q12_4(ship.y + (vy >> 4));
    let fire_cooldown = if ship.fire_cooldown > 0 {
        ship.fire_cooldown - 1
    } else {
        ship.fire_cooldown
    };

    PredictedShip {
        x,
        y,
        vx,
        vy,
        angle,
        radius: ship.radius,
        fire_cooldown,
    }
}

// ── Toroidal math ───────────────────────────────────────────────────

const TORUS_SHIFTS_X: [i32; 3] = [-WORLD_WIDTH_Q12_4, 0, WORLD_WIDTH_Q12_4];
const TORUS_SHIFTS_Y: [i32; 3] = [-WORLD_HEIGHT_Q12_4, 0, WORLD_HEIGHT_Q12_4];

#[derive(Clone, Copy)]
pub struct TorusApproach {
    pub immediate_px: f64,
    pub closest_px: f64,
    pub t_closest: f64,
    pub dot: f64,
}

pub fn torus_relative_approach(
    ref_x: i32,
    ref_y: i32,
    ref_vx: i32,
    ref_vy: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    horizon_frames: f64,
) -> TorusApproach {
    let horizon = horizon_frames.max(0.0);
    let rvx = (target_vx - ref_vx) as f64 / 256.0;
    let rvy = (target_vy - ref_vy) as f64 / 256.0;
    let rv_sq = rvx * rvx + rvy * rvy;

    let mut best = TorusApproach {
        immediate_px: f64::MAX,
        closest_px: f64::MAX,
        t_closest: 0.0,
        dot: 0.0,
    };

    for sx in TORUS_SHIFTS_X {
        for sy in TORUS_SHIFTS_Y {
            let dx = (target_x + sx - ref_x) as f64 / 16.0;
            let dy = (target_y + sy - ref_y) as f64 / 16.0;
            let immediate = (dx * dx + dy * dy).sqrt();
            let dot = dx * rvx + dy * rvy;

            let mut t = if rv_sq > 1e-6 { -dot / rv_sq } else { 0.0 };
            t = t.clamp(0.0, horizon);
            let cdx = dx + rvx * t;
            let cdy = dy + rvy * t;
            let closest = (cdx * cdx + cdy * cdy).sqrt();

            if closest < best.closest_px - 1e-6
                || ((closest - best.closest_px).abs() <= 1e-6 && immediate < best.immediate_px)
            {
                best = TorusApproach {
                    immediate_px: immediate,
                    closest_px: closest,
                    t_closest: t,
                    dot,
                };
            }
        }
    }

    best
}

#[derive(Clone, Copy)]
pub struct WrapAimSolution {
    pub distance_px: f64,
    pub aim_angle: i32,
    pub intercept_frames: f64,
}

pub fn best_wrapped_aim(
    shooter_x: i32,
    shooter_y: i32,
    shooter_vx: i32,
    shooter_vy: i32,
    shooter_angle: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    bullet_speed_px: f64,
    max_lead_frames: f64,
) -> Option<WrapAimSolution> {
    if bullet_speed_px <= 0.05 {
        return None;
    }

    let rvx = (target_vx - shooter_vx) as f64 / 256.0;
    let rvy = (target_vy - shooter_vy) as f64 / 256.0;
    let lead_cap = max_lead_frames.max(0.0);
    let mut best: Option<(WrapAimSolution, f64)> = None;

    for sx in TORUS_SHIFTS_X {
        for sy in TORUS_SHIFTS_Y {
            let base_dx = (target_x + sx - shooter_x) as f64 / 16.0;
            let base_dy = (target_y + sy - shooter_y) as f64 / 16.0;
            let base_dist = (base_dx * base_dx + base_dy * base_dy).sqrt();
            if base_dist < 0.1 {
                continue;
            }

            let a = rvx * rvx + rvy * rvy - bullet_speed_px * bullet_speed_px;
            let b = 2.0 * (base_dx * rvx + base_dy * rvy);
            let c = base_dx * base_dx + base_dy * base_dy;
            let mut candidates = [f64::NAN; 2];
            let mut count = 0usize;

            if a.abs() <= 1e-8 {
                if b.abs() > 1e-8 {
                    candidates[0] = -c / b;
                    count = 1;
                }
            } else {
                let disc = b * b - 4.0 * a * c;
                if disc >= 0.0 {
                    let sqrt_disc = disc.sqrt();
                    candidates[0] = (-b - sqrt_disc) / (2.0 * a);
                    candidates[1] = (-b + sqrt_disc) / (2.0 * a);
                    count = 2;
                }
            }

            let mut best_t: Option<f64> = None;
            for t in candidates.iter().copied().take(count) {
                if !t.is_finite() || t < 0.0 || t > lead_cap {
                    continue;
                }
                match best_t {
                    None => best_t = Some(t),
                    Some(existing) if t < existing => best_t = Some(t),
                    _ => {}
                }
            }

            let Some(t) = best_t else { continue };
            let pdx = base_dx + rvx * t;
            let pdy = base_dy + rvy * t;
            let distance_px = (pdx * pdx + pdy * pdy).sqrt();
            if distance_px < 0.1 {
                continue;
            }

            let aim_angle = atan2_bam((pdy * 16.0) as i32, (pdx * 16.0) as i32);
            let angle_error = signed_angle_delta(shooter_angle, aim_angle).abs() as f64;
            let ranking = t + angle_error * 0.015;
            let candidate = WrapAimSolution {
                distance_px,
                aim_angle,
                intercept_frames: t,
            };

            match best {
                None => best = Some((candidate, ranking)),
                Some((_, r)) if ranking < r => best = Some((candidate, ranking)),
                _ => {}
            }
        }
    }

    best.map(|(sol, _)| sol)
}

pub fn projectile_wrap_closest_approach(
    start_x: i32,
    start_y: i32,
    bullet_vx: i32,
    bullet_vy: i32,
    target_x: i32,
    target_y: i32,
    target_vx: i32,
    target_vy: i32,
    max_frames: f64,
) -> (f64, f64) {
    let horizon = max_frames.max(0.0);
    let rvx = (target_vx - bullet_vx) as f64 / 256.0;
    let rvy = (target_vy - bullet_vy) as f64 / 256.0;
    let rv_sq = rvx * rvx + rvy * rvy;

    let mut best_closest = f64::MAX;
    let mut best_t = 0.0;

    for sx in TORUS_SHIFTS_X {
        for sy in TORUS_SHIFTS_Y {
            let dx = (target_x + sx - start_x) as f64 / 16.0;
            let dy = (target_y + sy - start_y) as f64 / 16.0;
            let dot = dx * rvx + dy * rvy;
            let mut t = if rv_sq > 1e-6 { -dot / rv_sq } else { 0.0 };
            t = t.clamp(0.0, horizon);
            let cdx = dx + rvx * t;
            let cdy = dy + rvy * t;
            let closest = (cdx * cdx + cdy * cdy).sqrt();
            if closest < best_closest {
                best_closest = closest;
                best_t = t;
            }
        }
    }

    (best_closest, best_t)
}

// ── Angle helpers ───────────────────────────────────────────────────

#[inline]
pub fn signed_angle_delta(current: i32, target: i32) -> i32 {
    let mut delta = (target - current) & 0xff;
    if delta > 127 {
        delta -= 256;
    }
    delta
}

// ── Distance helpers ────────────────────────────────────────────────

pub fn torus_distance_px(x1: i32, y1: i32, x2: i32, y2: i32) -> f64 {
    let dx = shortest_delta_q12_4(x1, x2, WORLD_WIDTH_Q12_4) as f64 / 16.0;
    let dy = shortest_delta_q12_4(y1, y2, WORLD_HEIGHT_Q12_4) as f64 / 16.0;
    (dx * dx + dy * dy).sqrt()
}

pub fn nearest_threat_distance(world: &WorldSnapshot, x: i32, y: i32) -> f64 {
    let mut nearest = f64::MAX;

    for asteroid in &world.asteroids {
        if asteroid.alive {
            nearest = nearest.min(torus_distance_px(x, y, asteroid.x, asteroid.y));
        }
    }
    for saucer in &world.saucers {
        if saucer.alive {
            nearest = nearest.min(torus_distance_px(x, y, saucer.x, saucer.y));
        }
    }
    for bullet in &world.saucer_bullets {
        if bullet.alive {
            nearest = nearest.min(torus_distance_px(x, y, bullet.x, bullet.y));
        }
    }

    if nearest == f64::MAX {
        9999.0
    } else {
        nearest
    }
}

pub fn nearest_saucer_distance(world: &WorldSnapshot, pred: PredictedShip) -> f64 {
    let mut nearest = f64::MAX;
    for saucer in &world.saucers {
        if saucer.alive {
            let approach = torus_relative_approach(
                pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy,
                16.0,
            );
            nearest = nearest.min(approach.immediate_px);
        }
    }
    nearest
}

// ── Fire quality estimation ─────────────────────────────────────────

pub fn estimate_fire_quality(pred: PredictedShip, world: &WorldSnapshot) -> f64 {
    let (dx, dy) = displace_q12_4(pred.angle, pred.radius + 6);
    let start_x = wrap_x_q12_4(pred.x + dx);
    let start_y = wrap_y_q12_4(pred.y + dy);
    let ship_speed_approx = ((pred.vx.abs() + pred.vy.abs()) * 3) >> 2;
    let bullet_speed_q8_8 = SHIP_BULLET_SPEED_Q8_8 + ((ship_speed_approx * 89) >> 8);
    let (bvx, bvy) = velocity_q8_8(pred.angle, bullet_speed_q8_8);
    let bullet_vx = pred.vx + bvx;
    let bullet_vy = pred.vy + bvy;

    let mut best = 0.0;
    let max_t = SHIP_BULLET_LIFETIME_FRAMES as f64;

    let mut consider = |x: i32, y: i32, vx: i32, vy: i32, radius: i32, weight: f64| {
        if weight <= 0.0 {
            return;
        }
        let (closest, t) = projectile_wrap_closest_approach(
            start_x, start_y, bullet_vx, bullet_vy, x, y, vx, vy, max_t,
        );
        let safe = (radius + 2) as f64;
        let hit_score = (safe / (closest + 1.0)).powf(1.75);
        let time_factor = 1.0 - (t / max_t) * 0.42;
        let candidate = weight * hit_score * time_factor;
        if candidate > best {
            best = candidate;
        }
    };

    for asteroid in &world.asteroids {
        if !asteroid.alive {
            continue;
        }
        consider(
            asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, asteroid.radius,
            asteroid_target_weight(asteroid.size),
        );
    }

    for saucer in &world.saucers {
        if !saucer.alive {
            continue;
        }
        let mut w = if saucer.small { 2.6 } else { 1.75 };
        let approach = torus_relative_approach(
            pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 24.0,
        );
        if approach.closest_px < 220.0 {
            let urgency = ((220.0 - approach.closest_px) / 220.0).clamp(0.0, 1.0);
            w *= 1.0 + urgency * 0.72;
        }
        if saucer.small {
            w *= 1.12;
        }
        consider(saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius, w);
    }

    best
}

pub fn asteroid_target_weight(size: AsteroidSizeSnapshot) -> f64 {
    match size {
        AsteroidSizeSnapshot::Large => 0.96,
        AsteroidSizeSnapshot::Medium => 1.22,
        AsteroidSizeSnapshot::Small => 1.44,
    }
}

// ── Targeting ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct TargetInfo {
    pub distance_px: f64,
    pub aim_angle: i32,
    pub value: f64,
    pub intercept_frames: f64,
    pub target_x: i32,
    pub target_y: i32,
    pub target_vx: i32,
    pub target_vy: i32,
    pub target_radius: i32,
    pub is_saucer: bool,
}

pub fn best_target(world: &WorldSnapshot, pred: PredictedShip) -> Option<TargetInfo> {
    let bullet_speed = 8.6 + pred.speed_px() * 0.33;
    let mut best: Option<TargetInfo> = None;

    let mut consider =
        |x: i32, y: i32, vx: i32, vy: i32, radius: i32, weight: f64, is_saucer: bool| {
            if weight <= 0.0 {
                return;
            }
            let Some(intercept) = best_wrapped_aim(
                pred.x, pred.y, pred.vx, pred.vy, pred.angle, x, y, vx, vy, bullet_speed, 64.0,
            ) else {
                return;
            };
            let angle_error = signed_angle_delta(pred.angle, intercept.aim_angle).abs() as f64;
            let mut value = weight / (intercept.distance_px + 16.0);
            value += (1.0 - (angle_error / 128.0)).max(0.0) * 0.6;
            value *= 1.0 + (1.0 - (intercept.intercept_frames / 64.0).clamp(0.0, 1.0)) * 0.1;

            let candidate = TargetInfo {
                distance_px: intercept.distance_px,
                aim_angle: intercept.aim_angle,
                value,
                intercept_frames: intercept.intercept_frames,
                target_x: x,
                target_y: y,
                target_vx: vx,
                target_vy: vy,
                target_radius: radius,
                is_saucer,
            };
            match best {
                None => best = Some(candidate),
                Some(existing) if candidate.value > existing.value => best = Some(candidate),
                _ => {}
            }
        };

    for asteroid in &world.asteroids {
        if !asteroid.alive {
            continue;
        }
        consider(
            asteroid.x, asteroid.y, asteroid.vx, asteroid.vy, asteroid.radius,
            asteroid_target_weight(asteroid.size),
            false,
        );
    }

    for saucer in &world.saucers {
        if !saucer.alive {
            continue;
        }
        let mut w = if saucer.small { 2.6 } else { 1.75 };
        let approach = torus_relative_approach(
            pred.x, pred.y, pred.vx, pred.vy, saucer.x, saucer.y, saucer.vx, saucer.vy, 22.0,
        );
        if approach.closest_px < 200.0 {
            let urgency = ((200.0 - approach.closest_px) / 200.0).clamp(0.0, 1.0);
            w *= 1.0 + urgency * 0.72;
        }
        consider(saucer.x, saucer.y, saucer.vx, saucer.vy, saucer.radius, w, true);
    }

    best
}

// ── Bullet tracking (duplicate detection) ───────────────────────────

pub fn bullet_tracks_target(
    bullet: &BulletSnapshot,
    tx: i32, ty: i32, tvx: i32, tvy: i32, tradius: i32,
) -> bool {
    if !bullet.alive || bullet.life <= 0 {
        return false;
    }
    let horizon = (bullet.life as f64).min(32.0).max(1.0);
    let (closest, t) = projectile_wrap_closest_approach(
        bullet.x, bullet.y, bullet.vx, bullet.vy, tx, ty, tvx, tvy, horizon,
    );
    let hit_radius = (bullet.radius + tradius) as f64;
    closest <= hit_radius * 1.02 && t <= horizon * 0.9
}

pub fn target_already_covered(
    bullets: &[BulletSnapshot],
    tx: i32, ty: i32, tvx: i32, tvy: i32, tradius: i32,
) -> bool {
    bullets
        .iter()
        .any(|b| bullet_tracks_target(b, tx, ty, tvx, tvy, tradius))
}

pub fn own_bullet_stats(bullets: &[BulletSnapshot]) -> (usize, i32) {
    let mut active = 0usize;
    let mut shortest = i32::MAX;
    for b in bullets {
        if b.alive {
            active += 1;
            shortest = shortest.min(b.life);
        }
    }
    if shortest == i32::MAX {
        shortest = 0;
    }
    (active, shortest)
}

// ── Fire discipline gate ───────────────────────────────────────────

pub fn disciplined_fire_ok(
    active_bullets: usize,
    shortest_life: i32,
    fire_quality: f64,
    min_quality: f64,
    nearest_saucer_px: f64,
    nearest_threat_px: f64,
    is_duplicate: bool,
) -> bool {
    let strict_quality = (min_quality + 0.1).clamp(0.18, 0.9);
    if active_bullets == 0 {
        return fire_quality >= strict_quality;
    }
    if !is_duplicate {
        let rapid_window = nearest_threat_px < 118.0 || nearest_saucer_px < 136.0;
        let switch_quality = (strict_quality + 0.2).clamp(0.24, 0.95);
        if rapid_window && fire_quality >= switch_quality {
            return true;
        }
    }
    let emergency = nearest_threat_px < 78.0 || nearest_saucer_px < 88.0;
    let life_gate = if emergency { 3 } else { 2 };
    if shortest_life > life_gate {
        return false;
    }
    let stacked = (strict_quality + if emergency { 0.08 } else { 0.18 }).clamp(0.24, 0.94);
    fire_quality >= stacked
}
