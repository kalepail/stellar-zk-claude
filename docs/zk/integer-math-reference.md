# Integer Math Reference for ZK-Provable Asteroids

## Purpose

This document captures research on best mathematical practices for implementing
deterministic, integer-only game physics suitable for zero-knowledge proof generation.
All gameplay-affecting logic must produce identical results across JavaScript, Rust,
and RISC-V targets. Floating-point math is acceptable only for rendering/visual effects.

---

## Table of Contents

1. [Current State Audit](#1-current-state-audit)
2. [Historical Precedent: Classic Arcade Integer Math](#2-historical-precedent)
3. [Fixed-Point Format Selection](#3-fixed-point-format-selection)
4. [Angle Representation: Binary Angular Measurement](#4-angle-representation)
5. [Integer Trigonometry](#5-integer-trigonometry)
6. [Multiplication & Overflow Management](#6-multiplication--overflow-management)
7. [Division Alternatives](#7-division-alternatives)
8. [Collision Detection](#8-collision-detection)
9. [Drag & Friction Without Floats](#9-drag--friction-without-floats)
10. [RNG Separation Strategy](#10-rng-separation-strategy)
11. [ZK Circuit Cost Model](#11-zk-circuit-cost-model)
12. [Recommended Architecture](#12-recommended-architecture)
13. [Precision vs Efficiency Tradeoff Guide](#13-precision-vs-efficiency-tradeoff-guide)
14. [Migration Checklist](#14-migration-checklist)

---

## 1. Current State Audit

### What's Broken for ZK

Every gameplay-affecting calculation currently uses JavaScript `number` (IEEE 754 float64):

| Component | File | Issue |
|---|---|---|
| All entity positions (`x`, `y`) | `types.ts` Entity | Stored as float |
| All velocities (`vx`, `vy`) | `types.ts` Entity | Stored as float |
| Collision detection | `AsteroidsGame.ts:76-80` | `collisionDistanceSquared()` uses float positions |
| Ship rotation | `AsteroidsGame.ts` updateShip | `Math.cos`/`Math.sin` on float angles |
| Saucer aiming | `AsteroidsGame.ts` saucer bullets | `Math.atan2` for targeting |
| Drag/friction | `AsteroidsGame.ts` updateShip | `Math.pow(0.992, dt*60)` |
| Speed clamping | `AsteroidsGame.ts` updateShip | `Math.hypot(vx, vy)` |
| Timestep | `constants.ts:4` | `1/60` not representable in binary |
| Visual RNG coupling | `AsteroidsGame.ts` spawn* methods | Particles advance gameplay RNG |
| Thrust particle timing | `AsteroidsGame.ts` updateShip | `Date.now()` affects RNG sequence |

### What's Already Correct

- **Xorshift32 RNG core** (`rng.ts`): Uses `>>> 0` for u32 truncation, deterministic
- **Score tracking**: Integer addition
- **No visual-to-logic feedback**: Rendered interpolated positions don't write back to state
- **Fixed-timestep architecture**: Accumulator pattern is sound

---

## 2. Historical Precedent

### Original Atari Asteroids (1979)

The original ran on a 1.5 MHz MOS 6502 (8-bit, no hardware multiply, no FPU):

- **Coordinate system**: 10-bit (0-1023) via the Digital Vector Generator
- **Angles**: 8-bit values (256 steps = 1.406 degrees per step)
- **Rotation**: Small-angle approximation: `x' = x + ay`, `y' = y - ax`
- **Trig**: Lookup tables with 32 entries per quadrant (like BBC Micro's Elite)
- **Velocity damping**: `v - (v >> 8)` approximates multiply by 255/256 (~0.9961)
- **Collision**: Circle-circle with distance-squared comparison (no square root)
- **Multiplication**: Shift-and-add or quarter-square lookup tables

### Key Insight from Classic Games

Games that shipped on constrained hardware prove that 8-bit angles, 4-bit fractional
precision on positions, and lookup-table trig are *more than sufficient* for an
Asteroids-style game. The original game felt great with far less precision than we need.

### BattleZone Math Box (1980)

Atari added a hardware Math Box coprocessor for 3D:
- Used **Q1.15 fixed-point** (signed, 1 sign bit + 15 fractional bits) for sin/cos
  since trig values are always in [-1, 1]
- Distance approximation: `distance ~ 0.375*dx + 1.0*dy` (only multiply + add)
- This is the `max(|dx|,|dy|) + 0.375*min(|dx|,|dy|)` octagonal approximation

### PlayStation 1 (1994)

- 32-bit fixed-point with 12 fractional bits (Q20.12) in the geometry coprocessor
- Characteristic "jittery vertices" came from only 12 bits of fraction - still shipped
  hundreds of successful 3D games

---

## 3. Fixed-Point Format Selection

### The Q Notation

A **Q{m}.{f}** format has `m` integer bits and `f` fractional bits. A value is stored
as an integer that is implicitly divided by `2^f`. For example, in Q12.4:
- The integer 3 is stored as `3 << 4 = 48`
- The value 3.5 is stored as `3.5 * 16 = 56`
- Precision is `1/16 = 0.0625` per step

### Recommended Formats for This Game

Based on our 960x720 world and gameplay parameters:

#### Positions: Q12.4 (16 bits used in 32-bit int)

| Property | Value |
|---|---|
| Integer range | 0 to 4095 (covers 960 and 720 with headroom) |
| Fractional precision | 1/16 pixel = 0.0625 pixels |
| Bits used | 12 + 4 = 16 (sign bit not needed for positions if unsigned) |
| Max value | 4095.9375 |

Why Q12.4 over Q10.6:
- Q10.6 only goes to 1023 — too tight for 960 width
- Q12.4 goes to 4095 — plenty of headroom
- 1/16 pixel precision is finer than any display can render
- Fewer fractional bits = cheaper in ZK circuits (fewer bits to range-check)

#### Velocities: Q8.8 (16 bits used in 32-bit int)

| Property | Value |
|---|---|
| Integer range | -128 to +127 pixels/frame (signed) |
| Fractional precision | 1/256 pixel/frame |
| Per-frame max needed | ~5.67 px/frame (340 px/sec / 60) |
| Fits? | Yes, with large headroom |

Why Q8.8: Max per-frame velocity is ~5.67 pixels. Q8.8 handles up to 127, giving 22x
headroom. The 8 fractional bits give smooth sub-pixel movement accumulation.

#### Angles: 8-bit unsigned integer (BAM)

| Property | Value |
|---|---|
| Steps per full rotation | 256 |
| Degrees per step | 1.406 |
| Radians per step | ~0.02454 |

The original Asteroids used similar precision. Addition wraps naturally with `& 0xFF`.
Rotation is just integer addition — free in ZK circuits.

#### Timers/Cooldowns: Frame counts (integer)

Instead of `0.85 seconds` as a float, store `51 frames` as an integer (0.85 * 60 = 51).
Decrement by 1 each tick. No fractional math needed.

### Conversion Table for Current Constants

| Current Constant | Float Value | Integer Replacement | Format |
|---|---|---|---|
| `FIXED_TIMESTEP` | 1/60 | Eliminated — implicit in per-frame math | N/A |
| `SHIP_TURN_SPEED` | 4.8 rad/s | ~5 BAM units/frame | 8-bit BAM |
| `SHIP_THRUST` | 280 px/s² | ~75 Q8.8/frame² | Q8.8 |
| `SHIP_DRAG` | 0.992 | 254/256 (see Section 9) | Shift trick |
| `SHIP_MAX_SPEED` | 340 px/s | ~1451 in Q8.8 (5.67 * 256) | Q8.8 |
| `SHIP_BULLET_SPEED` | 520 px/s | ~2219 in Q8.8 (8.67 * 256) | Q8.8 |
| `SHIP_BULLET_LIFETIME` | 0.85s | 51 frames | Integer |
| `SHIP_BULLET_COOLDOWN` | 0.16s | 10 frames | Integer |
| `SHIP_RESPAWN_DELAY` | 1.25s | 75 frames | Integer |
| `SHIP_SPAWN_INVULNERABLE` | 2s | 120 frames | Integer |
| `SAUCER_BULLET_SPEED` | 280 px/s | ~1195 in Q8.8 | Q8.8 |
| `SAUCER_BULLET_LIFETIME` | 1.4s | 84 frames | Integer |
| `SAUCER_SPAWN_MIN` | 7s | 420 frames | Integer |
| `SAUCER_SPAWN_MAX` | 14s | 840 frames | Integer |
| `LURK_TIME_THRESHOLD` | 8s | 480 frames | Integer |
| `SHAKE_DECAY` | 0.92 | N/A (visual only) | Float OK |
| `SCANLINE_OPACITY` | 0.08 | N/A (visual only) | Float OK |

---

## 4. Angle Representation

### Binary Angular Measurement (BAM)

BAM represents angles as unsigned integers where overflow = full rotation.

```
8-bit BAM:
  0   = 0 degrees
  64  = 90 degrees
  128 = 180 degrees
  192 = 270 degrees
  256 = 360 degrees (wraps to 0)

Rotation is just addition: new_angle = (angle + delta) & 0xFF
```

**Why BAM is perfect for ZK:**
- Rotation = integer addition (free in ZK circuits)
- Wrapping is automatic with unsigned overflow (free — no modulo needed)
- No need for 2*PI constants or radians
- Ship turn speed becomes an integer (e.g., 5 BAM units per frame ≈ 7°/frame at 60fps)

### Computing Turn Speed

Current: `SHIP_TURN_SPEED = 4.8 rad/s`
Degrees per second: `4.8 * 180/PI ≈ 275°/s`
Degrees per frame: `275 / 60 ≈ 4.58°/frame`
BAM per frame: `4.58 / 1.406 ≈ 3.26`

Use **3 BAM/frame** (4.22°/frame, 253°/s) or **4 BAM/frame** (5.63°/frame, 337°/s).
Either feels good for gameplay — tune by playtesting.

---

## 5. Integer Trigonometry

### Lookup Table Approach (Recommended for This Game)

Store sin values for one quadrant (65 entries for 8-bit BAM, indexed 0-64),
scaled to Q0.14 (values from 0 to 16384 representing 0.0 to 1.0):

```
SIN_TABLE[0]  = 0       // sin(0°)
SIN_TABLE[16] = 6393    // sin(22.5°) ≈ 0.3827 × 16384
SIN_TABLE[32] = 11585   // sin(45°)  ≈ 0.7071 × 16384
SIN_TABLE[48] = 15137   // sin(67.5°) ≈ 0.9239 × 16384
SIN_TABLE[64] = 16384   // sin(90°)  = 1.0    × 16384
```

Full 256-entry table eliminates quadrant logic (costs 512 bytes for sin+cos):

```typescript
// Pre-generate at build time
const SIN_TABLE = new Int16Array(256);
const COS_TABLE = new Int16Array(256);
for (let i = 0; i < 256; i++) {
    SIN_TABLE[i] = Math.round(Math.sin(i * 2 * Math.PI / 256) * 16384);
    COS_TABLE[i] = Math.round(Math.cos(i * 2 * Math.PI / 256) * 16384);
}
```

**Scale factor 16384 (2^14)** chosen because:
- Trig values are in [-1, 1], so Q0.14 gives 14 bits of precision
- After multiplying a Q8.8 velocity by a Q0.14 sin value, the result is
  Q8.22 — shift right by 14 to get back to Q8.8
- Intermediate product fits in 32 bits: 16 + 16 = 32 bits max

### CORDIC Alternative

CORDIC computes sin/cos using only shifts and adds (no multiplication):
```
for each iteration i:
    if z >= 0:
        x_new = x - (y >> i)
        y_new = y + (x >> i)
        z -= arctan_table[i]
    else:
        x_new = x + (y >> i)
        y_new = y - (x >> i)
        z += arctan_table[i]
```

**Tradeoff**: CORDIC needs 14-16 iterations for good precision. For our game with
~20 entities max, a 256-entry lookup table is faster and simpler. CORDIC is better
when memory is extremely constrained (ZK circuit witness size matters more than
computation in some proof systems).

### Recommendation

**Use lookup tables.** 256 entries × 2 bytes = 512 bytes for sin, same for cos.
In ZK circuits, lookup tables can be verified efficiently via lookup arguments
(Plookup etc.) — the cost is proportional to number of lookups, not table size.

---

## 6. Multiplication & Overflow Management

### The Core Problem

When multiplying two fixed-point numbers, fractional bits double:
```
Q12.4 × Q8.8 → result has 4+8 = 12 fractional bits, 12+8 = 20 integer bits
Total: 32 bits + sign = needs 33 bits minimum
```

### Strategy: Use 32-bit Intermediates with Careful Scaling

For position updates (`position += velocity * dt`), since dt is implicit (1 frame),
this is just addition — no overflow concern:
```
new_pos_q12_4 = old_pos_q12_4 + (velocity_q8_8 >> 4)
```
The `>> 4` converts Q8.8 to Q12.4 (shifts away 4 fractional bits).

### When Real Multiplication is Needed

For thrust: `acceleration_component = thrust * cos(angle)`:
```
thrust = Q8.8 value
cos(angle) = Q0.14 from lookup table

product = thrust * cos_value     // Q8.22 (8+0=8 int, 8+14=22 frac)
result = product >> 14           // back to Q8.8

Max intermediate: 127 * 16384 = 2,080,768 — fits in 32-bit signed (max ~2.1 billion)
```

### Overflow Safety Rules

1. **Position × Position** (for distance²): Q12.4 × Q12.4 = Q24.8 → 32 bits. SAFE.
   Max: `(960*16)² = 235,929,600` — fits in 32-bit unsigned.

2. **Velocity × Trig**: Q8.8 × Q0.14 = Q8.22 → 30 bits. SAFE.
   Max: `127*256 * 16384 = 532,676,608` — fits in 32-bit signed.

3. **Distance² for collision**: Two Q12.4 deltas squared and summed:
   Max delta = 960 × 16 = 15,360 in Q12.4 units.
   Max delta² = 15,360² = 235,929,600.
   Max sum = 235,929,600 + 132,710,400 (720×16=11,520; 11,520² = 132,710,400)
   Total max = 368,640,000 — fits in 32-bit unsigned. SAFE.

4. **Danger zone**: Multiplying two Q12.4 positions by each other and then adding.
   Avoid this pattern. Use delta-based math instead.

### For ZK Circuits Specifically

In ZK circuits over finite fields (typically 254-bit prime fields like BN254):
- Overflow is not a concern for individual multiplications — field elements are huge
- But **range checks** (proving a value fits in N bits) cost ~N constraints
- So use the **minimum bit-width** that works: Q12.4 positions need only 16-bit range checks
- Avoid unnecessary widening — keep values narrow to minimize range-check cost

---

## 7. Division Alternatives

Division is expensive everywhere and *especially* in ZK circuits. Avoid it.

### Replace Division with Shift

| Division | Replacement | Error |
|---|---|---|
| `÷ 2` | `>> 1` | Exact |
| `÷ 4` | `>> 2` | Exact |
| `÷ 16` | `>> 4` | Exact |
| `÷ 256` | `>> 8` | Exact |

### Replace Division with Reciprocal Multiply

For constant divisors, precompute the reciprocal as a fixed-point multiplier:

```
÷ 3  →  × 21845 >> 16    (21845/65536 ≈ 0.33333)
÷ 5  →  × 13107 >> 16    (13107/65536 ≈ 0.19999)
÷ 10 →  × 6554  >> 16    (6554/65536  ≈ 0.10001)
÷ 60 →  × 1092  >> 16    (1092/65536  ≈ 0.01666)
```

### In ZK Circuits

Division by a constant `c` in a finite field is multiplication by `c^(-1) mod p`,
which is a single multiplication constraint (free). Division by a variable requires
the prover to supply the quotient and the circuit verifies `a = b * quotient` (also
one constraint, but the prover does more work).

### Speed Clamping Without Division

Current code: `speed = Math.hypot(vx, vy); if (speed > max) { vx = vx/speed*max; ... }`

Integer alternative — use distance² comparison to avoid sqrt entirely:
```
speed_sq = vx*vx + vy*vy   // in Q16.16 (Q8.8 × Q8.8)
max_sq = MAX_SPEED_Q8_8 * MAX_SPEED_Q8_8

if (speed_sq > max_sq) {
    // Apply iterative scaling: halve velocity until under limit
    while (vx*vx + vy*vy > max_sq) {
        vx = (vx * 3) >> 2;  // multiply by 0.75
        vy = (vy * 3) >> 2;
    }
}
```

Or use BattleZone's octagonal approximation for fast magnitude:
```
approx_speed = max(|vx|, |vy|) + (3 * min(|vx|, |vy|)) >> 3
```
This approximates Euclidean distance within ~4% error using only shifts and adds.

---

## 8. Collision Detection

### Current Implementation (Float)

```typescript
function collisionDistanceSquared(ax, ay, bx, by) {
    const dx = shortestDelta(ax, bx, WORLD_WIDTH);
    const dy = shortestDelta(ay, by, WORLD_HEIGHT);
    return dx * dx + dy * dy;
}
```

### Integer Replacement

The structure is already correct — distance² avoids square root. Just change the
types to fixed-point:

```typescript
function collisionDistSq_i(ax: i32, ay: i32, bx: i32, by: i32): u32 {
    // All values in Q12.4
    let dx = shortestDelta_i(ax, bx, WORLD_WIDTH_Q12_4);
    let dy = shortestDelta_i(ay, by, WORLD_HEIGHT_Q12_4);
    // Result in Q24.8 (Q12.4 × Q12.4)
    return (dx * dx + dy * dy) as u32;
}

function shortestDelta_i(from: i32, to: i32, size: i32): i32 {
    let delta = to - from;
    const half = size >> 1;  // Exact since sizes are multiples of 16
    if (delta > half) delta -= size;
    if (delta < -half) delta += size;
    return delta;
}
```

### Overflow Analysis

Max delta in Q12.4: `960 * 16 = 15,360`
Max delta²: `15,360² = 235,929,600`
Max sum (dx² + dy²): `235,929,600 + 132,710,400 = 368,640,000`

**Fits in 32-bit unsigned** (max 4,294,967,295). No 64-bit needed.

### Collision Radii

Store radii in Q12.4 format. Collision threshold is `(r1 + r2)²`:

| Entity | Current Radius | Q12.4 Value |
|---|---|---|
| Large asteroid | 48 | 768 |
| Medium asteroid | 28 | 448 |
| Small asteroid | 16 | 256 |
| Ship | 14 | 224 |
| Player bullet | 2 | 32 |
| Saucer (large) | 28 | 448 |
| Saucer (small) | 16 | 256 |
| Saucer bullet | 2 | 32 |

Example threshold: ship (224) + large asteroid (768) = 992
Threshold²: 992² = 984,064 — easily fits in 32-bit.

### Ship-Asteroid Collision Fudge Factor

Current: `asteroid.radius * 0.88` — this makes the ship hitbox feel fair.

Integer replacement: `(radius * 225) >> 8` — this is `225/256 ≈ 0.879`, close enough.
Or `(radius * 7) >> 3` — this is `7/8 = 0.875`, even simpler.

---

## 9. Drag & Friction Without Floats

### The Problem

Current: `SHIP_DRAG = 0.992`, applied as `Math.pow(0.992, dt * 60)`

Per-frame: `velocity *= 0.992`

### Integer Solution: Multiply-and-Shift

```
velocity_next = (velocity * 254) >> 8
```

This computes `velocity * (254/256) = velocity * 0.9921875`

| Method | Factor | Error vs 0.992 | After 60 frames |
|---|---|---|---|
| `0.992` exact | 0.992000 | — | 0.6176 |
| `254/256` | 0.992188 | +0.019% | 0.6238 |
| `253/256` | 0.988281 | -0.375% | 0.4926 |
| `127/128` | 0.992188 | +0.019% | 0.6238 |
| `1016/1024` | 0.992188 | +0.019% | 0.6238 |
| `255/256` | 0.996094 | +0.412% | 0.7899 |

**`254/256` or equivalently `127/128` are the best match.**

Implementation:
```typescript
// Method 1: shift trick
vx = (vx * 254) >> 8;   // Q8.8 * 254 fits in 24 bits, >> 8 returns to Q8.8

// Method 2: subtract a fraction (equivalent, may be faster)
vx = vx - (vx >> 7);    // vx - vx/128 = vx * 127/128
```

Method 2 uses only a shift and subtract — extremely cheap in ZK circuits.

### Accumulated Error Analysis

After N frames, ship velocity V₀ decays to:

| Frames | Exact (0.992^N) | 127/128 (0.992188^N) | Error |
|---|---|---|---|
| 60 | 0.6176 | 0.6238 | +1.0% |
| 120 | 0.3814 | 0.3891 | +2.0% |
| 300 | 0.0899 | 0.0948 | +5.5% |

The error is small and only affects how quickly the ship decelerates — imperceptible
in gameplay. The exact value doesn't matter as long as it's consistent across all
implementations.

---

## 10. RNG Separation Strategy

### Current Problem

One `SeededRng` instance (`gameRng`) is shared between:
1. **Gameplay-critical calls**: asteroid spawn, saucer behavior, wave pacing
2. **Visual-only calls**: particle spawning, debris shapes, explosion colors

If particle counts differ (e.g., `MAX_PARTICLES` cap hit), subsequent gameplay
random values shift — breaking determinism.

### Solution: Two RNG Instances

```
gameplayRng    — used ONLY for gameplay decisions
visualRng      — used ONLY for particles, debris, screen effects
```

Both seeded from the game seed, but with different initial states:
```
gameplayRng = new SeededRng(gameSeed)
visualRng   = new SeededRng(gameSeed ^ 0x12345678)  // Different stream
```

### What Goes Where

**Gameplay RNG** (deterministic, goes into ZK proof):
- Asteroid spawn positions and velocities
- Asteroid split directions
- Saucer spawn timing and type (small/large)
- Saucer aim accuracy
- Wave-level pacing randomness
- Saucer drift direction changes

**Visual RNG** (non-deterministic OK, not in proof):
- Particle positions, velocities, lifetimes, colors
- Debris vertex shapes
- Explosion particle counts
- Muzzle flash particles
- Thrust flame particles
- Star twinkle patterns
- Screen shake offset (`Math.random()` currently — move to visualRng)

### Eliminate Date.now() from Simulation

Current thrust particle timing uses `Date.now()` — replace with frame counter:
```
if (this.frameCount % 3 === 0) spawnThrustParticles();  // Every 3rd frame
```

---

## 11. ZK Circuit Cost Model

### Operation Costs (approximate constraints per operation)

| Operation | Cost | Notes |
|---|---|---|
| Addition | ~0 | Folded into other constraints |
| Multiplication (two variables) | 1 | Single R1CS constraint |
| Multiplication by constant | ~0 | Free in most proof systems |
| Comparison (`<`, `>`) | ~N | N = bit width of operands |
| Range check (N bits) | ~N | One constraint per bit |
| Bitwise AND/OR/XOR | ~N | Requires bit decomposition |
| Right shift by constant | ~N | Requires bit decomposition |
| Division by variable | 1 | Prover computes quotient, circuit verifies |
| Lookup table access | ~log(table_size) | With Plookup-style arguments |
| Poseidon hash | ~200-300 | ZK-optimized hash function |
| SHA-256 | ~25,000 | Expensive — avoid in circuits |

### Implications for Our Game

**Cheap operations** to use freely:
- Addition/subtraction for position updates
- Multiplication for thrust, trig scaling
- Distance-squared for collisions

**Expensive operations** to minimize:
- Comparisons: each collision check needs one comparison (~16-32 constraints)
- Range checks: each position/velocity needs bounds verification
- Bit shifts: used in our drag calculation and fixed-point scaling

### Estimated Circuit Size per Game Tick

Assuming 5 asteroids, 1 ship, 4 bullets, 1 saucer:

| Component | Count | Constraints Each | Total |
|---|---|---|---|
| Position updates (pos += vel) | 11 entities × 2 axes | ~2 | ~44 |
| Velocity updates (thrust, drag) | 2 entities | ~20 | ~40 |
| Trig lookups (thrust direction) | 2 | ~10 | ~20 |
| Collision checks | ~30 pairs | ~40 | ~1,200 |
| Wrapping (modulo world size) | 22 | ~16 | ~352 |
| RNG advances | ~8 calls | ~10 | ~80 |
| State hash | 1 | ~250 | ~250 |
| **Total** | | | **~2,000** |

This is well within feasibility for modern proof systems (provable in <1 second).

---

## 12. Recommended Architecture

### Layer Separation

```
┌─────────────────────────────────────────┐
│              Rendering Layer             │  ← Float math OK
│  - Visual interpolation (lerpWrap)       │  - Uses prevX/prevY → x/y
│  - Particle rendering                    │  - Screen shake, CRT effects
│  - Score display, UI                     │  - Canvas/WebGL transforms
├─────────────────────────────────────────┤
│            Visual Effects Layer          │  ← Float math OK
│  - Particle spawning (visualRng)         │  - Debris, explosions
│  - Screen shake calculation              │  - Star twinkle
│  - Sound triggers                        │
├─────────────────────────────────────────┤
│          Game Logic Layer (ZK)           │  ← INTEGER MATH ONLY
│  - Position updates                      │  - Q12.4 positions
│  - Velocity updates with drag            │  - Q8.8 velocities
│  - Collision detection                   │  - Distance-squared
│  - Ship rotation (BAM angles)            │  - 8-bit angles
│  - Bullet spawning                       │  - Trig lookup tables
│  - Saucer AI                             │  - Integer decisions
│  - Scoring                               │  - Pure integer
│  - Game state transitions                │  - RNG: gameplayRng only
│  - Wrapping/bounds                       │  - Integer modulo
└─────────────────────────────────────────┘
```

### Data Flow

```
Game Logic Layer (integers) ──→ copy to prevX/prevY ──→ Rendering Layer (floats)
         ↑                                                        │
    Player Input                                          Visual output only
    (integer actions:                                     (no feedback to
     turn_left, turn_right,                                game logic)
     thrust, fire, pause, restart)
```

### State Representation for ZK

```typescript
interface ZKGameState {
    // All positions in Q12.4
    shipX: u16;
    shipY: u16;
    shipAngle: u8;       // 8-bit BAM
    shipVX: i16;         // Q8.8 signed
    shipVY: i16;
    shipAlive: u8;
    shipInvulnFrames: u16;
    shipRespawnFrames: u16;
    shipFireCooldown: u8;

    asteroids: Array<{
        x: u16;          // Q12.4
        y: u16;
        vx: i16;         // Q8.8
        vy: i16;
        size: u8;        // 0=large, 1=medium, 2=small
        alive: u8;
    }>;

    bullets: Array<{
        x: u16;          // Q12.4
        y: u16;
        vx: i16;         // Q8.8
        vy: i16;
        framesLeft: u8;
        alive: u8;
    }>;

    saucer: {
        x: u16;
        y: u16;
        vx: i16;
        vy: i16;
        alive: u8;
        small: u8;
        fireCooldown: u8;
        driftTimer: u16;
    };

    score: u32;
    lives: u8;
    level: u8;
    rngState: u32;       // Xorshift32 state
    frameCount: u32;
}
```

---

## 13. Precision vs Efficiency Tradeoff Guide

### What Needs High Precision

| Component | Why | Recommended |
|---|---|---|
| Position accumulation | Small velocities accumulate over many frames | Q12.4 (4 frac bits) |
| Velocity after thrust | Trig precision affects direction feel | Q8.8 (8 frac bits) |
| Trig lookup values | Directly multiplied with velocity | Q0.14 (14 frac bits) |

### What Needs Low Precision (Cheaper in ZK)

| Component | Why | Recommended |
|---|---|---|
| Angles | 256 steps feels fine for Asteroids | 8-bit BAM |
| Collision radii | Just need rough circles | Q12.4 (matches positions) |
| Timers/cooldowns | Frame counting is exact | Plain integer |
| Score | Already integer | Plain integer |
| Drag factor | ~1% error is imperceptible | 254/256 shift trick |

### What Doesn't Need Precision At All (Visual Only)

| Component | Current | Recommendation |
|---|---|---|
| Particle positions/velocities | Float | Keep float, use visualRng |
| Screen shake | `Math.random()` | Keep float, use visualRng |
| CRT effects | Float | Keep float |
| Star twinkle | Float | Keep float |
| Render interpolation | Float | Keep float |
| Sound parameters | Float | Keep float |

### The 80/20 Rule for ZK Games

Most ZK circuit cost comes from:
1. **Collision detection comparisons** (~60% of constraints)
2. **Range checks on state values** (~20% of constraints)
3. **Everything else** (~20% of constraints)

To optimize: reduce collision check count (spatial partitioning), and use minimum
bit widths for all values (narrow values = cheaper range checks).

---

## 14. Migration Checklist

### Phase 1: Separate Concerns (No Gameplay Change)
- [ ] Split RNG into `gameplayRng` and `visualRng`
- [ ] Move all particle/debris/effect RNG calls to `visualRng`
- [ ] Replace `Date.now()` in thrust particles with frame counter
- [ ] Convert time-based constants to frame counts
- [ ] Add `frameCount` to game state

### Phase 2: Integer Game Logic
- [ ] Define fixed-point types and conversion utilities
- [ ] Generate sin/cos lookup tables (256 entries, Q0.14)
- [ ] Convert Entity positions to Q12.4
- [ ] Convert Entity velocities to Q8.8
- [ ] Convert angles to 8-bit BAM
- [ ] Rewrite `collisionDistanceSquared` with integer math
- [ ] Rewrite `shortestDelta` with integer math
- [ ] Rewrite ship movement (thrust via lookup, drag via shift)
- [ ] Rewrite bullet spawning with integer trig
- [ ] Rewrite saucer AI with integer targeting
- [ ] Rewrite speed clamping without `Math.hypot`

### Phase 3: Rendering Bridge
- [ ] Add float conversion layer: `renderX = stateX / 16.0` (Q12.4 → float)
- [ ] Keep `prevX`/`prevY` interpolation in float (visual only)
- [ ] Verify visual appearance matches pre-migration

### Phase 4: ZK Circuit Preparation
- [ ] Define ZKGameState structure with exact bit widths
- [ ] Implement game tick function as pure function: `(state, input) → state`
- [ ] Verify determinism: same seed + same inputs = identical state sequence
- [ ] Profile constraint count for a single game tick
- [ ] Committed game seed (not `Date.now()`)

---

## References

### Classic Game Math
- Jed Margolin, "The Secret Life of Vector Generators" (DVG technical deep-dive)
- 6502 Asteroids Disassembly: https://6502disassembly.com/va-asteroids/
- BattleZone Math Box: https://6502disassembly.com/va-battlezone/mathbox.html
- Elite BBC trig tables: https://elite.bbcelite.com/deep_dives/the_sine_cosine_and_arctan_tables.html

### Fixed-Point Arithmetic
- Q format: https://en.wikipedia.org/wiki/Q_(number_format)
- Fixed-point math tutorial: https://vanhunteradams.com/FixedPoint/FixedPoint.html
- CORDIC: https://en.wikipedia.org/wiki/CORDIC
- Sine approximations: https://www.coranac.com/2009/07/sines/

### ZK Circuit Design
- "17 Misconceptions about SNARKs": https://a16zcrypto.com/posts/article/17-misconceptions-about-snarks/
- Compute-Then-Constrain pattern: https://rareskills.io/post/compute-then-constrain
- Arithmetization schemes: https://blog.lambdaclass.com/arithmetization-schemes-for-zk-snarks/
- Lookup arguments: https://blog.lambdaclass.com/lookups/

### ZK Games
- Dark Forest architecture: https://www.ingonyama.com/oldblogs/cryptographic-fog-of-war
- Deterministic physics (Box2D): https://box2d.org/posts/2024/08/determinism/
- Autonomous Worlds: https://www.bitkraft.vc/insights/fully-onchain-games-thesis/
- Deterministic fixed-point library: https://github.com/nilpunch/fixed-point
