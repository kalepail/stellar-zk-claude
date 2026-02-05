import {
  ASTEROID_CAP,
  ASTEROID_SPEED_Q8_8,
  EXTRA_LIFE_SCORE_STEP,
  FIXED_TIMESTEP,
  LURK_SAUCER_SPAWN_FAST_FRAMES,
  LURK_TIME_THRESHOLD_FRAMES,
  MAX_DEBRIS,
  MAX_FRAME_DELTA,
  MAX_PARTICLES,
  MAX_SUBSTEPS,
  SAUCER_BULLET_LIFETIME_FRAMES,
  SAUCER_BULLET_SPEED_Q8_8,
  SAUCER_SPAWN_MAX_FRAMES,
  SAUCER_SPAWN_MIN_FRAMES,
  SAUCER_SPEED_LARGE_Q8_8,
  SAUCER_SPEED_SMALL_Q8_8,
  SCORE_LARGE_ASTEROID,
  SCORE_LARGE_SAUCER,
  SCORE_MEDIUM_ASTEROID,
  SCORE_SMALL_ASTEROID,
  SCORE_SMALL_SAUCER,
  SHAKE_DECAY,
  SHAKE_INTENSITY_LARGE,
  SHAKE_INTENSITY_MEDIUM,
  SHAKE_INTENSITY_SMALL,
  SHIP_BULLET_COOLDOWN_FRAMES,
  SHIP_BULLET_LIFETIME_FRAMES,
  SHIP_BULLET_LIMIT,
  SHIP_BULLET_SPEED_Q8_8,
  SHIP_FACING_UP_BAM,
  SHIP_MAX_SPEED_SQ_Q16_16,
  SHIP_RADIUS,
  SHIP_RESPAWN_FRAMES,
  SHIP_SPAWN_INVULNERABLE_FRAMES,
  SHIP_THRUST_Q8_8,
  SHIP_TURN_SPEED_BAM,
  STARTING_LIVES,
  STORAGE_HIGH_SCORE_KEY,
  WORLD_HEIGHT,
  WORLD_HEIGHT_Q12_4,
  WORLD_WIDTH,
  WORLD_WIDTH_Q12_4,
} from "./constants";
import { AudioSystem } from "./AudioSystem";
import { Autopilot, type GameStateSnapshot } from "./Autopilot";
import {
  applyDrag,
  atan2BAM,
  BAMToRadians,
  clampSpeedQ8_8,
  cosBAM,
  displaceQ12_4,
  fromQ12_4,
  fromQ8_8,
  sinBAM,
  toQ12_4,
  velocityQ8_8,
} from "./fixed-point";
import { InputController } from "./input";
import type { InputSource } from "./input-source";
import { LiveInputSource, TapeInputSource } from "./input-source";
import {
  clamp,
  getGameRng,
  getGameRngState,
  randomInt,
  setGameSeed,
  visualRandomInt,
  visualRandomRange,
  wrapX,
  wrapXQ12_4,
  wrapY,
  wrapYQ12_4,
} from "./math";
import { deserializeTape, serializeTape, TapeRecorder } from "./tape";
import type {
  Asteroid,
  AsteroidSize,
  Bullet,
  Debris,
  GameMode,
  Particle,
  Saucer,
  Ship,
  Star,
} from "./types";

const ASTEROID_RADIUS_BY_SIZE: Record<AsteroidSize, number> = {
  large: 48,
  medium: 28,
  small: 16,
};

const SAUCER_RADIUS_LARGE = 22;
const SAUCER_RADIUS_SMALL = 16;

function shortestDeltaQ12_4(from: number, to: number, size: number): number {
  let delta = to - from;
  const half = size >> 1;
  if (delta > half) delta -= size;
  if (delta < -half) delta += size;
  return delta;
}

function collisionDistSqQ12_4(ax: number, ay: number, bx: number, by: number): number {
  const dx = shortestDeltaQ12_4(ax, bx, WORLD_WIDTH_Q12_4);
  const dy = shortestDeltaQ12_4(ay, by, WORLD_HEIGHT_Q12_4);
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

export interface GameConfig {
  canvas?: HTMLCanvasElement;
  headless?: boolean;
  seed?: number;
}

export class AsteroidsGame {
  private readonly canvas: HTMLCanvasElement | null;

  private readonly ctx: CanvasRenderingContext2D | null;

  private readonly input = new InputController();

  private readonly audio = new AudioSystem();

  private readonly autopilot = new Autopilot();

  private readonly headless: boolean;

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

  private saucerSpawnTimer = randomInt(SAUCER_SPAWN_MIN_FRAMES, SAUCER_SPAWN_MAX_FRAMES);

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

  // Thrust timing (frame-count based)
  private thrustParticleTimer = 0;

  // Anti-lurking: time since last asteroid destroyed by player
  private timeSinceLastKill = 0;

  // Game time for animations (star twinkle, etc.)
  private gameTime = 0;

  // Game seed for deterministic RNG (ZK-friendly)
  private gameSeed = 0;

  // Frame counter for deterministic timing
  private frameCount = 0;

  // Input source abstraction (live keyboard/autopilot or tape replay)
  private inputSource: InputSource | null = null;

  // Tape recording
  private recorder: TapeRecorder | null = null;

  // Replay state
  private replaySpeed = 1;
  private replayPaused = false;
  private replayTapeSource: TapeInputSource | null = null;

  // Current frame input (read at start of updateSimulation, used by updateShip)
  private currentFrameInput: { left: boolean; right: boolean; thrust: boolean; fire: boolean } = {
    left: false,
    right: false,
    thrust: false,
    fire: false,
  };

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

  constructor(config: GameConfig | HTMLCanvasElement) {
    // Backward compatible: accept raw canvas or config object
    const cfg: GameConfig =
      typeof HTMLCanvasElement !== "undefined" && config instanceof HTMLCanvasElement
        ? { canvas: config }
        : (config as GameConfig);

    this.headless = cfg.headless === true;
    this.canvas = cfg.canvas ?? null;
    this.ctx = null;
    this.ship = this.createShip();

    if (this.headless) {
      // Headless mode: no rendering, no events, no audio
      if (cfg.seed !== undefined) {
        this.gameSeed = cfg.seed;
        setGameSeed(this.gameSeed);
      }
      return;
    }

    // Interactive mode
    if (!this.canvas) {
      throw new Error("Canvas required for non-headless mode.");
    }

    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("aria-label", "Asteroids game canvas");

    const ctx = this.canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      throw new Error("Unable to create 2D context.");
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- bypass readonly for init
    (this as any).ctx = ctx;

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

    if (!this.headless) {
      this.detachEvents();
    }
  }

  private attachEvents(): void {
    window.addEventListener("keydown", this.keyDownHandler, { passive: false });
    window.addEventListener("keyup", this.keyUpHandler, { passive: false });
    window.addEventListener("resize", this.resizeHandler);
    document.addEventListener("visibilitychange", this.visibilityHandler);
    this.canvas!.addEventListener("pointerdown", this.pointerDownHandler);
  }

  private detachEvents(): void {
    window.removeEventListener("keydown", this.keyDownHandler);
    window.removeEventListener("keyup", this.keyUpHandler);
    window.removeEventListener("resize", this.resizeHandler);
    document.removeEventListener("visibilitychange", this.visibilityHandler);
    this.canvas!.removeEventListener("pointerdown", this.pointerDownHandler);
  }

  private resize(): void {
    const rect = this.canvas!.getBoundingClientRect();
    const width = Math.max(320, rect.width || WORLD_WIDTH);
    const height = Math.max(320, rect.height || WORLD_HEIGHT);
    const dpr = window.devicePixelRatio || 1;
    this.dpr = dpr;

    this.cssWidth = width;
    this.cssHeight = height;

    this.canvas!.width = Math.floor(width * dpr);
    this.canvas!.height = Math.floor(height * dpr);

    this.ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.ctx!.imageSmoothingEnabled = false;

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
    } else if (this.mode === "replay") {
      // Replay mode: accumulator-based, same as playing but with speed multiplier
      if (this.lastTimeMs === 0) {
        this.lastTimeMs = timestampMs;
      }

      let deltaSeconds = (timestampMs - this.lastTimeMs) / 1000;
      this.lastTimeMs = timestampMs;
      deltaSeconds = Math.min(MAX_FRAME_DELTA, Math.max(0, deltaSeconds));

      if (!this.replayPaused && this.replayTapeSource && !this.replayTapeSource.isComplete()) {
        this.accumulator += deltaSeconds * this.replaySpeed;
        let steps = 0;
        const maxSteps = MAX_SUBSTEPS * this.replaySpeed;

        while (this.accumulator >= FIXED_TIMESTEP && steps < maxSteps) {
          if (this.replayTapeSource.isComplete()) break;
          this.storePreviousPositions();
          this.updateSimulation(FIXED_TIMESTEP);
          this.accumulator -= FIXED_TIMESTEP;
          steps += 1;
        }

        if (steps >= maxSteps) {
          this.accumulator = 0;
        }
      }

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

    // Toggle autopilot with 'A' key
    if (this.input.consumePress("KeyA") && this.mode === "playing") {
      this.autopilot.toggle();
    }

    // Download tape with 'D' key in game-over
    if (this.input.consumePress("KeyD") && this.mode === "game-over") {
      this.downloadTape();
    }

    // Load tape with 'L' key in menu
    if (this.input.consumePress("KeyL") && this.mode === "menu") {
      this.triggerFileLoad();
    }

    // Replay speed controls
    if (this.mode === "replay") {
      if (this.input.consumePress("Digit1")) { this.replaySpeed = 1; this.accumulator = 0; }
      if (this.input.consumePress("Digit2")) { this.replaySpeed = 2; this.accumulator = 0; }
      if (this.input.consumePress("Digit4")) { this.replaySpeed = 4; this.accumulator = 0; }
      if (this.input.consumePress("Space")) {
        this.replayPaused = !this.replayPaused;
        // Reset timing to avoid accumulator jump after unpause
        this.lastTimeMs = 0;
        this.accumulator = 0;
      }
    }

    // Return to menu with Escape
    if (this.input.consumePress("Escape") && this.mode !== "menu") {
      this.mode = "menu";
      this.asteroids = [];
      this.bullets = [];
      this.saucers = [];
      this.saucerBullets = [];
      this.particles = [];
      this.debris = [];
      this.ship = this.createShip();
      this.shakeIntensity = 0;
      this.shakeX = 0;
      this.shakeY = 0;
      this.shakeRotation = 0;
      this.autopilot.setEnabled(false);
      this.replayTapeSource = null;
      this.inputSource = null;
    }
  }

  startNewGame(seed?: number): void {
    // Generate deterministic seed for ZK-friendly RNG
    this.gameSeed = seed ?? Date.now();
    setGameSeed(this.gameSeed);

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
    this.timeSinceLastKill = 0;
    this.frameCount = 0;
    this.ship = this.createShip();
    this.shakeIntensity = 0;
    this.autopilot.setEnabled(false);

    // Set up recording
    this.recorder = new TapeRecorder();

    // Set up live input source (unless an external source is already set)
    if (!this.inputSource || this.inputSource instanceof TapeInputSource) {
      this.inputSource = new LiveInputSource(
        this.input,
        this.autopilot,
        () => this.autopilot.update(this.getGameStateSnapshot(), FIXED_TIMESTEP, this.gameTime),
      );
    }

    // Reset replay state
    this.replayTapeSource = null;
    this.replaySpeed = 1;
    this.replayPaused = false;

    this.spawnWave();
    const waveMultPct = Math.max(40, 100 - (this.wave - 1) * 8);
    const spawnMin = ((SAUCER_SPAWN_MIN_FRAMES * waveMultPct) / 100) | 0;
    const spawnMax = ((SAUCER_SPAWN_MAX_FRAMES * waveMultPct) / 100) | 0;
    this.saucerSpawnTimer = randomInt(spawnMin, spawnMax);
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
    this.frameCount++;

    // Read input for this frame (always, even when ship can't be controlled)
    this.currentFrameInput = this.inputSource
      ? this.inputSource.getFrameInput()
      : { left: false, right: false, thrust: false, fire: false };

    // Record input for tape (one byte per frame, always)
    this.recorder?.record(this.currentFrameInput);

    this.updateShip(dt);
    this.updateAsteroids();
    this.updateBullets();
    this.updateSaucers();
    this.updateSaucerBullets();

    if (!this.headless) {
      this.updateParticles(dt);
      this.updateDebris(dt);
      this.updateScreenShake();
    }

    this.handleCollisions();
    this.pruneDestroyedEntities();

    // Anti-lurking timer (frame count)
    this.timeSinceLastKill++;

    // Advance input source cursor
    this.inputSource?.advance();

    if (
      (this.mode === "playing" || this.mode === "replay") &&
      this.asteroids.length === 0 &&
      this.saucers.length === 0
    ) {
      this.spawnWave();
    }
  }

  private createShip(): Ship {
    const x = toQ12_4(WORLD_WIDTH * 0.5); // 7680
    const y = toQ12_4(WORLD_HEIGHT * 0.5); // 5760
    const angle = SHIP_FACING_UP_BAM; // 192 (points up)
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
      invulnerableTimer: SHIP_SPAWN_INVULNERABLE_FRAMES,
    };
  }

  private getShipSpawnPoint(): { x: number; y: number } {
    return {
      x: toQ12_4(WORLD_WIDTH * 0.5),
      y: toQ12_4(WORLD_HEIGHT * 0.5),
    };
  }

  private queueShipRespawn(delayFrames: number): void {
    this.ship.canControl = false;
    this.ship.respawnTimer = delayFrames;
    this.ship.vx = 0;
    this.ship.vy = 0;
    this.ship.fireCooldown = 0;
    this.ship.invulnerableTimer = 0;
  }

  private isShipSpawnAreaClear(spawnX: number, spawnY: number, clearRadiusQ12_4 = 1920): boolean {
    const blockedByAsteroid = this.asteroids.some((asteroid) => {
      const hitDist = (asteroid.radius << 4) + clearRadiusQ12_4;
      return collisionDistSqQ12_4(asteroid.x, asteroid.y, spawnX, spawnY) < hitDist * hitDist;
    });

    if (blockedByAsteroid) {
      return false;
    }

    const blockedBySaucer = this.saucers.some((saucer) => {
      if (!saucer.alive) return false;
      const hitDist = (saucer.radius << 4) + clearRadiusQ12_4;
      return collisionDistSqQ12_4(saucer.x, saucer.y, spawnX, spawnY) < hitDist * hitDist;
    });

    if (blockedBySaucer) {
      return false;
    }

    return !this.saucerBullets.some((bullet) => {
      if (!bullet.alive) return false;
      const hitDist = (bullet.radius << 4) + clearRadiusQ12_4;
      return collisionDistSqQ12_4(bullet.x, bullet.y, spawnX, spawnY) < hitDist * hitDist;
    });
  }

  private trySpawnShipAtCenter(): boolean {
    const { x: spawnX, y: spawnY } = this.getShipSpawnPoint();

    if (!this.isShipSpawnAreaClear(spawnX, spawnY)) {
      return false;
    }

    this.ship.x = spawnX;
    this.ship.y = spawnY;
    this.ship.prevX = spawnX;
    this.ship.prevY = spawnY;
    this.ship.vx = 0;
    this.ship.vy = 0;
    this.ship.angle = SHIP_FACING_UP_BAM;
    this.ship.prevAngle = SHIP_FACING_UP_BAM;
    this.ship.canControl = true;
    this.ship.invulnerableTimer = SHIP_SPAWN_INVULNERABLE_FRAMES;
    return true;
  }

  private spawnWave(): void {
    this.wave += 1;
    this.timeSinceLastKill = 0;

    const largeCount = Math.min(16, 4 + (this.wave - 1) * 2);
    const { x: avoidX, y: avoidY } = this.getShipSpawnPoint();
    // 180px in Q12.4 = 2880; squared = 8,294,400
    const safeDistSq = 2880 * 2880;

    for (let i = 0; i < largeCount; i += 1) {
      let x = randomInt(0, WORLD_WIDTH_Q12_4);
      let y = randomInt(0, WORLD_HEIGHT_Q12_4);

      let guard = 0;

      while (collisionDistSqQ12_4(x, y, avoidX, avoidY) < safeDistSq && guard < 20) {
        x = randomInt(0, WORLD_WIDTH_Q12_4);
        y = randomInt(0, WORLD_HEIGHT_Q12_4);
        guard += 1;
      }

      this.asteroids.push(this.createAsteroid("large", x, y));
    }

    // Use the same spawn policy as death-respawn: ship enters only when center is safe.
    this.queueShipRespawn(0);
    this.trySpawnShipAtCenter();
  }

  private createAsteroid(size: AsteroidSize, x: number, y: number): Asteroid {
    const [minQ8_8, maxQ8_8] = ASTEROID_SPEED_Q8_8[size];
    const moveAngle = randomInt(0, 256); // BAM
    let speed = randomInt(minQ8_8, maxQ8_8);
    // Wave speed multiplier: speed * (1 + min(0.5, (wave-1)*0.06))
    // Integer: speed + speed * min(128, (wave-1)*15) >> 8
    speed = speed + ((speed * Math.min(128, (this.wave - 1) * 15)) >> 8);
    const { vx, vy } = velocityQ8_8(moveAngle, speed);
    const vertices = this.createAsteroidVertices();
    const startAngle = randomInt(0, 256); // BAM
    // spin: +-3 BAM/frame (was +-0.7 rad/s -> +-0.7/60/(2pi/256) ~ +-0.47 -> +-1-3)
    const spin = randomInt(-3, 4);

    return {
      id: this.nextId++,
      x,
      y,
      vx,
      vy,
      angle: startAngle,
      alive: true,
      radius: ASTEROID_RADIUS_BY_SIZE[size],
      prevX: x,
      prevY: y,
      prevAngle: startAngle,
      size,
      spin,
      vertices,
    };
  }

  private createAsteroidVertices(): number[] {
    // Vertices are visual-only; use visual RNG
    const vertexCount = visualRandomInt(9, 14);
    const vertices: number[] = [];

    for (let i = 0; i < vertexCount; i += 1) {
      vertices.push(visualRandomRange(0.72, 1.2));
    }

    return vertices;
  }

  private updateShip(_dt: number): void {
    const ship = this.ship;

    if (ship.fireCooldown > 0) ship.fireCooldown--;

    if (!ship.canControl) {
      if (ship.respawnTimer > 0) ship.respawnTimer--;

      if (ship.respawnTimer <= 0) {
        this.trySpawnShipAtCenter();
      }

      return;
    }

    if (ship.invulnerableTimer > 0) ship.invulnerableTimer--;

    // Get input from the current frame input (already read+recorded in updateSimulation)
    const frameInput = this.currentFrameInput;
    const turnLeft = frameInput.left;
    const turnRight = frameInput.right;
    const thrust = frameInput.thrust;
    const fire = frameInput.fire;

    if (turnLeft) {
      ship.angle = (ship.angle - SHIP_TURN_SPEED_BAM) & 0xff;
    }

    if (turnRight) {
      ship.angle = (ship.angle + SHIP_TURN_SPEED_BAM) & 0xff;
    }

    if (thrust) {
      const accelVx = (cosBAM(ship.angle) * SHIP_THRUST_Q8_8) >> 14;
      const accelVy = (sinBAM(ship.angle) * SHIP_THRUST_Q8_8) >> 14;
      ship.vx += accelVx;
      ship.vy += accelVy;

      // Thrust particles and sound (every 5 frames)
      this.thrustParticleTimer++;
      if (this.thrustParticleTimer >= 5) {
        this.spawnThrustParticles(ship);
        this.audio.playThrust();
        this.thrustParticleTimer = 0;
      }
    }

    ship.vx = applyDrag(ship.vx);
    ship.vy = applyDrag(ship.vy);

    ({ vx: ship.vx, vy: ship.vy } = clampSpeedQ8_8(ship.vx, ship.vy, SHIP_MAX_SPEED_SQ_Q16_16));

    if (fire && ship.fireCooldown <= 0 && this.bullets.length < SHIP_BULLET_LIMIT) {
      this.spawnShipBullet();
      ship.fireCooldown = SHIP_BULLET_COOLDOWN_FRAMES;
    }

    // Q8.8 velocity >> 4 -> Q12.4 displacement
    ship.x = wrapXQ12_4(ship.x + (ship.vx >> 4));
    ship.y = wrapYQ12_4(ship.y + (ship.vy >> 4));
  }

  /** Create a snapshot of game state for the autopilot AI (converts to float px/s) */
  private getGameStateSnapshot(): GameStateSnapshot {
    const toFloat = <
      T extends {
        x: number;
        y: number;
        vx: number;
        vy: number;
        angle: number;
        prevX: number;
        prevY: number;
        prevAngle: number;
      },
    >(
      e: T,
    ): T => ({
      ...e,
      x: fromQ12_4(e.x),
      y: fromQ12_4(e.y),
      vx: fromQ8_8(e.vx) * 60, // back to px/s for Autopilot
      vy: fromQ8_8(e.vy) * 60,
      angle: BAMToRadians(e.angle),
      prevX: fromQ12_4(e.prevX),
      prevY: fromQ12_4(e.prevY),
      prevAngle: BAMToRadians(e.prevAngle),
    });
    return {
      ship: toFloat(this.ship) as Ship,
      asteroids: this.asteroids.filter((a) => a.alive).map((a) => toFloat(a) as Asteroid),
      saucers: this.saucers.filter((s) => s.alive).map((s) => toFloat(s) as Saucer),
      bullets: this.bullets.filter((b) => b.alive).map((b) => toFloat(b) as Bullet),
      saucerBullets: this.saucerBullets.filter((b) => b.alive).map((b) => toFloat(b) as Bullet),
    };
  }

  private spawnShipBullet(): void {
    const ship = this.ship;
    const { dx, dy } = displaceQ12_4(ship.angle, ship.radius + 6);
    const startX = wrapXQ12_4(ship.x + dx);
    const startY = wrapYQ12_4(ship.y + dy);

    // Bullet speed = base + ship speed boost
    // Approximate ship speed magnitude: (|vx| + |vy|) * 3/4
    const shipSpeedApprox = ((Math.abs(ship.vx) + Math.abs(ship.vy)) * 3) >> 2;
    // 89/256 ~ 0.35
    const bulletSpeedQ8_8 = SHIP_BULLET_SPEED_Q8_8 + ((shipSpeedApprox * 89) >> 8);
    const { vx: bvx, vy: bvy } = velocityQ8_8(ship.angle, bulletSpeedQ8_8);

    const bullet: Bullet = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx: ship.vx + bvx,
      vy: ship.vy + bvy,
      angle: ship.angle,
      alive: true,
      radius: 2,
      prevX: startX,
      prevY: startY,
      prevAngle: ship.angle,
      life: SHIP_BULLET_LIFETIME_FRAMES,
      fromSaucer: false,
    };

    this.bullets.push(bullet);
    this.audio.playShoot();

    // Muzzle flash at slightly further offset
    const { dx: mfDx, dy: mfDy } = displaceQ12_4(ship.angle, ship.radius + 8);
    this.spawnMuzzleFlash(fromQ12_4(ship.x + mfDx), fromQ12_4(ship.y + mfDy));
  }

  private updateAsteroids(): void {
    for (const asteroid of this.asteroids) {
      if (!asteroid.alive) {
        continue;
      }

      asteroid.x = wrapXQ12_4(asteroid.x + (asteroid.vx >> 4));
      asteroid.y = wrapYQ12_4(asteroid.y + (asteroid.vy >> 4));
      asteroid.angle = (asteroid.angle + asteroid.spin) & 0xff;
    }
  }

  private updateBullets(): void {
    for (const bullet of this.bullets) {
      if (!bullet.alive) {
        continue;
      }

      bullet.life--;

      if (bullet.life <= 0) {
        bullet.alive = false;
        continue;
      }

      bullet.x = wrapXQ12_4(bullet.x + (bullet.vx >> 4));
      bullet.y = wrapYQ12_4(bullet.y + (bullet.vy >> 4));
    }
  }

  private updateSaucerBullets(): void {
    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) {
        continue;
      }

      bullet.life--;

      if (bullet.life <= 0) {
        bullet.alive = false;
        continue;
      }

      bullet.x = wrapXQ12_4(bullet.x + (bullet.vx >> 4));
      bullet.y = wrapYQ12_4(bullet.y + (bullet.vy >> 4));
    }
  }

  private updateSaucers(): void {
    if (this.saucerSpawnTimer > 0) this.saucerSpawnTimer--;

    // Anti-lurking: spawn saucers faster if player isn't killing asteroids
    const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD_FRAMES;
    const spawnThreshold = isLurking ? LURK_SAUCER_SPAWN_FAST_FRAMES : 0;

    const maxSaucers = this.wave < 4 ? 1 : this.wave < 7 ? 2 : 3;
    if (this.saucers.length < maxSaucers && this.saucerSpawnTimer <= spawnThreshold) {
      this.spawnSaucer();
      // waveSpawnMult as integer percentage: max(40, 100 - (wave-1)*8)
      const waveMultPct = Math.max(40, 100 - (this.wave - 1) * 8);
      const spawnMin = ((SAUCER_SPAWN_MIN_FRAMES * waveMultPct) / 100) | 0;
      const spawnMax = ((SAUCER_SPAWN_MAX_FRAMES * waveMultPct) / 100) | 0;
      this.saucerSpawnTimer = isLurking
        ? randomInt(LURK_SAUCER_SPAWN_FAST_FRAMES, LURK_SAUCER_SPAWN_FAST_FRAMES + 120)
        : randomInt(spawnMin, spawnMax);
    }

    for (const saucer of this.saucers) {
      if (!saucer.alive) {
        continue;
      }

      // Saucer doesn't wrap X (exits screen)
      saucer.x = saucer.x + (saucer.vx >> 4);
      saucer.y = wrapYQ12_4(saucer.y + (saucer.vy >> 4));

      // Off-screen check in Q12.4
      if (saucer.x < toQ12_4(-80) || saucer.x > toQ12_4(WORLD_WIDTH + 80)) {
        saucer.alive = false;
        continue;
      }

      if (saucer.driftTimer > 0) saucer.driftTimer--;
      if (saucer.driftTimer <= 0) {
        saucer.driftTimer = randomInt(48, 120);
        // drift vy: +-38 px/s -> +-38/60*256 ~ +-163 Q8.8
        saucer.vy = randomInt(-163, 164);
      }

      if (saucer.fireCooldown > 0) saucer.fireCooldown--;

      if (saucer.fireCooldown <= 0) {
        this.spawnSaucerBullet(saucer);
        saucer.fireCooldown = saucer.small
          ? isLurking
            ? randomInt(27, 46)
            : randomInt(39, 66)
          : isLurking
            ? randomInt(46, 67)
            : randomInt(66, 96);
      }
    }
  }

  private spawnSaucer(): void {
    const enterFromLeft = getGameRng().next() % 2 === 0;
    // Anti-lurking: more likely to spawn small (deadly) saucer when lurking
    const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD_FRAMES;
    const smallPct = isLurking ? 90 : this.score > 4000 ? 70 : 22;
    const small = getGameRng().next() % 100 < smallPct;
    const speedQ8_8 = small ? SAUCER_SPEED_SMALL_Q8_8 : SAUCER_SPEED_LARGE_Q8_8;

    const startX = toQ12_4(enterFromLeft ? -30 : WORLD_WIDTH + 30);
    const startY = randomInt(toQ12_4(72), toQ12_4(WORLD_HEIGHT - 72));

    const saucer: Saucer = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx: enterFromLeft ? speedQ8_8 : -speedQ8_8,
      // +-22 px/s -> +-22/60*256 ~ +-94 Q8.8
      vy: randomInt(-94, 95),
      angle: 0,
      alive: true,
      radius: small ? SAUCER_RADIUS_SMALL : SAUCER_RADIUS_LARGE,
      prevX: startX,
      prevY: startY,
      prevAngle: 0,
      small,
      fireCooldown: randomInt(18, 48),
      driftTimer: randomInt(48, 120),
    };

    this.saucers.push(saucer);
    this.audio.playSaucer(small);
  }

  private spawnSaucerBullet(saucer: Saucer): void {
    let shotAngle: number;

    if (saucer.small) {
      // Aimed shot using atan2BAM
      const dx = shortestDeltaQ12_4(saucer.x, this.ship.x, WORLD_WIDTH_Q12_4);
      const dy = shortestDeltaQ12_4(saucer.y, this.ship.y, WORLD_HEIGHT_Q12_4);
      const targetAngle = atan2BAM(dy, dx);
      // Error in BAM: 30deg ~ 21 BAM, 15deg ~ 11 BAM, 4deg ~ 3 BAM
      const isLurking = this.timeSinceLastKill > LURK_TIME_THRESHOLD_FRAMES;
      const baseErrorBAM = isLurking ? 11 : 21;
      const scoreBonus = (this.score / 2500) | 0; // integer division
      const waveBonus = Math.min(11, this.wave * 1); // ~ wave*2 deg -> wave*1.4 BAM ~ wave*1
      const errorBAM = clamp(baseErrorBAM - scoreBonus - waveBonus, 3, baseErrorBAM);
      shotAngle = (targetAngle + randomInt(-errorBAM, errorBAM + 1)) & 0xff;
    } else {
      // Random shot
      shotAngle = randomInt(0, 256);
    }

    const { vx, vy } = velocityQ8_8(shotAngle, SAUCER_BULLET_SPEED_Q8_8);
    const { dx: offDx, dy: offDy } = displaceQ12_4(shotAngle, saucer.radius + 4);
    const startX = wrapXQ12_4(saucer.x + offDx);
    const startY = wrapYQ12_4(saucer.y + offDy);

    const bullet: Bullet = {
      id: this.nextId++,
      x: startX,
      y: startY,
      vx,
      vy,
      angle: shotAngle,
      alive: true,
      radius: 2,
      prevX: startX,
      prevY: startY,
      prevAngle: shotAngle,
      life: SAUCER_BULLET_LIFETIME_FRAMES,
      fromSaucer: true,
    };

    this.saucerBullets.push(bullet);
  }

  private handleCollisions(): void {
    // Bullet-asteroid collisions
    for (const bullet of this.bullets) {
      if (!bullet.alive) continue;

      for (const asteroid of this.asteroids) {
        if (!asteroid.alive) continue;

        const hitDistQ12_4 = (bullet.radius + asteroid.radius) << 4;
        if (
          collisionDistSqQ12_4(bullet.x, bullet.y, asteroid.x, asteroid.y) <=
          hitDistQ12_4 * hitDistQ12_4
        ) {
          bullet.alive = false;
          this.destroyAsteroid(asteroid, true);
          break;
        }
      }
    }

    // Saucer bullet-asteroid collisions
    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) continue;

      for (const asteroid of this.asteroids) {
        if (!asteroid.alive) continue;

        const hitDistQ12_4 = (bullet.radius + asteroid.radius) << 4;
        if (
          collisionDistSqQ12_4(bullet.x, bullet.y, asteroid.x, asteroid.y) <=
          hitDistQ12_4 * hitDistQ12_4
        ) {
          bullet.alive = false;
          this.destroyAsteroid(asteroid, false);
          break;
        }
      }
    }

    // Player bullet-saucer collisions
    for (const bullet of this.bullets) {
      if (!bullet.alive) continue;

      for (const saucer of this.saucers) {
        if (!saucer.alive) continue;

        const hitDistQ12_4 = (bullet.radius + saucer.radius) << 4;
        if (
          collisionDistSqQ12_4(bullet.x, bullet.y, saucer.x, saucer.y) <=
          hitDistQ12_4 * hitDistQ12_4
        ) {
          bullet.alive = false;
          saucer.alive = false;
          this.addScore(saucer.small ? SCORE_SMALL_SAUCER : SCORE_LARGE_SAUCER);
          this.spawnExplosion(
            fromQ12_4(saucer.x),
            fromQ12_4(saucer.y),
            saucer.small ? "medium" : "large",
          );
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

    // Ship-asteroid collisions (fudge factor: radius * 0.88 -> (radius * 225) >> 8)
    for (const asteroid of this.asteroids) {
      if (!asteroid.alive) continue;

      const adjustedRadius = (asteroid.radius * 225) >> 8;
      const hitDistQ12_4 = (ship.radius + adjustedRadius) << 4;
      if (
        collisionDistSqQ12_4(ship.x, ship.y, asteroid.x, asteroid.y) <=
        hitDistQ12_4 * hitDistQ12_4
      ) {
        this.destroyShip();
        return;
      }
    }

    // Ship-saucer bullet collisions
    for (const bullet of this.saucerBullets) {
      if (!bullet.alive) continue;

      const hitDistQ12_4 = (ship.radius + bullet.radius) << 4;
      if (collisionDistSqQ12_4(ship.x, ship.y, bullet.x, bullet.y) <= hitDistQ12_4 * hitDistQ12_4) {
        bullet.alive = false;
        this.destroyShip();
        return;
      }
    }

    // Ship-saucer collisions
    for (const saucer of this.saucers) {
      if (!saucer.alive) continue;

      const hitDistQ12_4 = (ship.radius + saucer.radius) << 4;
      if (collisionDistSqQ12_4(ship.x, ship.y, saucer.x, saucer.y) <= hitDistQ12_4 * hitDistQ12_4) {
        saucer.alive = false;
        this.destroyShip();
        return;
      }
    }
  }

  private destroyAsteroid(asteroid: Asteroid, awardScore: boolean): void {
    asteroid.alive = false;

    if (awardScore) {
      this.timeSinceLastKill = 0;

      if (asteroid.size === "large") {
        this.addScore(SCORE_LARGE_ASTEROID);
      } else if (asteroid.size === "medium") {
        this.addScore(SCORE_MEDIUM_ASTEROID);
      } else {
        this.addScore(SCORE_SMALL_ASTEROID);
      }
    }

    // Spawn explosion and debris (convert to pixel coords for visual effects)
    const px = fromQ12_4(asteroid.x);
    const py = fromQ12_4(asteroid.y);
    this.spawnExplosion(px, py, asteroid.size);
    this.spawnDebris(px, py, asteroid.size);
    this.addScreenShake(
      asteroid.size === "large"
        ? SHAKE_INTENSITY_MEDIUM
        : asteroid.size === "medium"
          ? SHAKE_INTENSITY_SMALL
          : SHAKE_INTENSITY_SMALL * 0.5,
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
      // Velocity inheritance: (vx * 46) >> 8 ~ 0.18
      child.vx += (asteroid.vx * 46) >> 8;
      child.vy += (asteroid.vy * 46) >> 8;
      this.asteroids.push(child);
    }
  }

  private destroyShip(): void {
    this.queueShipRespawn(SHIP_RESPAWN_FRAMES);
    this.lives -= 1;

    // Big explosion effect (convert to pixel coords for visual effects)
    const px = fromQ12_4(this.ship.x);
    const py = fromQ12_4(this.ship.y);
    this.spawnExplosion(px, py, "large");
    this.spawnDebris(px, py, "large");
    this.addScreenShake(SHAKE_INTENSITY_LARGE);
    this.audio.playExplosion("large");

    if (this.lives <= 0) {
      if (this.mode !== "replay") {
        this.mode = "game-over";
      }
      this.ship.canControl = false;
      this.ship.respawnTimer = 99999;
      if (!this.headless) {
        this.saveHighScore();
      }
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
          WORLD_WIDTH * 0.5 + visualRandomRange(-100, 100),
          WORLD_HEIGHT * 0.5 + visualRandomRange(-50, 50),
          "spark",
          "#ffd700",
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
  private spawnParticle(
    x: number,
    y: number,
    type: Particle["type"],
    color: string,
    count = 1,
  ): void {
    if (this.headless) return;
    for (let i = 0; i < count; i++) {
      if (this.particles.length >= MAX_PARTICLES) break;

      const angle = visualRandomRange(0, Math.PI * 2);
      const speed = visualRandomRange(20, 120);
      const life = visualRandomRange(0.3, 0.8);

      this.particles.push({
        id: this.nextId++,
        x: x + visualRandomRange(-5, 5),
        y: y + visualRandomRange(-5, 5),
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        life,
        maxLife: life,
        size: visualRandomRange(1, 3),
        color,
        alpha: 1,
        decay: visualRandomRange(0.8, 1.2),
        type,
      });
    }
  }

  private spawnExplosion(x: number, y: number, size: "small" | "medium" | "large"): void {
    const particleCount = size === "large" ? 25 : size === "medium" ? 15 : 8;
    const colors = ["#ff6b35", "#f7931e", "#ffd700", "#ffffff"];

    for (let i = 0; i < particleCount; i++) {
      this.spawnParticle(x, y, "spark", colors[visualRandomInt(0, colors.length)]);
    }

    // Add smoke particles
    this.spawnParticle(x, y, "smoke", "#555555", 5);
  }

  private spawnMuzzleFlash(x: number, y: number): void {
    this.spawnParticle(x, y, "glow", "#a8ff60", 3);
  }

  private spawnThrustParticles(ship: Ship): void {
    // Opposite direction (angle + 128 BAM = +180deg)
    const { dx, dy } = displaceQ12_4((ship.angle + 128) & 0xff, ship.radius);
    const x = fromQ12_4(ship.x + dx);
    const y = fromQ12_4(ship.y + dy);

    this.spawnParticle(x, y, "spark", "#ffaa44", 2);
    this.spawnParticle(x, y, "smoke", "#666666", 1);
  }

  private spawnDebris(x: number, y: number, size: AsteroidSize | "large"): void {
    if (this.headless) return;
    const debrisCount = size === "large" ? 8 : size === "medium" ? 5 : 3;

    for (let i = 0; i < debrisCount; i++) {
      if (this.debris.length >= MAX_DEBRIS) break;

      const angle = visualRandomRange(0, Math.PI * 2);
      const speed = visualRandomRange(30, 90);
      const life = visualRandomRange(0.5, 1.2);
      const vertices: number[] = [];
      const vertexCount = visualRandomInt(4, 7);

      for (let j = 0; j < vertexCount; j++) {
        vertices.push(visualRandomRange(0.5, 1));
      }

      this.debris.push({
        id: this.nextId++,
        x: x + visualRandomRange(-10, 10),
        y: y + visualRandomRange(-10, 10),
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        angle: visualRandomRange(0, Math.PI * 2),
        spin: visualRandomRange(-2, 2),
        life,
        maxLife: life,
        size: visualRandomRange(3, 8),
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
      const baseAlpha = visualRandomRange(0.2, 0.95);
      this.stars.push({
        x: visualRandomRange(0, WORLD_WIDTH),
        y: visualRandomRange(0, WORLD_HEIGHT),
        alpha: baseAlpha,
        baseAlpha,
        twinkleSpeed: visualRandomRange(0.8, 2.5),
        twinklePhase: visualRandomRange(0, Math.PI * 2),
      });
    }
  }

  private render(alpha: number): void {
    if (this.headless || !this.ctx) return;
    const ctx = this.ctx;

    ctx.save();
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.cssWidth, this.cssHeight);

    // Deep space background
    const gradient = ctx.createRadialGradient(
      this.cssWidth / 2,
      this.cssHeight / 2,
      0,
      this.cssWidth / 2,
      this.cssHeight / 2,
      Math.max(this.cssWidth, this.cssHeight),
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

    if (!ship.canControl && (this.mode === "game-over" || this.lives <= 0)) {
      return;
    }

    if (ship.invulnerableTimer > 0 && Math.floor(ship.invulnerableTimer / 3) % 2 === 0) {
      return;
    }

    // Convert from fixed-point, then interpolate
    const renderX = lerpWrap(fromQ12_4(ship.prevX), fromQ12_4(ship.x), alpha, WORLD_WIDTH);
    const renderY = lerpWrap(fromQ12_4(ship.prevY), fromQ12_4(ship.y), alpha, WORLD_HEIGHT);
    const renderAngle = lerpAngle(BAMToRadians(ship.prevAngle), BAMToRadians(ship.angle), alpha);

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

      // Convert from fixed-point, then interpolate
      const renderX = lerpWrap(
        fromQ12_4(asteroid.prevX),
        fromQ12_4(asteroid.x),
        alpha,
        WORLD_WIDTH,
      );
      const renderY = lerpWrap(
        fromQ12_4(asteroid.prevY),
        fromQ12_4(asteroid.y),
        alpha,
        WORLD_HEIGHT,
      );
      const renderAngle = lerpAngle(
        BAMToRadians(asteroid.prevAngle),
        BAMToRadians(asteroid.angle),
        alpha,
      );

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

      // Convert from fixed-point, then interpolate
      const renderX = lerpWrap(fromQ12_4(bullet.prevX), fromQ12_4(bullet.x), alpha, WORLD_WIDTH);
      const renderY = lerpWrap(fromQ12_4(bullet.prevY), fromQ12_4(bullet.y), alpha, WORLD_HEIGHT);

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

      // Convert from fixed-point, then interpolate
      const renderX = lerpWrap(fromQ12_4(saucer.prevX), fromQ12_4(saucer.x), alpha, WORLD_WIDTH);
      const renderY = lerpWrap(fromQ12_4(saucer.prevY), fromQ12_4(saucer.y), alpha, WORLD_HEIGHT);

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
        particle.size,
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
    ctx.font = "600 20px 'Monaspace Neon', 'Monaspace Krypton', monospace";
    ctx.textBaseline = "top";

    const scoreLabel = `SCORE ${this.score.toString().padStart(5, "0")}`;
    const highLabel = `HIGH ${this.highScore.toString().padStart(5, "0")}`;
    const waveLabel = `WAVE ${Math.max(1, this.wave)}`;

    ctx.fillText(scoreLabel, 20, 18);
    ctx.fillText(highLabel, WORLD_WIDTH - 230, 18);
    ctx.fillText(waveLabel, WORLD_WIDTH - 145, WORLD_HEIGHT - 40);

    // Display seed for ZK verification
    ctx.font = "500 12px 'Monaspace Krypton', monospace";
    ctx.fillStyle = "#6b7280";
    ctx.fillText(`SEED ${this.gameSeed.toString(16).toUpperCase().padStart(8, "0")}`, 20, 44);

    // Draw ship lives as icons instead of text
    this.drawShipLives(ctx, 20, WORLD_HEIGHT - 45, this.lives);

    // Autopilot indicator
    if (this.autopilot.isEnabled()) {
      ctx.save();
      ctx.font = "600 16px 'Monaspace Neon', 'Monaspace Krypton', monospace";
      ctx.shadowBlur = 15;
      ctx.shadowColor = "#22d3ee";
      ctx.fillStyle = "#22d3ee";
      const pulse = 0.7 + Math.sin(this.gameTime * 4) * 0.3;
      ctx.globalAlpha = pulse;
      ctx.fillText("AUTOPILOT", WORLD_WIDTH / 2 - 50, 18);
      ctx.globalAlpha = 1;
      ctx.restore();
    }

    ctx.restore();
  }

  private drawShipLives(
    ctx: CanvasRenderingContext2D,
    startX: number,
    startY: number,
    count: number,
  ): void {
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
      ctx.font = "500 14px 'Monaspace Krypton', monospace";
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

    if (this.mode === "replay") {
      this.drawReplayOverlay(ctx);
      return;
    }

    ctx.save();

    // Vignette effect
    const gradient = ctx.createRadialGradient(
      WORLD_WIDTH / 2,
      WORLD_HEIGHT / 2,
      WORLD_HEIGHT * 0.3,
      WORLD_WIDTH / 2,
      WORLD_HEIGHT / 2,
      WORLD_HEIGHT * 0.8,
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
    ctx.font = "700 56px 'Monaspace Neon', 'Monaspace Krypton', monospace";

    if (this.mode === "menu") {
      // Animated title
      const pulse = 1 + Math.sin(Date.now() * 0.003) * 0.05;
      ctx.save();
      ctx.translate(WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.34);
      ctx.scale(pulse, pulse);
      ctx.fillText("ASTEROIDS", 0, 0);
      ctx.restore();

      ctx.font = "600 24px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      ctx.fillText("Arrow Keys: Turn + Thrust", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.46);
      ctx.fillText("Space: Fire", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.52);
      ctx.fillText("P: Pause  R: Restart", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.58);

      // Autopilot hint in cyan
      ctx.shadowColor = "#22d3ee";
      ctx.fillStyle = "#22d3ee";
      ctx.fillText("A: Toggle Autopilot", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.64);

      // Tape load hint in purple
      ctx.shadowColor = "#a855f7";
      ctx.fillStyle = "#a855f7";
      ctx.fillText("L: Load Replay Tape", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.70);

      ctx.shadowBlur = 10;
      ctx.shadowColor = "#4ade80";
      ctx.fillStyle = "#4ade80";
      ctx.fillText("Press Enter or Tap to Launch", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.78);
    }

    if (this.mode === "paused") {
      ctx.fillText("PAUSED", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.45);
      ctx.font = "600 24px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      if (this.pauseFromHidden) {
        ctx.fillText("Tab hidden: auto-paused", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.56);
      }
      ctx.fillText("Press P / Enter or Tap to Resume", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.66);
    }

    if (this.mode === "game-over") {
      ctx.shadowColor = "#ef4444";
      ctx.fillStyle = "#ff6b6b";
      ctx.fillText("GAME OVER", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.42);
      ctx.font = "600 28px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      ctx.fillStyle = "#d6fff0";
      ctx.fillText(
        `Final Score: ${this.score.toString().padStart(5, "0")}`,
        WORLD_WIDTH * 0.5,
        WORLD_HEIGHT * 0.56,
      );
      ctx.fillText("Press Enter, R, or Tap to Restart", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.64);

      // Tape save hint
      ctx.shadowColor = "#a855f7";
      ctx.fillStyle = "#a855f7";
      ctx.font = "600 24px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      ctx.fillText("D: Save Replay Tape", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.72);
    }

    ctx.restore();
  }

  // =========================================================================
  // Public API (for headless verification, replay, scripts)
  // =========================================================================

  /** Run one simulation step (storePreviousPositions + updateSimulation). */
  stepSimulation(): void {
    this.storePreviousPositions();
    this.updateSimulation(FIXED_TIMESTEP);
  }

  /** Replace the current input source. */
  setInputSource(source: InputSource): void {
    this.inputSource = source;
  }

  getScore(): number {
    return this.score;
  }
  getFrameCount(): number {
    return this.frameCount;
  }
  getRngState(): number {
    return getGameRngState();
  }
  getLives(): number {
    return this.lives;
  }
  getWave(): number {
    return this.wave;
  }
  getMode(): GameMode {
    return this.mode;
  }
  getGameSeed(): number {
    return this.gameSeed;
  }

  /** Build a serialized tape from the current recording. */
  getTape(): Uint8Array | null {
    if (!this.recorder) return null;
    return serializeTape(
      this.gameSeed,
      this.recorder.getInputs(),
      this.score,
      getGameRngState(),
    );
  }

  // =========================================================================
  // Visual Replay
  // =========================================================================

  /** Load a tape and enter visual replay mode. */
  loadReplay(tapeData: Uint8Array): void {
    const tape = deserializeTape(tapeData);
    this.startNewGame(tape.header.seed);
    this.mode = "replay";
    this.replaySpeed = 1;
    this.replayPaused = false;
    this.lastTimeMs = 0;
    this.accumulator = 0;
    const tapeSource = new TapeInputSource(tape.inputs);
    this.replayTapeSource = tapeSource;
    this.inputSource = tapeSource;
    // Stop recording during replay
    this.recorder = null;
  }

  private drawReplayOverlay(ctx: CanvasRenderingContext2D): void {
    const tapeComplete = this.replayTapeSource?.isComplete() ?? false;

    // Show completion overlay with vignette when tape is finished
    if (tapeComplete) {
      ctx.save();

      const gradient = ctx.createRadialGradient(
        WORLD_WIDTH / 2,
        WORLD_HEIGHT / 2,
        WORLD_HEIGHT * 0.3,
        WORLD_WIDTH / 2,
        WORLD_HEIGHT / 2,
        WORLD_HEIGHT * 0.8,
      );
      gradient.addColorStop(0, "rgba(0, 8, 14, 0.6)");
      gradient.addColorStop(1, "rgba(0, 8, 14, 0.9)");
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, WORLD_WIDTH, WORLD_HEIGHT);

      ctx.textAlign = "center";
      ctx.textBaseline = "middle";

      ctx.font = "700 40px 'Monaspace Neon', 'Monaspace Krypton', monospace";
      ctx.shadowBlur = 20;
      ctx.shadowColor = "#a855f7";
      ctx.fillStyle = "#a855f7";
      ctx.fillText("REPLAY COMPLETE", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.38);

      ctx.font = "600 28px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      ctx.shadowBlur = 0;
      ctx.fillStyle = "#d6fff0";
      ctx.fillText(
        `Final Score: ${this.score.toString().padStart(5, "0")}`,
        WORLD_WIDTH * 0.5,
        WORLD_HEIGHT * 0.50,
      );
      ctx.fillText(`Wave: ${this.wave}`, WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.57);

      ctx.font = "500 14px 'Monaspace Krypton', monospace";
      ctx.fillStyle = "#6b7280";
      ctx.fillText(
        `Seed: 0x${this.gameSeed.toString(16).toUpperCase().padStart(8, "0")}`,
        WORLD_WIDTH * 0.5,
        WORLD_HEIGHT * 0.64,
      );

      ctx.font = "600 22px 'Monaspace Krypton', 'SFMono-Regular', monospace";
      ctx.fillStyle = "#4ade80";
      ctx.shadowBlur = 10;
      ctx.shadowColor = "#4ade80";
      ctx.fillText("Press Esc to Exit", WORLD_WIDTH * 0.5, WORLD_HEIGHT * 0.76);

      ctx.restore();
      return;
    }

    ctx.save();

    // "REPLAY" label top-center
    ctx.font = "600 18px 'Monaspace Neon', 'Monaspace Krypton', monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.shadowBlur = 15;
    ctx.shadowColor = "#a855f7";
    ctx.fillStyle = "#a855f7";
    const pulse = 0.7 + Math.sin(this.gameTime * 4) * 0.3;
    ctx.globalAlpha = pulse;
    ctx.fillText("REPLAY", WORLD_WIDTH / 2, 18);
    ctx.globalAlpha = 1;

    if (this.replayTapeSource) {
      const current = this.replayTapeSource.getCurrentFrame();
      const total = this.replayTapeSource.getTotalFrames();

      // Frame counter
      ctx.font = "500 14px 'Monaspace Krypton', monospace";
      ctx.fillStyle = "#d6fff0";
      ctx.shadowBlur = 0;
      ctx.fillText(`Frame ${current} / ${total}`, WORLD_WIDTH / 2, 42);

      // Speed indicator
      const speedLabel = this.replayPaused ? "PAUSED" : `${this.replaySpeed}x`;
      ctx.fillText(speedLabel, WORLD_WIDTH / 2, 60);

      // Progress bar
      const barY = WORLD_HEIGHT - 10;
      const barH = 4;
      const progress = total > 0 ? current / total : 0;

      ctx.fillStyle = "#333";
      ctx.fillRect(20, barY, WORLD_WIDTH - 40, barH);

      ctx.fillStyle = "#a855f7";
      ctx.fillRect(20, barY, (WORLD_WIDTH - 40) * progress, barH);

      // Key hints
      ctx.font = "500 12px 'Monaspace Krypton', monospace";
      ctx.fillStyle = "#6b7280";
      ctx.textAlign = "center";
      ctx.fillText(
        "1/2/4: Speed    Space: Pause    Esc: Exit",
        WORLD_WIDTH / 2,
        WORLD_HEIGHT - 20,
      );
    }

    ctx.restore();
  }

  private downloadTape(): void {
    const tape = this.getTape();
    if (!tape) return;

    const seedHex = this.gameSeed.toString(16).padStart(8, "0");
    const filename = `asteroids-${seedHex}-${this.score}.tape`;

    const blob = new Blob([tape.buffer as ArrayBuffer], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  }

  private triggerFileLoad(): void {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".tape";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return;
      void file.arrayBuffer().then((buf) => {
        this.loadReplay(new Uint8Array(buf));
      });
    });
    input.click();
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
