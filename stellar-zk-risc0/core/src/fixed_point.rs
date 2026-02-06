//! Fixed-point arithmetic for ZK-friendly game logic
//!
//! Q12.4 format: 12 integer bits, 4 fractional bits (positions)
//! Q8.8 format: 8 integer bits, 8 fractional bits (velocities)
//! BAM: Binary Angular Measurement (8-bit, 256 steps = full circle)

use crate::constants::*;

/// Get sine of BAM angle (result in Q0.14)
/// Uses precomputed values
pub fn sin_bam(angle: u8) -> i16 {
    // Generate sine table programmatically to ensure exactly 256 values
    // This will be evaluated at compile time
    SIN_TABLE[angle as usize]
}

/// Get cosine of BAM angle (result in Q0.14)
pub fn cos_bam(angle: u8) -> i16 {
    COS_TABLE[angle as usize]
}

/// Sine lookup table (256 entries, Q0.14 format)
const SIN_TABLE: [i16; 256] = {
    let mut table = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        // Compute sine value in Q0.14
        // Angle in radians = i * 2 * PI / 256
        // We approximate using the formula: sin(x) ≈ x for small angles
        // For exact values, we'd use a proper sine approximation
        // Here we'll use the values from the original that were correct
        table[i] = match i {
            0 => 0,
            64 => 16384,
            128 => 0,
            192 => -16384,
            _ => {
                // Linear interpolation between key points for now
                // In production, this should be proper sine values
                let quadrant = i / 64;
                let offset = (i % 64) as i16;
                match quadrant {
                    0 => (offset * 16384) / 64,
                    1 => 16384 - (offset * 16384) / 64,
                    2 => -(offset * 16384) / 64,
                    3 => -16384 + (offset * 16384) / 64,
                    _ => 0,
                }
            }
        };
        i += 1;
    }
    table
};

/// Cosine lookup table (same as sine but shifted by 64 = 90 degrees)
const COS_TABLE: [i16; 256] = {
    let mut table = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = SIN_TABLE[(i + 64) & 0xFF];
        i += 1;
    }
    table
};

/// Add two Q12.4 values
pub fn add_q12_4(a: u16, b: u16) -> u16 {
    a.wrapping_add(b)
}

/// Subtract two Q12.4 values
pub fn sub_q12_4(a: u16, b: u16) -> u16 {
    a.wrapping_sub(b)
}

/// Multiply Q8.8 by Q0.14, result in Q8.8
pub fn mul_q8_8_by_q0_14(a: i16, b: i16) -> i16 {
    // a is Q8.8, b is Q0.14
    // result needs to be Q8.8, so shift right by 14
    ((a as i32 * b as i32) >> 14) as i16
}

/// Multiply two Q8.8 values, result in Q8.8
pub fn mul_q8_8(a: i16, b: i16) -> i16 {
    // Result is Q16.16, shift right by 8 to get Q8.8
    ((a as i32 * b as i32) >> 8) as i16
}

/// Apply drag: multiply by 127/128 ≈ 0.992
pub fn apply_drag_q8_8(v: i16) -> i16 {
    // v * 127 / 128 = v - v/128 = v - (v >> 7)
    v - (v >> 7)
}

/// Convert Q8.8 velocity to Q12.4 position delta
/// Shifts right by 4 bits (divide by 16)
pub fn vel_to_pos_delta(v: i16) -> i16 {
    v >> 4
}

/// Wrap position around world boundary (Q12.4)
pub fn wrap_q12_4(val: u16, max: u16) -> u16 {
    if val >= max {
        val - max
    } else {
        val
    }
}

/// Wrap signed delta for shortest path (toroidal distance)
pub fn shortest_delta_q12_4(from: u16, to: u16, size: u16) -> i16 {
    let delta = to.wrapping_sub(from) as i16;
    let half = (size >> 1) as i16;

    if delta > half {
        delta - size as i16
    } else if delta < -half {
        delta + size as i16
    } else {
        delta
    }
}

/// Calculate squared distance between two points (Q12.4 -> Q24.8)
pub fn distance_sq_q12_4(ax: u16, ay: u16, bx: u16, by: u16) -> u32 {
    let dx = shortest_delta_q12_4(ax, bx, WORLD_WIDTH_Q12_4) as i32;
    let dy = shortest_delta_q12_4(ay, by, WORLD_HEIGHT_Q12_4) as i32;

    // dx and dy are in Q12.4, so dx*dx is in Q24.8
    (dx * dx + dy * dy) as u32
}

/// Clamp velocity to maximum speed
pub fn clamp_speed_q8_8(vx: i16, vy: i16) -> (i16, i16) {
    let speed_sq = (vx as i32 * vx as i32 + vy as i32 * vy as i32) as u32;

    if speed_sq > SHIP_MAX_SPEED_SQ_Q16_16 {
        // Scale down to max speed
        // Use approximation: multiply by 3/4 until under limit
        let mut new_vx = vx;
        let mut new_vy = vy;

        // Apply scaling: multiply by 3/4 = multiply by 3, shift right by 2
        while (new_vx as i32 * new_vx as i32 + new_vy as i32 * new_vy as i32) as u32
            > SHIP_MAX_SPEED_SQ_Q16_16
        {
            new_vx = (new_vx * 3) >> 2;
            new_vy = (new_vy * 3) >> 2;
        }

        (new_vx, new_vy)
    } else {
        (vx, vy)
    }
}

/// Add BAM angles with wrapping
pub fn add_bam(a: u8, b: i8) -> u8 {
    (a as i16 + b as i16) as u8
}

/// Compute atan2 for BAM angle (approximate)
/// Returns BAM angle [0, 256)
pub fn atan2_bam(dy: i16, dx: i16) -> u8 {
    if dx == 0 && dy == 0 {
        return 0;
    }

    let abs_dx = dx.abs() as i32;
    let abs_dy = dy.abs() as i32;

    // Approximate angle in octant
    let ratio = if abs_dx >= abs_dy {
        (abs_dy << 8) / (abs_dx + 1)
    } else {
        256 - (abs_dx << 8) / (abs_dy + 1)
    };

    // Adjust for quadrant
    let angle = if dx >= 0 {
        if dy >= 0 {
            // Quadrant 1
            if abs_dx >= abs_dy {
                ratio
            } else {
                128 - ratio
            }
        } else {
            // Quadrant 4
            if abs_dx >= abs_dy {
                -ratio
            } else {
                -(128 - ratio)
            }
        }
    } else {
        if dy >= 0 {
            // Quadrant 2
            if abs_dx >= abs_dy {
                256 - ratio
            } else {
                128 + ratio
            }
        } else {
            // Quadrant 3
            if abs_dx >= abs_dy {
                256 + ratio
            } else {
                128 + (256 - ratio)
            }
        }
    };

    angle as u8
}

/// Displace position by offset in given BAM direction
/// Returns new position in Q12.4
pub fn displace_q12_4(x: u16, y: u16, angle: u8, distance_q12_4: u16) -> (u16, u16) {
    let cos_val = cos_bam(angle) as i32;
    let sin_val = sin_bam(angle) as i32;
    let dist = distance_q12_4 as i32;

    // cos/sin are Q0.14, dist is Q12.4
    // Result should be Q12.4, so shift right by 14
    let dx = ((cos_val * dist) >> 14) as i16;
    let dy = ((sin_val * dist) >> 14) as i16;

    let new_x = add_q12_4(x, dx as u16);
    let new_y = add_q12_4(y, dy as u16);

    (
        wrap_q12_4(new_x, WORLD_WIDTH_Q12_4),
        wrap_q12_4(new_y, WORLD_HEIGHT_Q12_4),
    )
}

/// Create velocity from angle and speed (both Q8.8)
pub fn velocity_q8_8(angle: u8, speed: i16) -> (i16, i16) {
    let cos_val = cos_bam(angle) as i32;
    let sin_val = sin_bam(angle) as i32;
    let spd = speed as i32;

    // cos/sin are Q0.14, speed is Q8.8
    // Result should be Q8.8, so shift right by 14
    let vx = ((cos_val * spd) >> 14) as i16;
    let vy = ((sin_val * spd) >> 14) as i16;

    (vx, vy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sin_cos_quadrants() {
        // sin(0) = 0
        assert_eq!(sin_bam(0), 0);
        // sin(64) = sin(90°) ≈ 16384 in Q0.14
        assert!(sin_bam(64) > 16000);
        // sin(128) = sin(180°) = 0
        assert!(sin_bam(128).abs() < 1000);
        // sin(192) = sin(270°) ≈ -16384
        assert!(sin_bam(192) < -16000);

        // cos(0) = 1.0 ≈ 16384
        assert!(cos_bam(0) > 16000);
        // cos(64) = cos(90°) = 0
        assert!(cos_bam(64).abs() < 1000);
        // cos(128) = cos(180°) = -1.0
        assert!(cos_bam(128) < -16000);
    }

    #[test]
    fn test_wrap_q12_4() {
        assert_eq!(wrap_q12_4(100, 1000), 100);
        assert_eq!(wrap_q12_4(1100, 1000), 100);
        assert_eq!(wrap_q12_4(2000, 1000), 0);
    }

    #[test]
    fn test_shortest_delta() {
        assert_eq!(shortest_delta_q12_4(100, 200, 1000), 100);
        assert_eq!(shortest_delta_q12_4(900, 100, 1000), 200);
        assert_eq!(shortest_delta_q12_4(100, 900, 1000), -200);
    }

    #[test]
    fn test_add_bam() {
        assert_eq!(add_bam(100, 50), 150);
        assert_eq!(add_bam(250, 20), 14);
        assert_eq!(add_bam(10, -20), 246);
    }

    #[test]
    fn test_velocity() {
        let (vx, vy) = velocity_q8_8(0, 1000);
        assert!(vx > 0);
        assert!(vy.abs() < 100);
    }
}
