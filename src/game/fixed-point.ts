// === Trig Tables (Q0.14 format, scale 16384) ===
// Locked to the Rust verifier values for deterministic TS/Rust parity.
export const SIN_TABLE = new Int16Array([
  0, 402, 804, 1205, 1606, 2006, 2404, 2801, 3196, 3590, 3981, 4370, 4756, 5139, 5520, 5897, 6270,
  6639, 7005, 7366, 7723, 8076, 8423, 8765, 9102, 9434, 9760, 10080, 10394, 10702, 11003, 11297,
  11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395, 13623, 13842, 14053, 14256, 14449, 14635,
  14811, 14978, 15137, 15286, 15426, 15557, 15679, 15791, 15893, 15986, 16069, 16143, 16207, 16261,
  16305, 16340, 16364, 16379, 16384, 16379, 16364, 16340, 16305, 16261, 16207, 16143, 16069, 15986,
  15893, 15791, 15679, 15557, 15426, 15286, 15137, 14978, 14811, 14635, 14449, 14256, 14053, 13842,
  13623, 13395, 13160, 12916, 12665, 12406, 12140, 11866, 11585, 11297, 11003, 10702, 10394, 10080,
  9760, 9434, 9102, 8765, 8423, 8076, 7723, 7366, 7005, 6639, 6270, 5897, 5520, 5139, 4756, 4370,
  3981, 3590, 3196, 2801, 2404, 2006, 1606, 1205, 804, 402, 0, -402, -804, -1205, -1606, -2006,
  -2404, -2801, -3196, -3590, -3981, -4370, -4756, -5139, -5520, -5897, -6270, -6639, -7005, -7366,
  -7723, -8076, -8423, -8765, -9102, -9434, -9760, -10080, -10394, -10702, -11003, -11297, -11585,
  -11866, -12140, -12406, -12665, -12916, -13160, -13395, -13623, -13842, -14053, -14256, -14449,
  -14635, -14811, -14978, -15137, -15286, -15426, -15557, -15679, -15791, -15893, -15986, -16069,
  -16143, -16207, -16261, -16305, -16340, -16364, -16379, -16384, -16379, -16364, -16340, -16305,
  -16261, -16207, -16143, -16069, -15986, -15893, -15791, -15679, -15557, -15426, -15286, -15137,
  -14978, -14811, -14635, -14449, -14256, -14053, -13842, -13623, -13395, -13160, -12916, -12665,
  -12406, -12140, -11866, -11585, -11297, -11003, -10702, -10394, -10080, -9760, -9434, -9102,
  -8765, -8423, -8076, -7723, -7366, -7005, -6639, -6270, -5897, -5520, -5139, -4756, -4370, -3981,
  -3590, -3196, -2801, -2404, -2006, -1606, -1205, -804, -402,
]);
export const COS_TABLE = new Int16Array([
  16384, 16379, 16364, 16340, 16305, 16261, 16207, 16143, 16069, 15986, 15893, 15791, 15679, 15557,
  15426, 15286, 15137, 14978, 14811, 14635, 14449, 14256, 14053, 13842, 13623, 13395, 13160, 12916,
  12665, 12406, 12140, 11866, 11585, 11297, 11003, 10702, 10394, 10080, 9760, 9434, 9102, 8765,
  8423, 8076, 7723, 7366, 7005, 6639, 6270, 5897, 5520, 5139, 4756, 4370, 3981, 3590, 3196, 2801,
  2404, 2006, 1606, 1205, 804, 402, 0, -402, -804, -1205, -1606, -2006, -2404, -2801, -3196, -3590,
  -3981, -4370, -4756, -5139, -5520, -5897, -6270, -6639, -7005, -7366, -7723, -8076, -8423, -8765,
  -9102, -9434, -9760, -10080, -10394, -10702, -11003, -11297, -11585, -11866, -12140, -12406,
  -12665, -12916, -13160, -13395, -13623, -13842, -14053, -14256, -14449, -14635, -14811, -14978,
  -15137, -15286, -15426, -15557, -15679, -15791, -15893, -15986, -16069, -16143, -16207, -16261,
  -16305, -16340, -16364, -16379, -16384, -16379, -16364, -16340, -16305, -16261, -16207, -16143,
  -16069, -15986, -15893, -15791, -15679, -15557, -15426, -15286, -15137, -14978, -14811, -14635,
  -14449, -14256, -14053, -13842, -13623, -13395, -13160, -12916, -12665, -12406, -12140, -11866,
  -11585, -11297, -11003, -10702, -10394, -10080, -9760, -9434, -9102, -8765, -8423, -8076, -7723,
  -7366, -7005, -6639, -6270, -5897, -5520, -5139, -4756, -4370, -3981, -3590, -3196, -2801, -2404,
  -2006, -1606, -1205, -804, -402, 0, 402, 804, 1205, 1606, 2006, 2404, 2801, 3196, 3590, 3981,
  4370, 4756, 5139, 5520, 5897, 6270, 6639, 7005, 7366, 7723, 8076, 8423, 8765, 9102, 9434, 9760,
  10080, 10394, 10702, 11003, 11297, 11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395, 13623,
  13842, 14053, 14256, 14449, 14635, 14811, 14978, 15137, 15286, 15426, 15557, 15679, 15791, 15893,
  15986, 16069, 16143, 16207, 16261, 16305, 16340, 16364, 16379,
]);

// === Trig lookups ===
export function sinBAM(angle: number): number {
  return SIN_TABLE[angle & 0xff];
}
export function cosBAM(angle: number): number {
  return COS_TABLE[angle & 0xff];
}

// === Integer atan2 returning BAM (0-255) ===
// Uses octant decomposition + small lookup table

// Lookup: atan(i/32) scaled to 0-32 BAM (one octant), 33 entries for i=0..32.
const ATAN_TABLE = new Uint8Array([
  0, 1, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 25, 26, 27,
  28, 29, 29, 30, 31, 31, 32,
]);

export function atan2BAM(dy: number, dx: number): number {
  if (dx === 0 && dy === 0) return 0;

  const absDx = Math.abs(dx);
  const absDy = Math.abs(dy);

  // Compute ratio index (0-32) for the smaller/larger magnitude
  let ratio: number;
  let swapped: boolean;
  if (absDx >= absDy) {
    // Shift-then-divide keeps us in integer arithmetic; `| 0` truncates
    // toward zero, matching Rust's default integer division semantics.
    ratio = absDx === 0 ? 0 : ((absDy << 5) / absDx) | 0;
    swapped = false;
  } else {
    ratio = absDy === 0 ? 0 : ((absDx << 5) / absDy) | 0;
    swapped = true;
  }

  if (ratio > 32) ratio = 32;
  let angle = ATAN_TABLE[ratio];

  // If we swapped, complement within quadrant (64 = quarter turn)
  if (swapped) {
    angle = 64 - angle;
  }

  // Map to correct quadrant based on signs
  if (dx < 0) {
    angle = 128 - angle;
  }
  if (dy < 0) {
    angle = (256 - angle) & 0xff;
  }

  return angle & 0xff;
}

// === Conversion helpers (for rendering layer) ===
export function fromQ12_4(v: number): number {
  return v / 16;
}
export function toQ12_4(v: number): number {
  return Math.round(v * 16);
}
export function fromQ8_8(v: number): number {
  return v / 256;
}
export function toQ8_8(v: number): number {
  return Math.round(v * 256);
}
export function BAMToRadians(bam: number): number {
  return ((bam & 0xff) * Math.PI * 2) / 256;
}

// === Displacement helper ===
// Get Q12.4 displacement from BAM angle and pixel distance
export function displaceQ12_4(angle: number, distPixels: number): { dx: number; dy: number } {
  return {
    dx: (cosBAM(angle) * distPixels) >> 10, // Q0.14 * px >> 10 -> Q12.4
    dy: (sinBAM(angle) * distPixels) >> 10,
  };
}

// === Velocity from angle ===
// Get Q8.8 velocity components from BAM angle and Q8.8 speed
export function velocityQ8_8(angle: number, speedQ8_8: number): { vx: number; vy: number } {
  return {
    vx: (cosBAM(angle) * speedQ8_8) >> 14, // Q0.14 * Q8.8 >> 14 -> Q8.8
    vy: (sinBAM(angle) * speedQ8_8) >> 14,
  };
}

// === Drag (bit-shift approximation of x0.992) ===
export function applyDrag(v: number): number {
  return v - (v >> 7);
}

// === Speed clamp (squared comparison, no sqrt) ===
export function clampSpeedQ8_8(
  vx: number,
  vy: number,
  maxSqQ16_16: number,
): { vx: number; vy: number } {
  let speedSq = vx * vx + vy * vy; // Q16.16
  if (speedSq <= maxSqQ16_16) return { vx, vy };
  // Iterative scale-down by 3/4 until under limit
  while (speedSq > maxSqQ16_16) {
    vx = (vx * 3) >> 2;
    vy = (vy * 3) >> 2;
    speedSq = vx * vx + vy * vy;
  }
  return { vx, vy };
}
