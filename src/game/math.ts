import { WORLD_HEIGHT, WORLD_WIDTH } from "./constants";
import type { Vec2 } from "./types";

export function randomRange(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

export function randomInt(min: number, maxExclusive: number): number {
  return Math.floor(randomRange(min, maxExclusive));
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
