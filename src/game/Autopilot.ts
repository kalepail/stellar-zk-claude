import {
  SHIP_BULLET_LIFETIME,
  SHIP_BULLET_SPEED,
  SHIP_MAX_SPEED,
  SHIP_RADIUS,
  WORLD_HEIGHT,
  WORLD_WIDTH,
} from "./constants";
import type { Asteroid, Bullet, Saucer, Ship, Vec2 } from "./types";

/**
 * AI Autopilot Engine for Stellar ZK Asteroids
 *
 * This class provides intelligent ship control by analyzing the game state
 * and generating appropriate inputs. It's designed to be modular and easy
 * to modify - all tunable parameters are at the top of the class.
 *
 * The AI uses a threat-based priority system:
 * 1. Immediate collision threats (dodge)
 * 2. Incoming bullets (evade)
 * 3. Nearest targetable enemy (engage)
 */

// ============================================================================
// TUNABLE PARAMETERS - Modify these to change AI behavior
// ============================================================================

/** How far ahead to predict collisions (seconds) */
const COLLISION_LOOKAHEAD = 1.5;

/** Distance at which threats become critical and require evasion */
const DANGER_RADIUS = 120;

/** Distance at which we start being cautious */
const CAUTION_RADIUS = 200;

/** How accurately the ship must aim before firing (radians) */
const AIM_TOLERANCE = 0.12;

/** Minimum distance to maintain from threats when not attacking */
const SAFE_DISTANCE = 180;

/** How much to lead targets (multiplier for prediction) */
const LEAD_FACTOR = 1.0;

/** Prefer shooting small asteroids first (they're worth more points) */
const PREFER_SMALL_ASTEROIDS = false;

/** Maximum angle difference to consider a shot viable */
const MAX_SHOT_ANGLE = Math.PI / 6; // 30 degrees

/** How aggressively to pursue targets vs play defensively (0-1) */
const AGGRESSION = 0.7;

/** Cooldown between shots to avoid wasting bullets */
const SHOT_PATIENCE = 0.05;

// ============================================================================
// AUTOPILOT CLASS
// ============================================================================

export interface AutopilotInput {
  left: boolean;
  right: boolean;
  thrust: boolean;
  fire: boolean;
  hyperspace: boolean;
}

export interface GameStateSnapshot {
  ship: Ship;
  asteroids: Asteroid[];
  saucers: Saucer[];
  bullets: Bullet[]; // Player bullets
  saucerBullets: Bullet[];
}

interface Threat {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  type: "asteroid" | "saucer" | "bullet";
  danger: number; // 0-1 danger level
  timeToImpact: number;
  entity: Asteroid | Saucer | Bullet;
}

interface Target {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  priority: number;
  angle: number; // Angle to aim at
  distance: number;
  entity: Asteroid | Saucer;
}

export class Autopilot {
  private enabled = false;
  private lastShotTime = 0;

  // Debug/visualization data
  private debugThreats: Threat[] = [];
  private debugTarget: Target | null = null;

  /** Enable or disable the autopilot */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    if (!enabled) {
      this.debugTarget = null;
      this.debugThreats = [];
    }
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  /** Toggle autopilot on/off */
  toggle(): boolean {
    this.enabled = !this.enabled;
    return this.enabled;
  }

  /** Get debug visualization data */
  getDebugData(): { threats: Threat[]; target: Target | null } {
    return {
      threats: this.debugThreats,
      target: this.debugTarget,
    };
  }

  /**
   * Main update function - analyzes game state and returns input commands
   */
  update(state: GameStateSnapshot, _dt: number, gameTime: number): AutopilotInput {
    const input: AutopilotInput = {
      left: false,
      right: false,
      thrust: false,
      fire: false,
      hyperspace: false,
    };

    if (!this.enabled || !state.ship.canControl || !state.ship.alive) {
      return input;
    }

    const ship = state.ship;

    // 1. Analyze all threats
    const threats = this.analyzeThreats(ship, state);
    this.debugThreats = threats;

    // 2. Check for immediate danger requiring evasion
    const criticalThreats = threats.filter((t) => t.danger > 0.7);
    const shouldEvade = criticalThreats.length > 0;

    if (shouldEvade) {
      return this.generateEvasionInput(ship, criticalThreats);
    }

    // 3. Find best target to engage
    const target = this.selectTarget(ship, state);
    this.debugTarget = target;

    if (!target) {
      // No targets - just drift safely
      return this.generateIdleInput(ship, threats);
    }

    // 4. Generate attack input
    return this.generateAttackInput(ship, target, threats, gameTime);
  }

  // ============================================================================
  // THREAT ANALYSIS
  // ============================================================================

  private analyzeThreats(ship: Ship, state: GameStateSnapshot): Threat[] {
    const threats: Threat[] = [];

    // Analyze asteroids
    for (const asteroid of state.asteroids) {
      if (!asteroid.alive) continue;

      const threat = this.assessThreat(ship, asteroid, "asteroid");
      if (threat) threats.push(threat);
    }

    // Analyze saucers
    for (const saucer of state.saucers) {
      if (!saucer.alive) continue;

      const threat = this.assessThreat(ship, saucer, "saucer");
      if (threat) {
        // Saucers are more dangerous
        threat.danger = Math.min(1, threat.danger * 1.3);
        threats.push(threat);
      }
    }

    // Analyze saucer bullets (most dangerous!)
    for (const bullet of state.saucerBullets) {
      if (!bullet.alive) continue;

      const threat = this.assessThreat(ship, bullet, "bullet");
      if (threat) {
        // Bullets are very dangerous
        threat.danger = Math.min(1, threat.danger * 1.5);
        threats.push(threat);
      }
    }

    // Sort by danger level
    threats.sort((a, b) => b.danger - a.danger);

    return threats;
  }

  private assessThreat(
    ship: Ship,
    entity: Asteroid | Saucer | Bullet,
    type: "asteroid" | "saucer" | "bullet"
  ): Threat | null {
    const delta = this.shortestDelta(ship.x, ship.y, entity.x, entity.y);
    const distance = Math.sqrt(delta.x * delta.x + delta.y * delta.y);

    // Skip if too far
    if (distance > CAUTION_RADIUS * 2) return null;

    // Calculate relative velocity
    const relVx = entity.vx - ship.vx;
    const relVy = entity.vy - ship.vy;

    // Time to closest approach
    const timeToImpact = this.timeToClosestApproach(
      0,
      0,
      relVx,
      relVy,
      delta.x,
      delta.y
    );

    // Only care about future threats
    if (timeToImpact < 0 || timeToImpact > COLLISION_LOOKAHEAD) {
      // Still track nearby entities as low-level threats
      if (distance < CAUTION_RADIUS) {
        return {
          x: entity.x,
          y: entity.y,
          vx: entity.vx,
          vy: entity.vy,
          radius: entity.radius,
          type,
          danger: 0.2 * (1 - distance / CAUTION_RADIUS),
          timeToImpact: 999,
          entity,
        };
      }
      return null;
    }

    // Calculate closest approach distance
    const futureX = delta.x + relVx * timeToImpact;
    const futureY = delta.y + relVy * timeToImpact;
    const closestDistance = Math.sqrt(futureX * futureX + futureY * futureY);

    const collisionRadius = SHIP_RADIUS + entity.radius + 20; // Buffer

    // Calculate danger level
    let danger = 0;

    if (closestDistance < collisionRadius) {
      // Will collide!
      danger = 1.0;
    } else if (closestDistance < DANGER_RADIUS) {
      danger = 0.8 * (1 - closestDistance / DANGER_RADIUS);
    } else if (closestDistance < CAUTION_RADIUS) {
      danger = 0.3 * (1 - closestDistance / CAUTION_RADIUS);
    }

    // Increase danger for faster-approaching threats
    const approachSpeed = Math.sqrt(relVx * relVx + relVy * relVy);
    danger *= 1 + approachSpeed / 200;

    // Time pressure increases danger
    if (timeToImpact < 0.5) {
      danger *= 1.5;
    }

    danger = Math.min(1, danger);

    if (danger < 0.1) return null;

    return {
      x: entity.x,
      y: entity.y,
      vx: entity.vx,
      vy: entity.vy,
      radius: entity.radius,
      type,
      danger,
      timeToImpact,
      entity,
    };
  }

  // ============================================================================
  // TARGET SELECTION
  // ============================================================================

  private selectTarget(ship: Ship, state: GameStateSnapshot): Target | null {
    const targets: Target[] = [];

    // Score asteroids as targets
    for (const asteroid of state.asteroids) {
      if (!asteroid.alive) continue;

      const target = this.scoreTarget(ship, asteroid);
      if (target) targets.push(target);
    }

    // Score saucers as targets (high priority!)
    for (const saucer of state.saucers) {
      if (!saucer.alive) continue;

      const target = this.scoreTarget(ship, saucer);
      if (target) {
        target.priority *= 2; // Saucers are priority targets
        targets.push(target);
      }
    }

    if (targets.length === 0) return null;

    // Sort by priority
    targets.sort((a, b) => b.priority - a.priority);

    return targets[0];
  }

  private scoreTarget(
    ship: Ship,
    entity: Asteroid | Saucer
  ): Target | null {
    const delta = this.shortestDelta(ship.x, ship.y, entity.x, entity.y);
    const distance = Math.sqrt(delta.x * delta.x + delta.y * delta.y);

    // Calculate lead angle (where to aim to hit moving target)
    const leadPos = this.calculateLeadPosition(ship, entity);
    const leadDelta = this.shortestDelta(ship.x, ship.y, leadPos.x, leadPos.y);
    const aimAngle = Math.atan2(leadDelta.y, leadDelta.x);

    // Calculate angle difference
    const angleDiff = this.normalizeAngle(aimAngle - ship.angle);

    // Priority based on distance (closer = higher priority)
    let priority = 1 / (distance + 50);

    // Bonus for targets we're already aimed at
    if (Math.abs(angleDiff) < MAX_SHOT_ANGLE) {
      priority *= 1.5;
    }

    // Asteroid size priority
    if ("size" in entity) {
      const asteroid = entity as Asteroid;
      if (PREFER_SMALL_ASTEROIDS) {
        if (asteroid.size === "small") priority *= 1.5;
        else if (asteroid.size === "medium") priority *= 1.2;
      } else {
        // Prefer large asteroids (easier to hit)
        if (asteroid.size === "large") priority *= 1.3;
      }
    }

    // Penalize targets that are too close (dangerous to engage)
    if (distance < SAFE_DISTANCE * 0.5) {
      priority *= 0.5;
    }

    return {
      x: entity.x,
      y: entity.y,
      vx: entity.vx,
      vy: entity.vy,
      radius: entity.radius,
      priority,
      angle: aimAngle,
      distance,
      entity,
    };
  }

  private calculateLeadPosition(ship: Ship, entity: Asteroid | Saucer): Vec2 {
    const delta = this.shortestDelta(ship.x, ship.y, entity.x, entity.y);
    const distance = Math.sqrt(delta.x * delta.x + delta.y * delta.y);

    // Estimate bullet travel time
    const bulletSpeed = SHIP_BULLET_SPEED + Math.hypot(ship.vx, ship.vy) * 0.35;
    const travelTime = distance / bulletSpeed;

    // Lead the target
    const leadX = entity.x + entity.vx * travelTime * LEAD_FACTOR;
    const leadY = entity.y + entity.vy * travelTime * LEAD_FACTOR;

    return { x: leadX, y: leadY };
  }

  // ============================================================================
  // INPUT GENERATION
  // ============================================================================

  private generateEvasionInput(
    ship: Ship,
    criticalThreats: Threat[]
  ): AutopilotInput {
    const input: AutopilotInput = {
      left: false,
      right: false,
      thrust: false,
      fire: false,
      hyperspace: false,
    };

    // Calculate escape vector (away from all threats, weighted by danger)
    let escapeX = 0;
    let escapeY = 0;

    for (const threat of criticalThreats) {
      const delta = this.shortestDelta(ship.x, ship.y, threat.x, threat.y);
      const distance = Math.sqrt(delta.x * delta.x + delta.y * delta.y) || 1;

      // Vector away from threat, weighted by danger
      escapeX -= (delta.x / distance) * threat.danger;
      escapeY -= (delta.y / distance) * threat.danger;
    }

    // Normalize escape vector
    const escapeMag = Math.sqrt(escapeX * escapeX + escapeY * escapeY) || 1;
    escapeX /= escapeMag;
    escapeY /= escapeMag;

    // Desired escape angle
    const escapeAngle = Math.atan2(escapeY, escapeX);
    const angleDiff = this.normalizeAngle(escapeAngle - ship.angle);

    // Turn toward escape direction
    if (Math.abs(angleDiff) > 0.1) {
      if (angleDiff > 0) {
        input.right = true;
      } else {
        input.left = true;
      }
    }

    // Thrust if roughly facing escape direction
    if (Math.abs(angleDiff) < Math.PI / 2) {
      input.thrust = true;
    }

    // Opportunistic shot if a threat is in our sights
    const mostDangerous = criticalThreats[0];
    if (mostDangerous) {
      const leadPos = this.calculateLeadPosition(ship, mostDangerous.entity as Asteroid);
      const leadDelta = this.shortestDelta(ship.x, ship.y, leadPos.x, leadPos.y);
      const aimAngle = Math.atan2(leadDelta.y, leadDelta.x);
      const aimDiff = Math.abs(this.normalizeAngle(aimAngle - ship.angle));

      if (aimDiff < AIM_TOLERANCE * 2) {
        input.fire = true;
      }
    }

    return input;
  }

  private generateAttackInput(
    ship: Ship,
    target: Target,
    threats: Threat[],
    gameTime: number
  ): AutopilotInput {
    const input: AutopilotInput = {
      left: false,
      right: false,
      thrust: false,
      fire: false,
      hyperspace: false,
    };

    // Calculate angle to target
    const angleDiff = this.normalizeAngle(target.angle - ship.angle);

    // Turn toward target
    if (Math.abs(angleDiff) > AIM_TOLERANCE / 2) {
      if (angleDiff > 0) {
        input.right = true;
      } else {
        input.left = true;
      }
    }

    // Fire if aimed
    if (Math.abs(angleDiff) < AIM_TOLERANCE) {
      // Check bullet will reach target
      const bulletRange = SHIP_BULLET_SPEED * SHIP_BULLET_LIFETIME;
      if (target.distance < bulletRange * 0.9) {
        // Rate limit shots
        if (gameTime - this.lastShotTime > SHOT_PATIENCE) {
          input.fire = true;
          this.lastShotTime = gameTime;
        }
      }
    }

    // Thrust management
    const speed = Math.hypot(ship.vx, ship.vy);
    const moderateThreats = threats.filter((t) => t.danger > 0.3);

    if (moderateThreats.length > 0) {
      // Threats nearby - be cautious about thrusting
      // Only thrust if moving away from threats
      const threatCenter = this.averagePosition(moderateThreats);
      const toThreat = this.shortestDelta(ship.x, ship.y, threatCenter.x, threatCenter.y);
      const thrustDir = { x: Math.cos(ship.angle), y: Math.sin(ship.angle) };

      // Dot product: positive means thrusting toward threat
      const dot = toThreat.x * thrustDir.x + toThreat.y * thrustDir.y;

      if (dot < 0) {
        // Thrusting away from threats
        input.thrust = true;
      }
    } else {
      // No immediate threats
      // Approach target if far, maintain distance if close
      if (target.distance > SAFE_DISTANCE * 1.5) {
        // Move toward target if aimed roughly at it
        if (Math.abs(angleDiff) < Math.PI / 3) {
          input.thrust = speed < SHIP_MAX_SPEED * AGGRESSION;
        }
      } else if (target.distance < SAFE_DISTANCE * 0.8) {
        // Too close - thrust away
        const awayAngle = this.normalizeAngle(target.angle + Math.PI - ship.angle);
        if (Math.abs(awayAngle) < Math.PI / 3) {
          input.thrust = true;
        }
      }
    }

    return input;
  }

  private generateIdleInput(ship: Ship, threats: Threat[]): AutopilotInput {
    const input: AutopilotInput = {
      left: false,
      right: false,
      thrust: false,
      fire: false,
      hyperspace: false,
    };

    // If there are any threats, rotate toward center for safety
    if (threats.length > 0) {
      const center = { x: WORLD_WIDTH / 2, y: WORLD_HEIGHT / 2 };
      const delta = this.shortestDelta(ship.x, ship.y, center.x, center.y);
      const centerAngle = Math.atan2(delta.y, delta.x);
      const angleDiff = this.normalizeAngle(centerAngle - ship.angle);

      if (Math.abs(angleDiff) > 0.2) {
        if (angleDiff > 0) input.right = true;
        else input.left = true;
      }
    }

    return input;
  }

  // ============================================================================
  // UTILITY FUNCTIONS
  // ============================================================================

  /** Calculate shortest delta in toroidal world */
  private shortestDelta(fromX: number, fromY: number, toX: number, toY: number): Vec2 {
    let dx = toX - fromX;
    let dy = toY - fromY;

    if (dx > WORLD_WIDTH / 2) dx -= WORLD_WIDTH;
    if (dx < -WORLD_WIDTH / 2) dx += WORLD_WIDTH;
    if (dy > WORLD_HEIGHT / 2) dy -= WORLD_HEIGHT;
    if (dy < -WORLD_HEIGHT / 2) dy += WORLD_HEIGHT;

    return { x: dx, y: dy };
  }

  /** Normalize angle to [-PI, PI] */
  private normalizeAngle(angle: number): number {
    while (angle > Math.PI) angle -= Math.PI * 2;
    while (angle < -Math.PI) angle += Math.PI * 2;
    return angle;
  }

  /** Calculate time to closest approach between two objects */
  private timeToClosestApproach(
    _x1: number,
    _y1: number,
    vx: number,
    vy: number,
    dx: number,
    dy: number
  ): number {
    const vMagSq = vx * vx + vy * vy;
    if (vMagSq < 0.001) return 999; // Essentially stationary

    // Time at which distance is minimized
    // d(t) = |p0 + v*t|
    // d'(t) = 0 when t = -(p0 · v) / |v|²
    const t = -(dx * vx + dy * vy) / vMagSq;
    return t;
  }

  /** Average position of threats */
  private averagePosition(threats: Threat[]): Vec2 {
    if (threats.length === 0) return { x: WORLD_WIDTH / 2, y: WORLD_HEIGHT / 2 };

    let x = 0;
    let y = 0;
    for (const t of threats) {
      x += t.x;
      y += t.y;
    }
    return { x: x / threats.length, y: y / threats.length };
  }
}
