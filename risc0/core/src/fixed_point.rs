//! Fixed-point math for ZK-deterministic game logic.
//!
//! Formats:
//! - Q12.4 positions: multiply pixel coords by 16
//! - Q8.8 velocities: multiply px/frame by 256
//! - 8-bit BAM angles: 256 steps per full rotation
//! - Q0.14 trig tables: sin/cos values scaled by 16384

/// Pre-computed sin table in Q0.14 format (256 entries).
/// SIN_TABLE[i] = round(sin(i * 2pi / 256) * 16384)
/// Generated to match TypeScript: Math.round(Math.sin(i*Math.PI*2/256)*16384)
static SIN_TABLE: [i16; 256] = [
    0, 402, 804, 1205, 1606, 2006, 2404, 2801, 3196, 3590, 3981, 4370, 4756, 5139, 5520, 5897,
    6270, 6639, 7005, 7366, 7723, 8076, 8423, 8765, 9102, 9434, 9760, 10080, 10394, 10702, 11003, 11297,
    11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395, 13623, 13842, 14053, 14256, 14449, 14635, 14811, 14978,
    15137, 15286, 15426, 15557, 15679, 15791, 15893, 15986, 16069, 16143, 16207, 16261, 16305, 16340, 16364, 16379,
    16384, 16379, 16364, 16340, 16305, 16261, 16207, 16143, 16069, 15986, 15893, 15791, 15679, 15557, 15426, 15286,
    15137, 14978, 14811, 14635, 14449, 14256, 14053, 13842, 13623, 13395, 13160, 12916, 12665, 12406, 12140, 11866,
    11585, 11297, 11003, 10702, 10394, 10080, 9760, 9434, 9102, 8765, 8423, 8076, 7723, 7366, 7005, 6639,
    6270, 5897, 5520, 5139, 4756, 4370, 3981, 3590, 3196, 2801, 2404, 2006, 1606, 1205, 804, 402,
    0, -402, -804, -1205, -1606, -2006, -2404, -2801, -3196, -3590, -3981, -4370, -4756, -5139, -5520, -5897,
    -6270, -6639, -7005, -7366, -7723, -8076, -8423, -8765, -9102, -9434, -9760, -10080, -10394, -10702, -11003, -11297,
    -11585, -11866, -12140, -12406, -12665, -12916, -13160, -13395, -13623, -13842, -14053, -14256, -14449, -14635, -14811, -14978,
    -15137, -15286, -15426, -15557, -15679, -15791, -15893, -15986, -16069, -16143, -16207, -16261, -16305, -16340, -16364, -16379,
    -16384, -16379, -16364, -16340, -16305, -16261, -16207, -16143, -16069, -15986, -15893, -15791, -15679, -15557, -15426, -15286,
    -15137, -14978, -14811, -14635, -14449, -14256, -14053, -13842, -13623, -13395, -13160, -12916, -12665, -12406, -12140, -11866,
    -11585, -11297, -11003, -10702, -10394, -10080, -9760, -9434, -9102, -8765, -8423, -8076, -7723, -7366, -7005, -6639,
    -6270, -5897, -5520, -5139, -4756, -4370, -3981, -3590, -3196, -2801, -2404, -2006, -1606, -1205, -804, -402,
];

/// Pre-computed cos table in Q0.14 format (256 entries).
/// COS_TABLE[i] = round(cos(i * 2pi / 256) * 16384)
static COS_TABLE: [i16; 256] = [
    16384, 16379, 16364, 16340, 16305, 16261, 16207, 16143, 16069, 15986, 15893, 15791, 15679, 15557, 15426, 15286,
    15137, 14978, 14811, 14635, 14449, 14256, 14053, 13842, 13623, 13395, 13160, 12916, 12665, 12406, 12140, 11866,
    11585, 11297, 11003, 10702, 10394, 10080, 9760, 9434, 9102, 8765, 8423, 8076, 7723, 7366, 7005, 6639,
    6270, 5897, 5520, 5139, 4756, 4370, 3981, 3590, 3196, 2801, 2404, 2006, 1606, 1205, 804, 402,
    0, -402, -804, -1205, -1606, -2006, -2404, -2801, -3196, -3590, -3981, -4370, -4756, -5139, -5520, -5897,
    -6270, -6639, -7005, -7366, -7723, -8076, -8423, -8765, -9102, -9434, -9760, -10080, -10394, -10702, -11003, -11297,
    -11585, -11866, -12140, -12406, -12665, -12916, -13160, -13395, -13623, -13842, -14053, -14256, -14449, -14635, -14811, -14978,
    -15137, -15286, -15426, -15557, -15679, -15791, -15893, -15986, -16069, -16143, -16207, -16261, -16305, -16340, -16364, -16379,
    -16384, -16379, -16364, -16340, -16305, -16261, -16207, -16143, -16069, -15986, -15893, -15791, -15679, -15557, -15426, -15286,
    -15137, -14978, -14811, -14635, -14449, -14256, -14053, -13842, -13623, -13395, -13160, -12916, -12665, -12406, -12140, -11866,
    -11585, -11297, -11003, -10702, -10394, -10080, -9760, -9434, -9102, -8765, -8423, -8076, -7723, -7366, -7005, -6639,
    -6270, -5897, -5520, -5139, -4756, -4370, -3981, -3590, -3196, -2801, -2404, -2006, -1606, -1205, -804, -402,
    0, 402, 804, 1205, 1606, 2006, 2404, 2801, 3196, 3590, 3981, 4370, 4756, 5139, 5520, 5897,
    6270, 6639, 7005, 7366, 7723, 8076, 8423, 8765, 9102, 9434, 9760, 10080, 10394, 10702, 11003, 11297,
    11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395, 13623, 13842, 14053, 14256, 14449, 14635, 14811, 14978,
    15137, 15286, 15426, 15557, 15679, 15791, 15893, 15986, 16069, 16143, 16207, 16261, 16305, 16340, 16364, 16379,
];

/// Atan lookup table: atan(i/32) scaled to BAM for one octant, 33 entries.
/// ATAN_TABLE[i] = round(atan(i/32) * 128 / PI)
static ATAN_TABLE: [u8; 33] = [
    0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18,
    19, 20, 21, 22, 23, 24, 25, 25, 26, 27, 28, 29, 29, 30, 31, 31, 32,
];

/// Sin lookup using 8-bit BAM angle. Returns Q0.14 value.
#[inline]
pub fn sin_bam(angle: u8) -> i32 {
    SIN_TABLE[angle as usize] as i32
}

/// Cos lookup using 8-bit BAM angle. Returns Q0.14 value.
#[inline]
pub fn cos_bam(angle: u8) -> i32 {
    COS_TABLE[angle as usize] as i32
}

/// Integer atan2 returning BAM (0-255).
/// Uses octant decomposition + small lookup table.
/// Matches the TypeScript atan2BAM exactly.
pub fn atan2_bam(dy: i32, dx: i32) -> u8 {
    if dx == 0 && dy == 0 {
        return 0;
    }

    let abs_dx = dx.unsigned_abs();
    let abs_dy = dy.unsigned_abs();

    let (ratio, swapped) = if abs_dx >= abs_dy {
        let r = if abs_dx == 0 { 0 } else { ((abs_dy * 32) / abs_dx) as usize };
        (r, false)
    } else {
        let r = if abs_dy == 0 { 0 } else { ((abs_dx * 32) / abs_dy) as usize };
        (r, true)
    };

    let ratio = ratio.min(32);
    let mut angle = ATAN_TABLE[ratio] as i32;

    // If we swapped, complement within quadrant (64 = quarter turn)
    if swapped {
        angle = 64 - angle;
    }

    // Map to correct quadrant based on signs
    if dx < 0 {
        angle = 128 - angle;
    }
    if dy < 0 {
        angle = (256 - angle) & 0xFF;
    }

    (angle & 0xFF) as u8
}

/// Get Q8.8 velocity components from BAM angle and Q8.8 speed.
/// vx = (cos(angle) * speed) >> 14  (Q0.14 * Q8.8 >> 14 = Q8.8)
#[inline]
pub fn velocity_q8_8(angle: u8, speed_q8_8: i32) -> (i32, i32) {
    let vx = (cos_bam(angle) * speed_q8_8) >> 14;
    let vy = (sin_bam(angle) * speed_q8_8) >> 14;
    (vx, vy)
}

/// Get Q12.4 displacement from BAM angle and pixel distance.
/// dx = (cos(angle) * dist_pixels) >> 10  (Q0.14 * px >> 10 = Q12.4)
#[inline]
pub fn displace_q12_4(angle: u8, dist_pixels: i32) -> (i32, i32) {
    let dx = (cos_bam(angle) * dist_pixels) >> 10;
    let dy = (sin_bam(angle) * dist_pixels) >> 10;
    (dx, dy)
}

/// Drag approximation: v - (v >> 7) = v * 127/128 ~ 0.992x
#[inline]
pub fn apply_drag(v: i32) -> i32 {
    v - (v >> 7)
}

/// Speed clamp using squared comparison (no sqrt).
/// Iteratively scales down by 3/4 until speed^2 <= max_sq.
#[inline]
pub fn clamp_speed_q8_8(mut vx: i32, mut vy: i32, max_sq_q16_16: i32) -> (i32, i32) {
    let mut speed_sq = vx * vx + vy * vy;
    if speed_sq <= max_sq_q16_16 {
        return (vx, vy);
    }
    while speed_sq > max_sq_q16_16 {
        vx = (vx * 3) >> 2;
        vy = (vy * 3) >> 2;
        speed_sq = vx * vx + vy * vy;
    }
    (vx, vy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trig_tables() {
        // sin(0) = 0, cos(0) = 16384
        assert_eq!(sin_bam(0), 0);
        assert_eq!(cos_bam(0), 16384);

        // sin(64) = sin(90deg) = 16384, cos(64) = cos(90deg) = 0
        assert_eq!(sin_bam(64), 16384);
        assert_eq!(cos_bam(64), 0);

        // sin(128) = sin(180deg) = 0, cos(128) = cos(180deg) = -16384
        assert_eq!(sin_bam(128), 0);
        assert_eq!(cos_bam(128), -16384);

        // sin(192) = sin(270deg) = -16384 (ship facing up), cos(192) = 0
        assert_eq!(sin_bam(192), -16384);
        assert_eq!(cos_bam(192), 0);
    }

    #[test]
    fn test_drag() {
        assert_eq!(apply_drag(1000), 1000 - (1000 >> 7));
        assert_eq!(apply_drag(-1000), -1000 - (-1000 >> 7));
        assert_eq!(apply_drag(0), 0);
    }

    #[test]
    fn test_velocity_from_angle() {
        // angle 0 (right): vx = speed, vy = 0
        let (vx, vy) = velocity_q8_8(0, 256);
        assert_eq!(vx, 256); // cos(0) * 256 >> 14 = 16384 * 256 >> 14 = 256
        assert_eq!(vy, 0);

        // angle 192 (up): vx = 0, vy = -speed
        let (vx, vy) = velocity_q8_8(192, 256);
        assert_eq!(vx, 0);
        assert_eq!(vy, -256);
    }

    #[test]
    fn test_clamp_speed() {
        // Under limit: no change
        let (vx, vy) = clamp_speed_q8_8(100, 100, 100 * 100 + 100 * 100 + 1);
        assert_eq!((vx, vy), (100, 100));

        // Over limit: should reduce
        let (vx, vy) = clamp_speed_q8_8(2000, 2000, 1451 * 1451);
        assert!(vx * vx + vy * vy <= 1451 * 1451);
    }
}
