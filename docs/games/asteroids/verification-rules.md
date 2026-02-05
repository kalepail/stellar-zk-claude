# Verification Rules Engine: Ensuring Fair Play

## The Core Architecture Question

Before cataloging rules, we need to understand **why the current architecture is
already stronger than it might appear**, and where the real risks lie.

### Why Inputs-Only Tapes Are Powerful

The tape contains ONLY:
- A seed (u32)
- A sequence of input bytes (4 booleans per frame: left, right, thrust, fire)

The verifier creates a fresh game engine instance with the seed, feeds those
exact inputs frame by frame, and checks that the final state matches. **The
player never directly controls velocity, position, score, or any game state.**
Everything is computed deterministically by the engine.

This means a cheater CANNOT:
- Give themselves extra points (score is computed by collision events)
- Teleport (position is computed from velocity, which comes from thrust)
- Shoot faster than cooldown allows (engine enforces fire cooldown)
- Have more bullets than the limit (engine checks `bullets.length`)
- Skip waves (engine only advances when all asteroids + saucers are gone)
- Be invulnerable forever (engine decrements timers)
- Move faster than max speed (engine clamps velocity)

**The only degree of freedom is which of 4 buttons to press each frame.** A bot
can play optimally, but it must play within the rules.

### Where The Real Risks Are

1. **Engine bugs**: If the game engine has a bug that allows impossible states,
   the verifier has the same bug. An exploit in the engine is an exploit in
   verification.

2. **ZK circuit mismatch**: The ZK circuit must implement the EXACT same state
   transition function as the TypeScript engine. Any deviation means a tape
   verified in JS could fail in ZK or vice versa.

3. **Non-determinism leaks**: If any gameplay code path uses non-deterministic
   operations (Math.random, floating point with rounding differences across
   platforms), replays could diverge.

4. **Tape format attacks**: Malformed tapes, reserved bits set, etc.

### Defense in Depth: Why We Want Explicit Rule Checks

Even though the engine-replay approach inherently enforces rules, adding explicit
invariant checks provides:

- **Safety net**: Catches engine bugs before they become exploits
- **ZK specification**: Each rule becomes a constraint the circuit must enforce
- **Debugging**: When verification fails, pinpoints WHICH rule was violated
- **Audit trail**: Documents every assumption the game depends on

---

## Complete Rule Catalog

Every rule below must be enforced by the ZK circuit. Rules are organized by the
order they're evaluated each frame.

### Category 1: Initialization (Frame 0 Setup)

These are checked once at the start of verification.

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| I-1  | RNG seeded from tape header seed | `setGameSeed(seed)` | RNG state after init = xorshift32(seed) |
| I-2  | Starting lives = 3 | `startNewGame()` | `lives === STARTING_LIVES` |
| I-3  | Starting score = 0 | `startNewGame()` | `score === 0` |
| I-4  | Starting wave = 0 (incremented to 1 by spawnWave) | `startNewGame()` | `wave === 0` before spawnWave |
| I-5  | Ship at center: x=7680, y=5760 (Q12.4) | `createShip()` | `ship.x === toQ12_4(480)`, `ship.y === toQ12_4(360)` |
| I-6  | Ship facing up: angle = 192 BAM | `createShip()` | `ship.angle === 192` |
| I-7  | Ship invulnerable for 120 frames | `createShip()` | `ship.invulnerableTimer === 120` |
| I-8  | No bullets, saucers, or saucer bullets | `startNewGame()` | All arrays empty |
| I-9  | nextExtraLifeScore = 10000 | `startNewGame()` | `nextExtraLifeScore === EXTRA_LIFE_SCORE_STEP` |
| I-10 | frameCount = 0 | `startNewGame()` | `frameCount === 0` |
| I-11 | First wave spawned (4 large asteroids) | `spawnWave()` | `asteroids.length === 4` after init |
| I-12 | Asteroids avoid ship center (safeDist 180px) | `spawnWave()` | All asteroid distance from center > 2880 Q12.4 |

### Category 2: Input Validation (Per Frame)

| ID   | Rule | Check |
|------|------|-------|
| V-1  | Input byte uses only bits 0-3 | `(inputByte & 0xF0) === 0` |
| V-2  | Frame count matches tape length | `frameCount === tape.header.frameCount` at end |
| V-3  | One input byte consumed per frame, always | Cursor advances exactly once per frame |

### Category 3: Ship Physics (Per Frame, when `canControl === true`)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| S-1  | Turn speed: exactly +-3 BAM per frame | `updateShip()` L822-826 | `abs(newAngle - oldAngle) ∈ {0, 3, 253}` (mod 256) |
| S-2  | Thrust acceleration: cosBAM * 20 >> 14, sinBAM * 20 >> 14 | `updateShip()` L830-833 | Velocity delta matches trig table lookup |
| S-3  | Drag: v = v - (v >> 7) each frame | `applyDrag()` | `new_v === old_v - (old_v >> 7)` |
| S-4  | Speed clamp: vx*vx + vy*vy <= 2105401 | `clampSpeedQ8_8()` | Speed-squared never exceeds SHIP_MAX_SPEED_SQ_Q16_16 |
| S-5  | Position update: x += vx >> 4, then wrap | `updateShip()` L855-856 | Position follows velocity exactly |
| S-6  | Position wraps: 0 <= x < 15360, 0 <= y < 11520 | `wrapXQ12_4()` | Positions always in bounds |
| S-7  | Fire cooldown decrements by 1 per frame | `updateShip()` L800 | `cooldown_new = max(0, cooldown_old - 1)` |
| S-8  | Cannot fire if cooldown > 0 | `updateShip()` L849 | No bullet spawned when cooldown > 0 |
| S-9  | Cannot fire if bullets.length >= 4 | `updateShip()` L849 | `bullets.length <= SHIP_BULLET_LIMIT` always |
| S-10 | Firing sets cooldown = 10 | `updateShip()` L851 | On fire: cooldown = SHIP_BULLET_COOLDOWN_FRAMES |
| S-11 | Invulnerable timer decrements by 1 | `updateShip()` L812 | Timer counts down correctly |

### Category 4: Ship State Machine (Per Frame, when `canControl === false`)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| D-1  | Respawn timer decrements by 1 | `updateShip()` L803 | Timer counts down |
| D-2  | When timer hits 0, attempt spawn at center | `trySpawnShipAtCenter()` | Spawn only if area clear |
| D-3  | Spawn requires clear area (check vs asteroids, saucers, saucer bullets) | `isShipSpawnAreaClear()` | Clear radius = 1920 Q12.4 (120px) |
| D-4  | On respawn: invulnerableTimer = 120 | `trySpawnShipAtCenter()` L721 | Always set on respawn |
| D-5  | On respawn: velocity reset to 0 | `queueShipRespawn()` L673 | vx = vy = 0 |
| D-6  | Ship inputs are still read+recorded while dead | `updateSimulation()` L602 | Tape has bytes for dead frames too |

### Category 5: Bullet Physics (Per Frame)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| B-1  | Bullet life decrements by 1 per frame | `updateBullets()` L949 | `life_new = life_old - 1` |
| B-2  | Bullet dies when life <= 0 | `updateBullets()` L951 | Dead bullets pruned |
| B-3  | Bullet starting life = 51 frames | `spawnShipBullet()` L919 | `life === SHIP_BULLET_LIFETIME_FRAMES` |
| B-4  | Bullet position: x += vx >> 4, wraps | `updateBullets()` L956-957 | Follows velocity |
| B-5  | Bullet spawns at ship + offset | `spawnShipBullet()` L896-898 | Start position from displaceQ12_4 |
| B-6  | Bullet velocity = ship velocity + base | `spawnShipBullet()` L901-905 | Inherits ship momentum |
| B-7  | Max 4 player bullets at any time | `updateShip()` L849 | `bullets.length <= 4` always |

### Category 6: Asteroid Rules (Per Frame)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| A-1  | Asteroid position: x += vx >> 4, wraps both axes | `updateAsteroids()` L937-938 | Standard motion |
| A-2  | Asteroid angle: (angle + spin) & 0xFF | `updateAsteroids()` L939 | Bounded BAM |
| A-3  | Asteroid radii fixed: large=48, medium=28, small=16 | `ASTEROID_RADIUS_BY_SIZE` | Never changes |
| A-4  | Large destroys into 2 medium | `destroyAsteroid()` L1257-1267 | Correct split |
| A-5  | Medium destroys into 2 small | `destroyAsteroid()` L1257-1267 | Correct split |
| A-6  | Small destroys into nothing | `destroyAsteroid()` L1253-1255 | No children |
| A-7  | Split capped at 1 when alive >= 27 | `destroyAsteroid()` L1258-1259 | ASTEROID_CAP enforced |
| A-8  | Child velocity inherits: += (parent.v * 46) >> 8 | `destroyAsteroid()` L1264-1265 | Exact formula |
| A-9  | Speed has wave multiplier | `createAsteroid()` L760 | `speed += (speed * min(128, (wave-1)*15)) >> 8` |
| A-10 | Asteroid speed within range for size | `createAsteroid()` L757 | Within ASTEROID_SPEED_Q8_8[size] |

### Category 7: Saucer Rules (Per Frame)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| U-1  | saucerSpawnTimer decrements by 1 | `updateSaucers()` L980 | Counts down |
| U-2  | Max saucers: wave<4:1, wave<7:2, else:3 | `updateSaucers()` L986 | Count enforced |
| U-3  | Anti-lurk: timeSinceLastKill > 360 triggers fast spawn | `updateSaucers()` L983-984 | Lurking behavior |
| U-4  | Saucer X does NOT wrap (exits screen) | `updateSaucers()` L1004 | No wrapX on saucer |
| U-5  | Saucer dies at x < Q12_4(-80) or x > Q12_4(960+80) | `updateSaucers()` L1008-1010 | Off-screen death |
| U-6  | Saucer Y wraps | `updateSaucers()` L1005 | wrapYQ12_4 applied |
| U-7  | Saucer drift: random vy change every 48-120 frames | `updateSaucers()` L1014-1018 | Timer-based drift |
| U-8  | Small saucer aims at ship with error | `spawnSaucerBullet()` L1071-1082 | atan2BAM + random error |
| U-9  | Large saucer fires randomly | `spawnSaucerBullet()` L1084-1085 | Random angle |
| U-10 | Saucer bullet lifetime = 84 frames | `spawnSaucerBullet()` L1105 | Fixed lifetime |
| U-11 | Saucer bullet speed = 1195 Q8.8 | `spawnSaucerBullet()` L1088 | Fixed speed |

### Category 8: Collision Rules (Per Frame)

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| C-1  | Distance check: squared, in Q12.4, with wrap | `collisionDistSqQ12_4()` | Correct toroidal distance |
| C-2  | Hit distance: (radius_a + radius_b) << 4 then squared | `handleCollisions()` | Correct threshold |
| C-3  | Ship-asteroid fudge: asteroid radius * 225/256 | `handleCollisions()` L1188 | 0.88x factor |
| C-4  | Ship cannot be hit when invulnerable (timer > 0) | `handleCollisions()` L1178 | Guard check |
| C-5  | Ship cannot be hit when not controllable | `handleCollisions()` L1178 | Guard check |
| C-6  | Bullet dies on collision (one-hit) | `handleCollisions()` various | `bullet.alive = false` |
| C-7  | Saucer dies on player bullet hit | `handleCollisions()` L1164 | `saucer.alive = false` |
| C-8  | Saucer bullet can destroy asteroids (no score) | `handleCollisions()` L1133-1148 | awardScore=false |

### Category 9: Scoring Rules

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| P-1  | Large asteroid = 20 pts | `destroyAsteroid()` L1231 | Exact value |
| P-2  | Medium asteroid = 50 pts | `destroyAsteroid()` L1233 | Exact value |
| P-3  | Small asteroid = 100 pts | `destroyAsteroid()` L1235 | Exact value |
| P-4  | Large saucer = 200 pts | `handleCollisions()` L1165 | Exact value |
| P-5  | Small saucer = 1000 pts | `handleCollisions()` L1165 | Exact value |
| P-6  | Score only from player bullets, never saucer bullets | `destroyAsteroid(_, false)` | awardScore flag |
| P-7  | Score only increases, never decreases | `addScore()` | Monotonic |
| P-8  | Extra life at every 10000 points | `addScore()` L1297-1299 | EXTRA_LIFE_SCORE_STEP |
| P-9  | Score cannot be awarded while ship is dead | Collision guard at L1178 | No hits while dead |

### Category 10: Wave Progression Rules

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| W-1  | Wave advances when asteroids=0 AND saucers=0 | `updateSimulation()` L631-636 | Both arrays empty |
| W-2  | Wave increments by exactly 1 | `spawnWave()` L726 | `wave_new = wave_old + 1` |
| W-3  | Asteroid count: min(16, 4 + (wave-1)*2) | `spawnWave()` L729 | Formula enforced |
| W-4  | Wave 1 = 4 asteroids, wave 2 = 6, ..., wave 7+ = 16 | Derived from W-3 | Capped at 16 |
| W-5  | timeSinceLastKill resets on wave start | `spawnWave()` L727 | Reset to 0 |

### Category 11: Life/Death Rules

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| L-1  | Ship death: lives -= 1 | `destroyShip()` L1272 | Exactly 1 life lost |
| L-2  | Game over when lives <= 0 | `destroyShip()` L1282 | Mode transitions |
| L-3  | Respawn delay = 75 frames | `destroyShip()` L1271 | SHIP_RESPAWN_FRAMES |
| L-4  | No respawn when lives = 0 | `destroyShip()` L1287 | respawnTimer = 99999 |
| L-5  | Extra life awards are cumulative | `addScore()` L1297 | while loop, not if |

### Category 12: RNG Integrity

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| R-1  | Xorshift32: x^=x<<13; x^=x>>>17; x^=x<<5; state=x>>>0 | `SeededRng.next()` | Exact algorithm |
| R-2  | nextInt(max) = next() % max | `SeededRng.nextInt()` | Modulo operation |
| R-3  | nextRange(min, max) = min + nextInt(max - min) | `SeededRng.nextRange()` | Offset + modulo |
| R-4  | Every RNG call in exact order across all game systems | All callers | Call sequence preserved |
| R-5  | Visual RNG is separate, never affects gameplay | `visualRng` | Different instance |
| R-6  | Final RNG state must match tape footer | Footer check | Catches ANY divergence |

### Category 13: Anti-Lurking Rules

| ID   | Rule | Engine Location | Invariant |
|------|------|-----------------|-----------|
| K-1  | timeSinceLastKill increments each frame | `updateSimulation()` L625 | Monotonic until reset |
| K-2  | Resets to 0 on player asteroid kill | `destroyAsteroid()` L1228 | Only player kills |
| K-3  | Lurking threshold = 360 frames (6s) | `LURK_TIME_THRESHOLD_FRAMES` | Fixed constant |
| K-4  | When lurking: saucers spawn faster | `updateSaucers()` L993-995 | Fast spawn timer |
| K-5  | When lurking: small saucer more likely (90%) | `spawnSaucer()` L1039 | Higher probability |
| K-6  | When lurking: saucer aim improves | `spawnSaucerBullet()` L1078 | Smaller error BAM |

---

## RNG Call Sequence (Critical for ZK)

The order of RNG calls is the most subtle and important thing to get right.
One misplaced call shifts ALL subsequent randomness. The exact call sequence
per frame is:

### On `spawnWave()`:
1. For each asteroid (4 + wave-scaled):
   - `randomInt(0, WORLD_WIDTH_Q12_4)` - x position
   - `randomInt(0, WORLD_HEIGHT_Q12_4)` - y position
   - (may repeat up to 20x for safe distance)
   - `randomInt(0, 256)` - move angle (BAM)
   - `randomInt(minSpeed, maxSpeed)` - speed
   - `randomInt(0, 256)` - start angle
   - `randomInt(-3, 4)` - spin

### On `startNewGame()` after `spawnWave()`:
2. `randomInt(spawnMin, spawnMax)` - initial saucerSpawnTimer

### Each frame in `updateSaucers()`:
3. If saucer spawns:
   - `getGameRng().next()` - enter from left/right
   - `getGameRng().next()` - small/large
   - `randomInt(y_min, y_max)` - start Y
   - `randomInt(-94, 95)` - initial vy
   - `randomInt(18, 48)` - initial fire cooldown
   - `randomInt(48, 120)` - initial drift timer
   - `randomInt(spawnMin, spawnMax)` - next spawn timer
4. For each alive saucer:
   - If driftTimer expires: `randomInt(48, 120)` + `randomInt(-163, 164)`
   - If fireCooldown expires: `randomInt(cooldown range)` + bullet spawn RNG

### On saucer bullet spawn:
5. If small: `randomInt(-errorBAM, errorBAM+1)` for aim error
6. If large: `randomInt(0, 256)` for random angle

### On asteroid destroy (if not small):
7. For each child (1 or 2):
   - Same RNG sequence as `createAsteroid()`

---

## Current Verification: What We Have

The current `verify-tape.ts` does:

1. Deserialize tape, verify CRC-32 (integrity, not authentication)
2. Create headless game with tape's seed
3. Feed tape inputs frame by frame through the engine
4. Compare final score and RNG state against tape footer

**This is already rule-enforcing** because the engine IS the rules. The tape
contains only inputs; everything else is computed. A cheater cannot craft inputs
that violate physics because the physics are applied by the verifier, not the
submitter.

**The RNG state check is the master check.** Because every random event in the
game (asteroid spawn positions, saucer behavior, spawn timers) depends on the
RNG, and each call shifts the state, the final RNG state is effectively a hash
of the entire game history. One wrong collision, one missed spawn, one skipped
RNG call would cascade into a completely different final RNG state.

---

## What The Rules Engine Adds

The rules engine adds **frame-by-frame invariant checking** on top of the
engine replay. This serves as:

1. **Defense in depth**: Catches engine bugs the replay alone wouldn't surface
2. **Debugging tool**: When a tape fails, tells you exactly WHICH rule broke
3. **ZK circuit specification**: Each check maps 1:1 to a circuit constraint
4. **Test harness**: Can run against known-good and known-bad tapes

### Invariant Checks Per Frame

After each frame, the rules engine checks:

```
SHIP STATE:
- Position in bounds: 0 <= x < 15360, 0 <= y < 11520
- Angle in bounds: 0 <= angle <= 255
- Speed within limit: vx*vx + vy*vy <= 2105401
- If canControl: position moved by exactly vx>>4, vy>>4 (with wrap)
- If !canControl: position unchanged
- fireCooldown >= 0
- invulnerableTimer >= 0
- respawnTimer >= 0

BULLETS:
- Count <= 4 (player bullets)
- Each bullet life >= 0
- Each bullet position in bounds
- Each bullet moved by exactly vx>>4, vy>>4

ASTEROIDS:
- Each alive asteroid has valid size (large/medium/small)
- Each asteroid radius matches its size
- Each asteroid position in bounds
- Score awarded matches destroyed asteroid size

SAUCERS:
- Count <= maxSaucers for current wave
- Each saucer has valid size flag

SCORING:
- Score only increases
- Score delta is 0 or a valid score value (20, 50, 100, 200, 1000)
- Score delta only occurs when a collision happened

LIVES:
- Lives only decrease by collision or increase by score threshold
- Game over only when lives = 0

WAVES:
- Wave only advances when field is clear
- Wave increments by exactly 1
- Correct number of asteroids spawned

RNG:
- State matches expected xorshift32 sequence (strongest check)
```

### Implementation Plan

**Phase 1: State snapshot + comparison framework**
- After each `stepSimulation()`, capture full game state snapshot
- Compare consecutive snapshots to verify transitions
- Run on all existing test tapes to establish baseline

**Phase 2: Invariant checks**
- Implement each invariant from the tables above as a check function
- Checks return `{ passed: boolean, rule: string, detail: string }`
- Run all checks after every frame during verification
- First failure halts verification with diagnostic info

**Phase 3: Adversarial testing**
- Create intentionally broken tapes (modify bytes in valid tapes)
- Verify that the rules engine catches specific violations
- Create "almost valid" tapes that exercise edge cases

**Phase 4: ZK circuit mapping**
- Map each invariant check to a circuit constraint
- Verify the TypeScript checks match the circuit behavior
- Cross-validate: same tape, same result in both JS and circuit

---

## Attack Vectors and Mitigations

| Attack | Feasibility | Mitigation |
|--------|-------------|------------|
| Craft inputs that score high | Easy, this is just botting | Allowed by design - tape proves fair play, not human play |
| Modify tape bytes to change score | Impossible | CRC-32 catches corruption; replay catches score mismatch |
| Forge tape with fake footer | Impossible | Replay recomputes score + RNG; must match |
| Exploit engine bug for impossible score | Possible | Rules engine catches invariant violations |
| Float precision exploit | Impossible | All gameplay is integer arithmetic |
| RNG prediction for optimal play | Possible, acceptable | RNG is deterministic from seed - optimal play is valid play |
| Inject extra RNG calls | Impossible | RNG call sequence is fixed by engine code |
| Skip frames / duplicate frames | Impossible | Frame count in header must match body length |
| Reserved input bits set | Catchable | V-1 check: `(byte & 0xF0) === 0` |

---

## Irrelevant to Verification (Visual Only)

These elements exist in the engine but are NOT part of the ZK proof and do NOT
affect game state:

- Particles (spark, smoke, glow) - visual RNG only
- Debris - visual RNG only
- Screen shake - uses `Math.random()`, visual only
- Star twinkling - purely cosmetic
- Asteroid vertices - visual RNG, cosmetic shape
- Interpolation alpha - rendering only
- Audio - no effect on state
- HUD rendering - display only
- `nextId` counter - never used in gameplay logic
- `thrustParticleTimer` - increments in gameplay but only triggers visual code
- `gameTime` - used for animations only

---

## Summary: The Verification Stack

```
Layer 1: Tape Format Validation
  - Magic bytes, version, CRC-32, size consistency
  - Input byte validation (reserved bits)

Layer 2: Deterministic Replay (CURRENT)
  - Same engine, same seed, same inputs → same outputs
  - Final score + RNG state comparison
  - ALREADY catches 99.9% of tampering

Layer 3: Frame-by-Frame Invariant Checks (PROPOSED)
  - Per-frame state validation against all rules
  - Catches engine bugs and edge case exploits
  - Maps directly to ZK circuit constraints
  - Diagnostic output on first violation

Layer 4: ZK Circuit (FUTURE)
  - Circuit IS the state transition function
  - Proof generation: "I know inputs that produce this score"
  - On-chain verification: succinct proof check
```

Each layer adds defense. Layer 2 is already implemented and strong. Layer 3 is
the proposed rules engine. Layer 4 is the ultimate goal.
