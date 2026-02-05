import {
  ASTEROID_CAP,
  EXTRA_LIFE_SCORE_STEP,
  FIXED_TIMESTEP,
  HYPERSPACE_COOLDOWN,
  LURK_SAUCER_SPAWN_FAST,
  LURK_TIME_THRESHOLD,
  MAX_DEBRIS,
  MAX_FRAME_DELTA,
  MAX_PARTICLES,
  MAX_SUBSTEPS,
  SAUCER_BULLET_LIFETIME,
  SAUCER_BULLET_SPEED,
  SAUCER_SPAWN_MAX,
  SAUCER_SPAWN_MIN,
  SCORE_LARGE_ASTEROID,
  SCORE_LARGE_SAUCER,
  SCORE_MEDIUM_ASTEROID,
  SCORE_SMALL_ASTEROID,
  SCORE_SMALL_SAUCER,
  SHAKE_DECAY,
  SHAKE_INTENSITY_LARGE,
  SHAKE_INTENSITY_MEDIUM,
  SHAKE_INTENSITY_SMALL,
  SHIP_BULLET_COOLDOWN,
  SHIP_BULLET_LIFETIME,
  SHIP_BULLET_LIMIT,
  SHIP_BULLET_SPEED,
  SHIP_DRAG,
  SHIP_MAX_SPEED,
  SHIP_RADIUS,
  SHIP_RESPAWN_DELAY,
  SHIP_SPAWN_INVULNERABLE,
  SHIP_THRUST,
  SHIP_TURN_SPEED,
  STARTING_LIVES,
  STORAGE_HIGH_SCORE_KEY,
  WORLD_HEIGHT,
  WORLD_WIDTH,
} from "./constants";
import { AudioSystem } from "./AudioSystem";
import { InputController } from "./input";
import { angleToVector, clamp, randomInt, randomRange, wrapX, wrapY } from "./math";
import type { Asteroid, AsteroidSize, Bullet, Debris, GameMode, Particle, Saucer, Ship, Star } from "./types";

const ASTEROID_RADIUS_BY_SIZE: Record<AsteroidSize, number> = {
  large: 48,
  medium: 28,
  small: 16,
};

const ASTEROID_SPEED_RANGE_BY_SIZE: Record<AsteroidSize, [number, number]> = {
  large: [34, 58],
  medium: [62, 94],
  small: [98, 142],
};

const SAUCER_RADIUS_LARGE = 22;
const SAUCER_RADIUS_SMALL = 16;

function shortestDelta(from: number, to: number, size: number): number {
  let delta = to - from;

  if (delta > size / 2) {
    delta -= size;
  }

  if (delta < -size / 2) {
    delta += size;
  }

  return delta;
}

function collisionDistanceSquared(ax: number, ay: number, bx: number, by: number): number {
  const dx = shortestDelta(ax, bx, WORLD_WIDTH);
  const dy = shortestDelta(ay, by, WORLD_HEIGHT);
  return dx * dx + dy * dy;
}

// Linear interpolation with wrap-around handling
function lerpWrap(prev: number, curr: number, alpha: number, size: number): number {
  let delta = curr - prev;
  if (delta > size / 2) delta -= size;
  if (delta < -size / 2) delta += size;
  let result = prev + delta * alpha;
  if (result < 0) result += size;
  if (result >= size) result -= size;
  return result;
}

function lerpAngle(prev: number, curr: number, alpha: number): number {
  let delta = curr - prev;
  while (delta > Math.PI) delta -= Math.PI * 2;
  while (delta < -Math.PI) delta += Math.PI * 2;
  return prev + delta * alpha;
}

export class AsteroidsGame {
  private readonly canvas: HTMLCanvasElement;

  private readonly ctx: CanvasRenderingContext2D;

  private readonly input = new InputController();

  private readonly audio = new AudioSystem();

  private readonly stars: Star[] = [];

  private particles: Particle[] = [];

  private debris: Debris[] = [];

  private mode: GameMode = "menu";

  private score = 0;

  private highScore = 0;

  private lives = STARTING_LIVES;

  private wave = 0;

  private nextExtraLifeScore = EXTRA_LIFE_SCORE_STEP;

  private ship: Ship;

  private asteroids: Asteroid[] = [];

  private bullets: Bullet[] = [];

  private saucers: Saucer[] = [];

  private saucerBullets: Bullet[] = [];

  private saucerSpawnTimer = randomRange(SAUCER_SPAWN_MIN, SAUCER_SPAWN_MAX);

  private nextId = 1;

  private rafId: number | null = null;

  private lastTimeMs = 0;

  private accumulator = 0;

  private running = true;

  private pauseFromHidden = false;

  private cssWidth = WORLD_WIDTH;

  private cssHeight = WORLD_HEIGHT;

  private dpr = 1;

  private viewScale = 1;

  private viewOffsetX = 0;

  private viewOffsetY = 0;

  // Screen shake
  private shakeX = 0;
  private shakeY = 0;
  private shakeIntensity = 0;
  private shakeRotation = 0;

  // Thrust timing for audio
  private lastThrustTime = 0;

  // Anti-lurking: time since last asteroid destroyed by player
  private timeSinceLastKill = 0;

  // Game time for animations (star twinkle, etc.)
  private gameTime = 0;

  private readonly keyDownHandler = (event: KeyboardEvent): void => {
    this.input.handleKeyDown(event);
  };

  private readonly keyUpHandler = (event: KeyboardEvent): void => {
    this.input.handleKeyUp(event);
  };

  private readonly visibilityHandler = (): void => {
    if (document.hidden && this.mode === "playing") {
      this.mode = "paused";
      this.pauseFromHidden = true;
    } else if (!document.hidden && this.mode === "paused" && this.pauseFromHidden) {
      this.mode = "playing";
      this.pauseFromHidden = false;
      this.lastTimeMs = 0;
      this.accumulator = 0;
    }
  };

  private readonly pointerDownHandler = (): void => {
    if (this.mode === "menu" || this.mode === "game-over") {
      this.audio.enable();
      this.startNewGame();
    } else if (this.mode === "paused" && !this.pauseFromHidden) {
      this.mode = "playing";
    }
  };

  private readonly resizeHandler = (): void => {
    this.resize();
  };

  private readonly frameHandler = (timestampMs: number): void => {
    if (!this.running) {
      return;
    }

    this.updateFrame(timestampMs);
    this.rafId = window.requestAnimationFrame(this.frameHandler);
  };

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("aria-label", "Asteroids game canvas");

    const ctx = this.canvas.getContext("2d", { alpha: false });

    if (!ctx) {
      throw new Error("Unable to create 2D context.");
    }

    this.ctx = ctx;
    this.ship = this.createShip();

    this.loadHighScore();
    this.seedStars(120);
    this.attachEvents();
    this.resize();

    this.rafId = window.requestAnimationFrame(this.frameHandler);
  }

  dispose(): void {
    this.running = false;

    if (this.rafId !== null) {
      window.cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }

    this.detachEvents();
  }

  private attachEvents(): void {
    window.addEventListener("keydown", this.keyDownHandler, { passive: false });
    window.addEventListener("keyup", this.keyUpHandler, { passive: false });
    window.addEventListener("resize", this.resizeHandler);
    document.addEventListener("visibilitychange", this.visibilityHandler);
    this.canvas.addEventListener("pointerdown", this.pointerDownHandler);
  }

  private detachEvents(): void {
    window.removeEventListener("keydown", this.keyDownHandler);
    window.removeEventListener("keyup", this.keyUpHandler);
    window.removeEventListener("resize", this.resizeHandler);
    document.removeEventListener("visibilitychange", this.visibilityHandler);
    this.canvas.removeEventListener("pointerdown", this.pointerDownHandler);
  }

  private resize(): void {
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(320, rect.width || WORLD_WIDTH);
    const height = Math.max(320, rect.height || WORLD_HEIGHT);
    const dpr = window.devicePixelRatio || 1;
    this.dpr = dpr;

    this.cssWidth = width;
    this.cssHeight = height;

    this.canvas.width = Math.floor(width * dpr);
    this.canvas.height = Math.floor(height * dpr);

    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.ctx.imageSmoothingEnabled = false;

    this.viewScale = Math.min(width / WORLD_WIDTH, height / WORLD_HEIGHT);
    this.viewOffsetX = (width - WORLD_WIDTH * this.viewScale) * 0.5;
    this.viewOffsetY = (height - WORLD_HEIGHT * this.viewScale) * 0.5;
  }

  private updateFrame(timestampMs: number): void {
    this.handleGlobalInput();

    if (this.mode === "playing") {
      if (this.lastTimeMs === 0) {
        this.lastTimeMs = timestampMs;
      }

      let deltaSeconds = (timestampMs - this.lastTimeMs) / 1000;
      this.lastTimeMs = timestampMs;
      deltaSeconds = Math.min(MAX_FRAME_DELTA, Math.max(0, deltaSeconds));
      this.accumulator += deltaSeconds;

      let steps = 0;

      while (this.accumulator >= FIXED_TIMESTEP && steps < MAX_SUBSTEPS) {
        // Store previous positions for interpolation before update
        this.storePreviousPositions();
        this.updateSimulation(FIXED_TIMESTEP);
        this.accumulator -= FIXED_TIMESTEP;
        steps += 1;
      }

      if (steps === MAX_SUBSTEPS) {
        this.accumulator = 0;
      }

      // Update game time for animations
      this.gameTime += deltaSeconds;
    } else {
      this.lastTimeMs = 0;
      this.accumulator = 0;
      // Still update game time for menu animations
      this.gameTime += 1 / 60;
    }

    const alpha = this.accumulator / FIXED_TIMESTEP;
    this.render(alpha);
    this.input.clearPressed();
  }

  private handleGlobalInput(): void {
    if (this.input.consumePress("Enter")) {
      if (this.mode === "menu" || this.mode === "game-over") {
        this.audio.enable();
        this.startNewGame();
      } else if (this.mode === "paused") {
        this.mode = "playing";
        this.pauseFromHidden = false;
      }
    }

    if (this.input.consumePress("KeyP")) {
      if (this.mode === "playing") {
        this.mode = "paused";
        this.pauseFromHidden = false;
      } else if (this.mode === "paused" && !this.pauseFromHidden) {
        this.mode = "playing";
      }
    }

    if (this.input.consumePress("KeyR") && this.mode !== "menu") {
      this.startNewGame();
    }
  }

  private startNewGame(): void {
    this.mode = "playing";
    this.score = 0;
    this.lives = STARTING_LIVES;
    this.wave = 0;
    this.nextExtraLifeScore = EXTRA_LIFE_SCORE_STEP;
    this.asteroids = [];
    this.bullets = [];
    this.saucers = [];
    this.saucerBullets = [];
    this.particles = [];
    this.debris = [];
    this.saucerSpawnTimer = randomRange(SAUCER_SPAWN_MIN, SAUCER_SPAWN_MAX);
    this.timeSinceLastKill = 0;
    this.ship = this.createShip();
    this.shakeIntensity = 0;
    this.spawnWave();
  }

  private storePreviousPositions(): void {
    const ship = this.ship;
    ship.prevX = ship.x;
    ship.prevY = ship.y;
    ship.prevAngle = ship.angle;

    for (const asteroid of this.asteroids) {
      asteroid.prevX = asteroid.x;
      asteroid.prevY = asteroid.y;
      asteroid.prevAngle = asteroid.angle;
    }

    for (const bullet of this.bullets) {
      bullet.prevX = bullet.x;
      bullet.prevY = bullet.y;
      bullet.prevAngle = bullet.angle;
    }

    for (const bullet of this.saucerBullets) {
      bullet.prevX = bullet.x;
      bullet.prevY = bullet.y;
      bullet.prevAngle = bullet.angle;
    }

    for (const saucer of this.saucers) {
      saucer.prevX = saucer.x;
      saucer.prevY = saucer.y;
      saucer.prevAngle = saucer.angle;
    }
  }

  private updateSimulation(dt: number): void {
    this.updateShip(dt);
    this.updateAsteroids(dt);
    this.updateBullets(dt);
    this.updateSaucers(dt);
    this.updateSaucerBullets(dt);
    this.updateParticles(dt);
    this.updateDebris(dt);
    this.updateScreenShake();
    this.handleCollisions();
    this.pruneDestroyedEntities();

    // Anti-lurking timer
    this.timeSinceLastKill += dt;

    if (this.mode === "playing" && this.asteroids.length === 0 && this.saucers.length === 0) {
      this.spawnWave();
    }
  }

  private createShip(): Ship {
    const x = WORLD_WIDTH * 0.5;
    const y = WORLD_HEIGHT * 0.5;
    const angle = -Math.PI * 0.5;
    return {
      id: this.nextId++,
      x,
      y,
      vx: 0,
      vy: 0,
      angle,
      alive: true,
      radius: SHIP_RADIUS,
      prevX: x,
      prevY: y,
      prevAngle: angle,
      canControl: true,
      fireCooldown: 0,
      respawnTimer: 0,
      invulnerableTimer: SHIP_SPAWN_INVULNERABLE,
      hyperspaceCooldown: 0,
    };
  }

  private spawnWave(): void {
    this.wave += 1;
    this.timeSinceLastKill = 0;

    const largeCount = Math.min(11, 4 + (this.wave - 1) * 2);

    for (let i = 0; i < largeCount; i += 1) {
      let x = randomRange(0, WORLD_WIDTH);
      let y = randomRange(0, WORLD_HEIGHT);

      let guard = 0;

      while (
        collisionDistanceSquared(x, y, WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.5) < 180 * 180 &&
        guard < 20
      ) {
        x = randomRange(0, WORLD_WIDTH);
        y = randomRange(0, WORLD_HEIGHT);
        guard += 1;
      }

      this.asteroids.push(this.createAsteroid("large", x, y));
    }
  }

  private createAsteroid(size: AsteroidSize, x: number, y: number): Asteroid {
    const [minSpeed, maxSpeed] = ASTEROID_SPEED_RANGE_BY_SIZE[size];
    const moveAngle = randomRange(0, Math.PI * 2);
    const direction = angleToVector(moveAngle);
    const speed = randomRange(minSpeed, maxSpeed);
    const vertices = this.createAsteroidVertices();
    const startAngle = randomRange(0, Math.PI * 2);

    return {
      id: this.nextId++,
      x,
      y,
      vx: direction.x * speed,
      vy: direction.y * speed,
      angle: startAngle,
      alive: true,
      radius: ASTEROID_RADIUS_BY_SIZE[size],
      prevX: x,
      prevY: y,
      prevAngle: startAngle,
      size,
      spin: randomRange(-0.7, 0.7),
      vertices,
    };
  }

  private createAsteroidVertices(): number[] {
    const vertexCount = randomInt(9, 14);
    const vertices: number[] = [];

    for (let i = 0; i < vertexCount; i += 1) {
      vertices.push(randomRange(0.72, 1.2));
    }

    return vertices;
  }

  private updateShip(dt: number): void {
    const ship = this.ship;

    ship.fireCooldown = Math.max(0, ship.fireCooldown - dt);
    ship.hyperspaceCooldown = Math.max(0, ship.hyperspaceCooldown - dt);

    if (!ship.canControl) {
      ship.respawnTimer -= dt;

      if (ship.respawnTimer <= 0) {
        const clearRadius = 120;
        const spawnX = WORLD_WIDTH * 0.5;
        const spawnY = WORLD_HEIGHT * 0.5;

        // Check asteroids
        const blockedByAsteroid = this.asteroids.some(
          (asteroid) =>
            collisionDistanceSquared(asteroid.x, asteroid.y, spawnX, spawnY) <
            (asteroid.radius + clearRadius) * (asteroid.radius + clearRadius),
        );

        // Check saucers
        const blockedBySaucer = this.saucers.some(
          (saucer) =>
            saucer.alive &&
            collisionDistanceSquared(saucer.x, saucer.y, spawnX, spawnY) <
            (saucer.radius + clearRadius) * (saucer.radius + clearRadius),
        );

        // Check saucer bullets
        const blockedByBullet = this.saucerBullets.some(
          (bullet) =>
            bullet.alive &&
            collisionDistanceSquared(bullet.x, bullet.y, spawnX, spawnY) <
            (bullet.radius + clearRadius) * (bullet.radius + clearRadius),
        );

        if (!blockedByAsteroid && !blockedBySaucer && !blockedByBullet) {
          ship.x = spawnX;
          ship.y = spawnY;
          ship.prevX = spawnX;
          ship.prevY = spawnY;
          ship.vx = 0;
          ship.vy = 0;
          ship.angle = -Math.PI * 0.5;
          ship.prevAngle = ship.angle;
          ship.canControl = true;
          ship.invulnerableTimer = SHIP_SPAWN_INVULNERABLE;
        }
      }

      return;
    }

    ship.invulnerableTimer = Math.max(0, ship.invulnerableTimer - dt);

    if (this.input.isDown("ArrowLeft")) {
      ship.angle -= SHIP_TURN_SPEED * dt;
    }

    if (this.input.isDown("ArrowRight")) {
      ship.angle += SHIP_TURN_SPEED * dt;
    }

    if (this.input.isDown("ArrowUp")) {
      const direction = angleToVector(ship.angle);
      ship.vx += direction.x * SHIP_THRUST * dt;
      ship.vy += direction.y * SHIP_THRUST * dt;

      // Thrust particles and sound
      const now = Date.now();
      if (now - this.lastThrustTime > 80) {
        this.spawnThrustParticles(ship);
        this.audio.playThrust();
        this.lastThrustTime = now;
      }
    }

    const dragFactor = Math.pow(SHIP_DRAG, dt * 60);
    ship.vx *= dragFactor;
    ship.vy *= dragFactor;

    const speed = Math.hypot(ship.vx, ship.vy);

    if (speed > SHIP_MAX_SPEED) {
      const scale = SHIP_MAX_SPEED / speed;
      ship.vx *= scale;
      ship.vy *= scale;
    }

    if (
      this.input.isDown("Space") &&
      ship.fireCooldown <= 0 &&
      this.bullets.length < SHIP_BULLET_LIMIT
    ) {
      this.spawnShipBullet();
      ship.fireCooldown = SHIP_BULLET_COOLDOWN;
    }

    if (this.input.consumePress("ShiftLeft") && ship.hyperspaceCooldown <= 0) {
      this.useHyperspace();
    }

    ship.x = wrapX(ship.x + ship.vx * dt);
    ship.y = wrapY(ship.y + ship.vy * dt);
  }

  private spawnShipBullet(): void {
    const ship = this.ship;
    const direction = angleToVector(ship.angle);
    const bulletSpeed = SHIP_BULLET_SPEED + Math.hypot(ship.vx, ship.vy) * 0.35;

    const startX = ship.x + direction.x * (ship.radius + 6);
    const startY = ship.y + direction.y * (ship.radius + 6);

    const bullet: Bullet = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx: ship.vx + direction.x * bulletSpeed,
      vy: ship.vy + direction.y * bulletSpeed,
      angle: ship.angle,
      alive: true,
      radius: 2,
      prevX: startX,
      prevY: startY,
      prevAngle: ship.angle,
      life: SHIP_BULLET_LIFETIME,
      fromSaucer: false,
    };

    this.bullets.push(bullet);
    this.audio.playShoot();
    this.spawnMuzzleFlash(ship.x + direction.x * (ship.radius + 8), ship.y + direction.y * (ship.radius + 8));
  }

  private useHyperspace(): void {
    const ship = this.ship;
    ship.hyperspaceCooldown = HYPERSPACE_COOLDOWN;

    this.spawnHyperspaceEffect(ship.x, ship.y);
    this.audio.playHyperspace();

    const crowdedness = clamp(this.asteroids.length / 18, 0, 1);
    const failChance = 0.12 + crowdedness * 0.2;

    if (Math.random() < failChance) {
      this.destroyShip();
      return;
    }

    ship.x = randomRange(40, WORLD_WIDTH - 40);
    ship.y = randomRange(40, WORLD_HEIGHT - 40);
    ship.prevX = ship.x;
    ship.prevY = ship.y;
    ship.vx = 0;
    ship.vy = 0;
    ship.invulnerableTimer = Math.max(ship.invulnerableTimer, 0.8);

    this.spawnHyperspaceEffect(ship.x, ship.y);
  }

  private updateAsteroids(dt: number): void {
    for (const asteroid of this.asteroids) {
      if (!asteroid.alive) {
        continue;
      }

      asteroid.x = wrapX(asteroid.x + asteroid.vx * dt);
      asteroid.y = wrapY(asteroid.y + asteroid.vy * dt);
      asteroid.angle += asteroid.spin * dt;
    }
  }

  private updateBullets(dt: number): void {
    for (const bullet of this.bullets) {
      if (!bullet.alive) {
        continue;
      }

      bullet.life -= dt;

      if (bullet.life <= 0) {
        bullet.alive = false;
        continue;
      }

      bullet.x = wrapX(bullet.x + bullet.vx * dt);
      bullet.y = wrapY(bullet.y + bullet.vy * dt);
    }
  }

  private updateSaucerBullets(dt: number): void {
    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) {
        continue;
      }

      bullet.life -= dt;

      if (bullet.life <= 0) {
        bullet.alive = false;
        continue;
      }

      bullet.x = wrapX(bullet.x + bullet.vx * dt);
      bullet.y = wrapY(bullet.y + bullet.vy * dt);
    }
  }

  private updateSaucers(dt: number): void {
    this.saucerSpawnTimer -= dt;

    // Anti-lurking: spawn saucers faster if player isn't killing asteroids
    const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD;
    const spawnThreshold = isLurking ? LURK_SAUCER_SPAWN_FAST : 0;

    if (this.saucers.length === 0 && this.saucerSpawnTimer <= spawnThreshold) {
      this.spawnSaucer();
      // Spawn faster when lurking
      this.saucerSpawnTimer = isLurking
        ? randomRange(LURK_SAUCER_SPAWN_FAST, LURK_SAUCER_SPAWN_FAST + 2)
        : randomRange(SAUCER_SPAWN_MIN, SAUCER_SPAWN_MAX);
    }

    for (const saucer of this.saucers) {
      if (!saucer.alive) {
        continue;
      }

      saucer.x += saucer.vx * dt;
      saucer.y = wrapY(saucer.y + saucer.vy * dt);

      if (saucer.x < -80 || saucer.x > WORLD_WIDTH + 80) {
        saucer.alive = false;
        continue;
      }

      saucer.driftTimer -= dt;
      if (saucer.driftTimer <= 0) {
        saucer.driftTimer = randomRange(0.8, 2);
        saucer.vy = randomRange(-38, 38);
      }

      saucer.fireCooldown -= dt;

      if (saucer.fireCooldown <= 0) {
        this.spawnSaucerBullet(saucer);
        // Fire faster when lurking
        const fireSpeedMult = isLurking ? 0.7 : 1;
        saucer.fireCooldown = saucer.small
          ? randomRange(0.65, 1.1) * fireSpeedMult
          : randomRange(1.1, 1.6) * fireSpeedMult;
      }
    }
  }

  private spawnSaucer(): void {
    const enterFromLeft = Math.random() > 0.5;
    // Anti-lurking: more likely to spawn small (deadly) saucer when lurking
    const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD;
    const smallChance = isLurking ? 0.9 : this.score > 4000 ? 0.7 : 0.22;
    const small = Math.random() < smallChance;
    const speed = small ? 95 : 70;

    const startX = enterFromLeft ? -30 : WORLD_WIDTH + 30;
    const startY = randomRange(72, WORLD_HEIGHT - 72);

    const saucer: Saucer = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx: enterFromLeft ? speed : -speed,
      vy: randomRange(-22, 22),
      angle: 0,
      alive: true,
      radius: small ? SAUCER_RADIUS_SMALL : SAUCER_RADIUS_LARGE,
      prevX: startX,
      prevY: startY,
      prevAngle: 0,
      small,
      fireCooldown: randomRange(0.3, 0.8),
      driftTimer: randomRange(0.8, 2),
    };

    this.saucers.push(saucer);
    this.audio.playSaucer(small);
  }

  private spawnSaucerBullet(saucer: Saucer): void {
    let shotAngle: number;

    if (saucer.small) {
      const dx = shortestDelta(saucer.x, this.ship.x, WORLD_WIDTH);
      const dy = shortestDelta(saucer.y, this.ship.y, WORLD_HEIGHT);
      const targetAngle = Math.atan2(dy, dx);
      // More accurate when lurking (anti-lurk mechanic)
      const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD;
      const baseError = isLurking ? 15 : 30;
      const errorDegrees = clamp(baseError - this.score / 2500, 4, baseError);
      const errorRadians = (errorDegrees * Math.PI) / 180;
      shotAngle = targetAngle + randomRange(-errorRadians, errorRadians);
    } else {
      shotAngle = randomRange(0, Math.PI * 2);
    }

    const direction = angleToVector(shotAngle);

    const startX = wrapX(saucer.x + direction.x * (saucer.radius + 4));
    const startY = wrapY(saucer.y + direction.y * (saucer.radius + 4));

    const bullet: Bullet = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx: direction.x * SAUCER_BULLET_SPEED,
      vy: direction.y * SAUCER_BULLET_SPEED,
      angle: shotAngle,
      alive: true,
      radius: 2.3,
      prevX: startX,
      prevY: startY,
      prevAngle: shotAngle,
      life: SAUCER_BULLET_LIFETIME,
      fromSaucer: true,
    };

    this.saucerBullets.push(bullet);
  }

  private handleCollisions(): void {
    for (const bullet of this.bullets) {
      if (!bullet.alive) {
        continue;
      }

      for (const asteroid of this.asteroids) {
        if (!asteroid.alive) {
          continue;
        }

        const hitDistance = bullet.radius + asteroid.radius;

        if (
          collisionDistanceSquared(bullet.x, bullet.y, asteroid.x, asteroid.y) <=
          hitDistance * hitDistance
        ) {
          bullet.alive = false;
          this.destroyAsteroid(asteroid, true);
          break;
        }
      }
    }

    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) {
        continue;
      }

      for (const asteroid of this.asteroids) {
        if (!asteroid.alive) {
          continue;
        }

        const hitDistance = bullet.radius + asteroid.radius;

        if (
          collisionDistanceSquared(bullet.x, bullet.y, asteroid.x, asteroid.y) <=
          hitDistance * hitDistance
        ) {
          bullet.alive = false;
          this.destroyAsteroid(asteroid, false);
          break;
        }
      }
    }

    for (const bullet of this.bullets) {
      if (!bullet.alive) {
        continue;
      }

      for (const saucer of this.saucers) {
        if (!saucer.alive) {
          continue;
        }

        const hitDistance = bullet.radius + saucer.radius;

        if (
          collisionDistanceSquared(bullet.x, bullet.y, saucer.x, saucer.y) <=
          hitDistance * hitDistance
        ) {
          bullet.alive = false;
          saucer.alive = false;
          this.addScore(saucer.small ? SCORE_SMALL_SAUCER : SCORE_LARGE_SAUCER);
          this.spawnExplosion(saucer.x, saucer.y, saucer.small ? "medium" : "large");
          this.addScreenShake(saucer.small ? SHAKE_INTENSITY_MEDIUM : SHAKE_INTENSITY_LARGE);
          this.audio.playExplosion(saucer.small ? "medium" : "large");
          break;
        }
      }
    }

    if (!this.ship.canControl || this.ship.invulnerableTimer > 0) {
      return;
    }

    const ship = this.ship;

    for (const asteroid of this.asteroids) {
      if (!asteroid.alive) {
        continue;
      }

      const collisionDistance = ship.radius + asteroid.radius * 0.88;

      if (
        collisionDistanceSquared(ship.x, ship.y, asteroid.x, asteroid.y) <=
        collisionDistance * collisionDistance
      ) {
        this.destroyShip();
        return;
      }
    }

    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) {
        continue;
      }

      const collisionDistance = ship.radius + bullet.radius;

      if (
        collisionDistanceSquared(ship.x, ship.y, bullet.x, bullet.y) <=
        collisionDistance * collisionDistance
      ) {
        bullet.alive = false;
        this.destroyShip();
        return;
      }
    }

    for (const saucer of this.saucers) {
      if (!saucer.alive) {
        continue;
      }

      const collisionDistance = ship.radius + saucer.radius;

      if (
        collisionDistanceSquared(ship.x, ship.y, saucer.x, saucer.y) <=
        collisionDistance * collisionDistance
      ) {
        saucer.alive = false;
        this.destroyShip();
        return;
      }
    }
  }

  private destroyAsteroid(asteroid: Asteroid, awardScore: boolean): void {
    asteroid.alive = false;

    if (awardScore) {
      // Reset anti-lurking timer when player destroys asteroid
      this.timeSinceLastKill = 0;

      if (asteroid.size === "large") {
        this.addScore(SCORE_LARGE_ASTEROID);
      } else if (asteroid.size === "medium") {
        this.addScore(SCORE_MEDIUM_ASTEROID);
      } else {
        this.addScore(SCORE_SMALL_ASTEROID);
      }
    }

    // Spawn explosion and debris
    this.spawnExplosion(asteroid.x, asteroid.y, asteroid.size);
    this.spawnDebris(asteroid.x, asteroid.y, asteroid.size);
    this.addScreenShake(
      asteroid.size === "large" ? SHAKE_INTENSITY_MEDIUM :
      asteroid.size === "medium" ? SHAKE_INTENSITY_SMALL : SHAKE_INTENSITY_SMALL * 0.5
    );
    this.audio.playExplosion(asteroid.size);

    if (asteroid.size === "small") {
      return;
    }

    const childSize: AsteroidSize = asteroid.size === "large" ? "medium" : "small";
    const totalObjects = this.asteroids.filter((entry) => entry.alive).length;
    const splitCount = totalObjects >= ASTEROID_CAP ? 1 : 2;

    for (let i = 0; i < splitCount; i += 1) {
      const child = this.createAsteroid(childSize, asteroid.x, asteroid.y);
      child.vx += asteroid.vx * 0.18;
      child.vy += asteroid.vy * 0.18;
      this.asteroids.push(child);
    }
  }

  private destroyShip(): void {
    this.ship.canControl = false;
    this.ship.respawnTimer = SHIP_RESPAWN_DELAY;
    this.ship.vx = 0;
    this.ship.vy = 0;
    this.ship.invulnerableTimer = 0;
    this.lives -= 1;

    // Big explosion effect
    this.spawnExplosion(this.ship.x, this.ship.y, "large");
    this.spawnDebris(this.ship.x, this.ship.y, "large");
    this.addScreenShake(SHAKE_INTENSITY_LARGE);
    this.audio.playExplosion("large");

    if (this.lives <= 0) {
      this.mode = "game-over";
      this.ship.canControl = false;
      this.ship.respawnTimer = 999;
      this.saveHighScore();
    }
  }

  private addScore(points: number): void {
    this.score += points;

    while (this.score >= this.nextExtraLifeScore) {
      this.lives += 1;
      this.nextExtraLifeScore += EXTRA_LIFE_SCORE_STEP;
      this.audio.playExtraLife();
      // Extra life celebration effect
      for (let i = 0; i < 20; i++) {
        this.spawnParticle(
          WORLD_WIDTH * 0.5 + randomRange(-100, 100),
          WORLD_HEIGHT * 0.5 + randomRange(-50, 50),
          "spark",
          "#ffd700"
        );
      }
    }

    if (this.score > this.highScore) {
      this.highScore = this.score;
    }
  }

  private pruneDestroyedEntities(): void {
    this.asteroids = this.asteroids.filter((entity) => entity.alive);
    this.bullets = this.bullets.filter((entity) => entity.alive);
    this.saucers = this.saucers.filter((entity) => entity.alive);
    this.saucerBullets = this.saucerBullets.filter((entity) => entity.alive);
    this.particles = this.particles.filter((p) => p.life > 0).slice(-MAX_PARTICLES);
    this.debris = this.debris.filter((d) => d.life > 0).slice(-MAX_DEBRIS);
  }

  // Particle system
  private spawnParticle(x: number, y: number, type: Particle["type"], color: string, count = 1): void {
    for (let i = 0; i < count; i++) {
      if (this.particles.length >= MAX_PARTICLES) break;

      const angle = randomRange(0, Math.PI * 2);
      const speed = randomRange(20, 120);
      const life = randomRange(0.3, 0.8);

      this.particles.push({
        id: this.nextId++,
        x: x + randomRange(-5, 5),
        y: y + randomRange(-5, 5),
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        life,
        maxLife: life,
        size: randomRange(1, 3),
        color,
        alpha: 1,
        decay: randomRange(0.8, 1.2),
        type,
      });
    }
  }

  private spawnExplosion(x: number, y: number, size: "small" | "medium" | "large"): void {
    const particleCount = size === "large" ? 25 : size === "medium" ? 15 : 8;
    const colors = ["#ff6b35", "#f7931e", "#ffd700", "#ffffff"];

    for (let i = 0; i < particleCount; i++) {
      this.spawnParticle(x, y, "spark", colors[randomInt(0, colors.length)]);
    }

    // Add smoke particles
    this.spawnParticle(x, y, "smoke", "#555555", 5);
  }

  private spawnMuzzleFlash(x: number, y: number): void {
    this.spawnParticle(x, y, "glow", "#a8ff60", 3);
  }

  private spawnThrustParticles(ship: Ship): void {
    const direction = angleToVector(ship.angle + Math.PI);
    const x = ship.x + direction.x * ship.radius;
    const y = ship.y + direction.y * ship.radius;

    this.spawnParticle(x, y, "spark", "#ffaa44", 2);
    this.spawnParticle(x, y, "smoke", "#666666", 1);
  }

  private spawnHyperspaceEffect(x: number, y: number): void {
    for (let i = 0; i < 30; i++) {
      this.spawnParticle(x, y, "glow", "#44aaff", 1);
    }
  }

  private spawnDebris(x: number, y: number, size: AsteroidSize | "large"): void {
    const debrisCount = size === "large" ? 8 : size === "medium" ? 5 : 3;

    for (let i = 0; i < debrisCount; i++) {
      if (this.debris.length >= MAX_DEBRIS) break;

      const angle = randomRange(0, Math.PI * 2);
      const speed = randomRange(30, 90);
      const life = randomRange(0.5, 1.2);
      const vertices: number[] = [];
      const vertexCount = randomInt(4, 7);

      for (let j = 0; j < vertexCount; j++) {
        vertices.push(randomRange(0.5, 1));
      }

      this.debris.push({
        id: this.nextId++,
        x: x + randomRange(-10, 10),
        y: y + randomRange(-10, 10),
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        angle: randomRange(0, Math.PI * 2),
        spin: randomRange(-2, 2),
        life,
        maxLife: life,
        size: randomRange(3, 8),
        vertices,
      });
    }
  }

  private updateParticles(dt: number): void {
    for (const particle of this.particles) {
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      particle.vx *= 0.98;
      particle.vy *= 0.98;
      particle.life -= dt * particle.decay;
      particle.alpha = particle.life / particle.maxLife;
    }
  }

  private updateDebris(dt: number): void {
    for (const d of this.debris) {
      d.x = wrapX(d.x + d.vx * dt);
      d.y = wrapY(d.y + d.vy * dt);
      d.angle += d.spin * dt;
      d.life -= dt;
    }
  }

  // Screen shake
  private addScreenShake(intensity: number): void {
    this.shakeIntensity = Math.max(this.shakeIntensity, intensity);
  }

  private updateScreenShake(): void {
    if (this.shakeIntensity > 0.1) {
      this.shakeX = (Math.random() - 0.5) * this.shakeIntensity * 2;
      this.shakeY = (Math.random() - 0.5) * this.shakeIntensity * 2;
      this.shakeRotation = (Math.random() - 0.5) * this.shakeIntensity * 0.02;
      this.shakeIntensity *= SHAKE_DECAY;
    } else {
      this.shakeX = 0;
      this.shakeY = 0;
      this.shakeRotation = 0;
      this.shakeIntensity = 0;
    }
  }

  private seedStars(count: number): void {
    this.stars.length = 0;

    for (let i = 0; i < count; i += 1) {
      const baseAlpha = randomRange(0.2, 0.95);
      this.stars.push({
        x: randomRange(0, WORLD_WIDTH),
        y: randomRange(0, WORLD_HEIGHT),
        alpha: baseAlpha,
        baseAlpha,
        twinkleSpeed: randomRange(0.8, 2.5),
        twinklePhase: randomRange(0, Math.PI * 2),
      });
    }
  }

  private render(alpha: number): void {
    const ctx = this.ctx;

    ctx.save();
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.cssWidth, this.cssHeight);

    // Deep space background
    const gradient = ctx.createRadialGradient(
      this.cssWidth / 2, this.cssHeight / 2, 0,
      this.cssWidth / 2, this.cssHeight / 2, Math.max(this.cssWidth, this.cssHeight)
    );
    gradient.addColorStop(0, "#0a0f1a");
    gradient.addColorStop(1, "#020408");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, this.cssWidth, this.cssHeight);

    ctx.translate(this.viewOffsetX, this.viewOffsetY);
    ctx.scale(this.viewScale, this.viewScale);

    // Apply screen shake
    ctx.translate(this.shakeX, this.shakeY);
    ctx.rotate(this.shakeRotation);

    this.drawStars(ctx);

    // Set up glow effect
    ctx.shadowBlur = 8;
    ctx.shadowColor = "#4ade80";
    ctx.strokeStyle = "#b8ffe3";
    ctx.fillStyle = "#b8ffe3";
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";

    this.drawDebris(ctx);
    this.drawAsteroids(ctx, alpha);
    this.drawShip(ctx, alpha);
    this.drawBullets(ctx, this.bullets, alpha);
    this.drawSaucers(ctx, alpha);
    this.drawBullets(ctx, this.saucerBullets, alpha);
    this.drawParticles(ctx);
    this.drawHud(ctx);

    // Reset shadow for HUD and overlay
    ctx.shadowBlur = 0;

    this.drawOverlay(ctx, alpha);
    ctx.restore();
  }

  private drawStars(ctx: CanvasRenderingContext2D): void {
    for (const star of this.stars) {
      // Twinkling effect
      const twinkle = Math.sin(this.gameTime * star.twinkleSpeed + star.twinklePhase);
      const alpha = star.baseAlpha * (0.6 + twinkle * 0.4);
      ctx.globalAlpha = clamp(alpha, 0.1, 1);
      ctx.fillStyle = "#9fd4ff";
      ctx.fillRect(star.x, star.y, 1.4, 1.4);
    }

    ctx.globalAlpha = 1;
  }

  private drawShip(ctx: CanvasRenderingContext2D, alpha: number): void {
    const ship = this.ship;

    if (!ship.canControl && this.mode === "game-over") {
      return;
    }

    if (ship.invulnerableTimer > 0 && Math.floor(ship.invulnerableTimer * 18) % 2 === 0) {
      return;
    }

    // Interpolate position for smooth rendering
    const renderX = lerpWrap(ship.prevX, ship.x, alpha, WORLD_WIDTH);
    const renderY = lerpWrap(ship.prevY, ship.y, alpha, WORLD_HEIGHT);
    const renderAngle = lerpAngle(ship.prevAngle, ship.angle, alpha);

    ctx.save();
    ctx.translate(renderX, renderY);
    ctx.rotate(renderAngle + Math.PI * 0.5);

    // Ship glow
    ctx.shadowBlur = 15;
    ctx.shadowColor = "#4ade80";

    ctx.beginPath();
    ctx.moveTo(0, -ship.radius);
    ctx.lineTo(ship.radius * 0.72, ship.radius);
    ctx.lineTo(0, ship.radius * 0.45);
    ctx.lineTo(-ship.radius * 0.72, ship.radius);
    ctx.closePath();
    ctx.stroke();

    // Thrust flame with glow
    if (ship.canControl && this.input.isDown("ArrowUp") && this.mode === "playing") {
      ctx.shadowBlur = 20;
      ctx.shadowColor = "#ff6b35";
      ctx.strokeStyle = "#ffaa44";
      ctx.beginPath();
      const flame = 8 + Math.sin(Date.now() * 0.03) * 4;
      ctx.moveTo(-4, ship.radius * 0.9);
      ctx.lineTo(0, ship.radius + flame);
      ctx.lineTo(4, ship.radius * 0.9);
      ctx.stroke();
      ctx.strokeStyle = "#b8ffe3";
      ctx.shadowColor = "#4ade80";
    }

    ctx.restore();
  }

  private drawAsteroids(ctx: CanvasRenderingContext2D, alpha: number): void {
    ctx.shadowBlur = 10;
    ctx.shadowColor = "#6b7280";

    for (const asteroid of this.asteroids) {
      if (!asteroid.alive) {
        continue;
      }

      const vertices = asteroid.vertices;
      const vertexCount = vertices.length;

      // Interpolate position for smooth rendering
      const renderX = lerpWrap(asteroid.prevX, asteroid.x, alpha, WORLD_WIDTH);
      const renderY = lerpWrap(asteroid.prevY, asteroid.y, alpha, WORLD_HEIGHT);
      const renderAngle = lerpAngle(asteroid.prevAngle, asteroid.angle, alpha);

      ctx.save();
      ctx.translate(renderX, renderY);
      ctx.rotate(renderAngle);
      ctx.beginPath();

      for (let i = 0; i < vertexCount; i += 1) {
        const angle = (i / vertexCount) * Math.PI * 2;
        const radius = asteroid.radius * vertices[i];
        const x = Math.cos(angle) * radius;
        const y = Math.sin(angle) * radius;

        if (i === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }

      ctx.closePath();
      ctx.stroke();
      ctx.restore();
    }

    ctx.shadowColor = "#4ade80";
    ctx.shadowBlur = 8;
  }

  private drawBullets(ctx: CanvasRenderingContext2D, bullets: Bullet[], alpha: number): void {
    ctx.shadowBlur = 12;
    ctx.shadowColor = "#fbbf24";

    for (const bullet of bullets) {
      if (!bullet.alive) {
        continue;
      }

      // Interpolate position for smooth rendering
      const renderX = lerpWrap(bullet.prevX, bullet.x, alpha, WORLD_WIDTH);
      const renderY = lerpWrap(bullet.prevY, bullet.y, alpha, WORLD_HEIGHT);

      ctx.fillStyle = "#fef3c7";
      ctx.fillRect(renderX - 1.2, renderY - 1.2, 2.4, 2.4);
    }

    ctx.shadowColor = "#4ade80";
    ctx.shadowBlur = 8;
  }

  private drawSaucers(ctx: CanvasRenderingContext2D, alpha: number): void {
    for (const saucer of this.saucers) {
      if (!saucer.alive) {
        continue;
      }

      const w = saucer.small ? 22 : 30;
      const h = saucer.small ? 9 : 12;

      // Interpolate position for smooth rendering
      const renderX = lerpWrap(saucer.prevX, saucer.x, alpha, WORLD_WIDTH);
      const renderY = lerpWrap(saucer.prevY, saucer.y, alpha, WORLD_HEIGHT);

      ctx.save();
      ctx.translate(renderX, renderY);

      // Saucer glow - different colors for small vs large
      ctx.shadowBlur = 15;
      ctx.shadowColor = saucer.small ? "#ef4444" : "#f97316";

      ctx.beginPath();
      ctx.ellipse(0, 0, w * 0.6, h * 0.45, 0, 0, Math.PI * 2);
      ctx.stroke();

      ctx.beginPath();
      ctx.moveTo(-w * 0.5, 0);
      ctx.lineTo(-w * 0.28, -h * 0.55);
      ctx.lineTo(w * 0.28, -h * 0.55);
      ctx.lineTo(w * 0.5, 0);
      ctx.stroke();
      ctx.restore();
    }

    ctx.shadowColor = "#4ade80";
    ctx.shadowBlur = 8;
  }

  private drawParticles(ctx: CanvasRenderingContext2D): void {
    for (const particle of this.particles) {
      if (particle.life <= 0) continue;

      ctx.globalAlpha = particle.alpha;
      ctx.fillStyle = particle.color;

      if (particle.type === "glow") {
        ctx.shadowBlur = particle.size * 3;
        ctx.shadowColor = particle.color;
      } else {
        ctx.shadowBlur = 0;
      }

      ctx.fillRect(
        particle.x - particle.size * 0.5,
        particle.y - particle.size * 0.5,
        particle.size,
        particle.size
      );
    }

    ctx.globalAlpha = 1;
    ctx.shadowBlur = 8;
    ctx.shadowColor = "#4ade80";
  }

  private drawDebris(ctx: CanvasRenderingContext2D): void {
    ctx.shadowBlur = 5;
    ctx.shadowColor = "#6b7280";

    for (const d of this.debris) {
      if (d.life <= 0) continue;

      ctx.globalAlpha = d.life / d.maxLife;
      ctx.save();
      ctx.translate(d.x, d.y);
      ctx.rotate(d.angle);

      ctx.beginPath();
      const vertexCount = d.vertices.length;
      for (let i = 0; i < vertexCount; i++) {
        const angle = (i / vertexCount) * Math.PI * 2;
        const r = d.size * d.vertices[i];
        const x = Math.cos(angle) * r;
        const y = Math.sin(angle) * r;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();
      ctx.stroke();
      ctx.restore();
    }

    ctx.globalAlpha = 1;
    ctx.shadowBlur = 8;
    ctx.shadowColor = "#4ade80";
  }

  private drawHud(ctx: CanvasRenderingContext2D): void {
    ctx.save();
    ctx.shadowBlur = 10;
    ctx.shadowColor = "#4ade80";
    ctx.fillStyle = "#d6fff0";
    ctx.font = "600 20px 'Orbitron', 'Eurostile', sans-serif";
    ctx.textBaseline = "top";

    const scoreLabel = `SCORE ${this.score.toString().padStart(5, "0")}`;
    const highLabel = `HIGH ${this.highScore.toString().padStart(5, "0")}`;
    const waveLabel = `WAVE ${Math.max(1, this.wave)}`;

    ctx.fillText(scoreLabel, 20, 18);
    ctx.fillText(highLabel, WORLD_WIDTH - 230, 18);
    ctx.fillText(waveLabel, WORLD_WIDTH - 145, WORLD_HEIGHT - 40);

    // Draw ship lives as icons instead of text
    this.drawShipLives(ctx, 20, WORLD_HEIGHT - 45, this.lives);

    ctx.restore();
  }

  private drawShipLives(ctx: CanvasRenderingContext2D, startX: number, startY: number, count: number): void {
    const shipSize = 10;
    const spacing = 22;
    const maxDisplay = Math.min(count, 10); // Cap display at 10 ships

    ctx.save();
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = "#d6fff0";

    for (let i = 0; i < maxDisplay; i++) {
      const x = startX + i * spacing + shipSize;
      const y = startY;

      ctx.save();
      ctx.translate(x, y);
      // Point up
      ctx.rotate(0);

      ctx.beginPath();
      ctx.moveTo(0, -shipSize);
      ctx.lineTo(shipSize * 0.72, shipSize);
      ctx.lineTo(0, shipSize * 0.45);
      ctx.lineTo(-shipSize * 0.72, shipSize);
      ctx.closePath();
      ctx.stroke();

      ctx.restore();
    }

    // If more than 10 lives, show "+N" indicator
    if (count > 10) {
      ctx.font = "500 14px 'IBM Plex Mono', monospace";
      ctx.fillStyle = "#d6fff0";
      ctx.fillText(`+${count - 10}`, startX + maxDisplay * spacing + 5, startY - 5);
    }

    ctx.restore();
  }

  private drawOverlay(ctx: CanvasRenderingContext2D, alpha: number): void {
    void alpha;

    if (this.mode === "playing") {
      return;
    }

    ctx.save();

    // Vignette effect
    const gradient = ctx.createRadialGradient(
      WORLD_WIDTH / 2, WORLD_HEIGHT / 2, WORLD_HEIGHT * 0.3,
      WORLD_WIDTH / 2, WORLD_HEIGHT / 2, WORLD_HEIGHT * 0.8
    );
    gradient.addColorStop(0, "rgba(0, 8, 14, 0.6)");
    gradient.addColorStop(1, "rgba(0, 8, 14, 0.9)");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, WORLD_WIDTH, WORLD_HEIGHT);

    ctx.shadowBlur = 20;
    ctx.shadowColor = "#4ade80";
    ctx.fillStyle = "#d6fff0";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = "700 56px 'Orbitron', 'Eurostile', sans-serif";

    if (this.mode === "menu") {
      // Animated title
      const pulse = 1 + Math.sin(Date.now() * 0.003) * 0.05;
      ctx.save();
      ctx.translate(WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.34);
      ctx.scale(pulse, pulse);
      ctx.fillText("ASTEROIDS", 0, 0);
      ctx.restore();

      ctx.font = "600 24px 'IBM Plex Mono', 'SFMono-Regular', monospace";
      ctx.fillText("Arrow Keys: Turn + Thrust", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.48);
      ctx.fillText("Space: Fire  Shift: Hyperspace", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.54);
      ctx.fillText("P: Pause  R: Restart", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.6);

      ctx.shadowBlur = 10;
      ctx.fillStyle = "#4ade80";
      ctx.fillText("Press Enter or Tap to Launch", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.72);
    }

    if (this.mode === "paused") {
      ctx.fillText("PAUSED", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.45);
      ctx.font = "600 24px 'IBM Plex Mono', 'SFMono-Regular', monospace";
      if (this.pauseFromHidden) {
        ctx.fillText("Tab hidden: auto-paused", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.56);
      }
      ctx.fillText("Press P / Enter or Tap to Resume", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.66);
    }

    if (this.mode === "game-over") {
      ctx.shadowColor = "#ef4444";
      ctx.fillStyle = "#ff6b6b";
      ctx.fillText("GAME OVER", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.42);
      ctx.font = "600 28px 'IBM Plex Mono', 'SFMono-Regular', monospace";
      ctx.fillStyle = "#d6fff0";
      ctx.fillText(
        `Final Score: ${this.score.toString().padStart(5, "0")}`,
        WORLD_WIDTH * 0.5,
        WORLD_HEIGHT * 0.56,
      );
      ctx.fillText("Press Enter, R, or Tap to Restart", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.68);
    }

    ctx.restore();
  }

  private loadHighScore(): void {
    try {
      const value = window.localStorage.getItem(STORAGE_HIGH_SCORE_KEY);

      if (!value) {
        return;
      }

      const parsed = Number.parseInt(value, 10);

      if (Number.isFinite(parsed) && parsed > 0) {
        this.highScore = parsed;
      }
    } catch {
      // Ignore storage access issues.
    }
  }

  private saveHighScore(): void {
    try {
      if (this.score > this.highScore) {
        this.highScore = this.score;
      }

      window.localStorage.setItem(STORAGE_HIGH_SCORE_KEY, String(this.highScore));
    } catch {
      // Ignore storage write failures.
    }
  }
}
