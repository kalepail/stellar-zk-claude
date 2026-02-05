export type GameMode = "menu" | "playing" | "paused" | "game-over" | "replay";

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
  // Previous frame position for interpolation
  prevX: number;
  prevY: number;
  prevAngle: number;
}

export interface Ship extends Entity {
  canControl: boolean;
  fireCooldown: number;
  respawnTimer: number;
  invulnerableTimer: number;
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
  baseAlpha: number;
  twinkleSpeed: number;
  twinklePhase: number;
}

export interface Particle {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  life: number;
  maxLife: number;
  size: number;
  color: string;
  alpha: number;
  decay: number;
  type: "spark" | "smoke" | "debris" | "glow";
}

export interface Debris {
  id: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  angle: number;
  spin: number;
  life: number;
  maxLife: number;
  size: number;
  vertices: number[];
}
