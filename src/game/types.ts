export type GameMode = "menu" | "playing" | "paused" | "game-over";

export type AsteroidSize = "large" | "medium" | "small";

export interface Vec2 {
  x: number;
  y: number;
}

export interface Entity {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  angle: number;
  alive: boolean;
  radius: number;
}

export interface Ship extends Entity {
  canControl: boolean;
  fireCooldown: number;
  respawnTimer: number;
  invulnerableTimer: number;
  hyperspaceCooldown: number;
}

export interface Asteroid extends Entity {
  size: AsteroidSize;
  spin: number;
  vertices: number[];
}

export interface Bullet extends Entity {
  life: number;
  fromSaucer: boolean;
}

export interface Saucer extends Entity {
  small: boolean;
  fireCooldown: number;
  driftTimer: number;
}

export interface Star {
  x: number;
  y: number;
  alpha: number;
}
