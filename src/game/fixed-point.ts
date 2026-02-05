// === Trig Tables (Q0.14 format, scale 16384) ===
export const SIN_TABLE = new Int16Array(256);
export const COS_TABLE = new Int16Array(256);
for (let i = 0; i < 256; i++) {
  SIN_TABLE[i] = Math.round(Math.sin((i * Math.PI * 2) / 256) * 16384);
  COS_TABLE[i] = Math.round(Math.cos((i * Math.PI * 2) / 256) * 16384);
}

// === Trig lookups ===
export function sinBAM(angle: number): number {
  return SIN_TABLE[angle & 0xff];
}
export function cosBAM(angle: number): number {
  return COS_TABLE[angle & 0xff];
}

// === Integer atan2 returning BAM (0-255) ===
// Uses octant decomposition + small lookup table

// Lookup: atan(i/32) scaled to 0-32 BAM (one octant), 33 entries for i=0..32
const ATAN_TABLE = new Uint8Array(33);
for (let i = 0; i <= 32; i++) {
  ATAN_TABLE[i] = Math.round(Math.atan(i / 32) * (128 / Math.PI));
}

export function atan2BAM(dy: number, dx: number): number {
  if (dx === 0 && dy === 0) return 0;

  const absDx = Math.abs(dx);
  const absDy = Math.abs(dy);

  // Compute ratio index (0-32) for the smaller/larger magnitude
  let ratio: number;
  let swapped: boolean;
  if (absDx >= absDy) {
    ratio = absDx === 0 ? 0 : ((absDy * 32) / absDx) | 0;
    swapped = false;
  } else {
    ratio = absDy === 0 ? 0 : ((absDx * 32) / absDy) | 0;
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
