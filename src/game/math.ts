import { WORLD_HEIGHT, WORLD_HEIGHT_Q12_4, WORLD_WIDTH, WORLD_WIDTH_Q12_4 } from "./constants";
import { SeededRng } from "./rng";
import type { Vec2 } from "./types";

// Global game RNG instance (deterministic — used in ZK proofs)
let gameRng: SeededRng = new SeededRng(Date.now());

// Visual-only RNG instance (NOT used in ZK proofs)
let visualRng: SeededRng = new SeededRng(Date.now() ^ 0x12345678);

export function setGameSeed(seed: number): void {
  gameRng = new SeededRng(seed);
  visualRng = new SeededRng(seed ^ 0x12345678);
}

export function getGameRng(): SeededRng {
  return gameRng;
}

export function getGameRngState(): number {
  return gameRng.getState();
}

export function randomRange(min: number, max: number): number {
  return gameRng.nextFloatRange(min, max);
}

export function randomInt(min: number, maxExclusive: number): number {
  return gameRng.nextRange(min, maxExclusive);
}

// Visual-only random functions (NOT used in ZK proofs)
export function visualRandomRange(min: number, max: number): number {
  return visualRng.nextFloatRange(min, max);
}

export function visualRandomInt(min: number, maxExclusive: number): number {
  return visualRng.nextRange(min, maxExclusive);
}

export function angleToVector(angle: number): Vec2 {
  return {
    x: Math.cos(angle),
    y: Math.sin(angle),
  };
}

export function wrapX(x: number): number {
  if (x < 0) {
    return x + WORLD_WIDTH;
  }

  if (x >= WORLD_WIDTH) {
    return x - WORLD_WIDTH;
  }

  return x;
}

export function wrapY(y: number): number {
  if (y < 0) {
    return y + WORLD_HEIGHT;
  }

  if (y >= WORLD_HEIGHT) {
    return y - WORLD_HEIGHT;
  }

  return y;
}

export function wrapPosition(position: Vec2): Vec2 {
  return {
    x: wrapX(position.x),
    y: wrapY(position.y),
  };
}

export function distanceSquared(a: Vec2, b: Vec2): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return dx * dx + dy * dy;
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function wrapXQ12_4(x: number): number {
  // Fast-path: positions are almost always already in-range.
  if (x >>> 0 < WORLD_WIDTH_Q12_4) return x;
  if (x < 0) return x + WORLD_WIDTH_Q12_4;
  return x - WORLD_WIDTH_Q12_4;
}

export function wrapYQ12_4(y: number): number {
  if (y >>> 0 < WORLD_HEIGHT_Q12_4) return y;
  if (y < 0) return y + WORLD_HEIGHT_Q12_4;
  return y - WORLD_HEIGHT_Q12_4;
}
