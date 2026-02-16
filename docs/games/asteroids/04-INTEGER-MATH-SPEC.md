# Asteroids Integer Math Spec

## Purpose
Define deterministic numeric rules used by gameplay and verification.

## Canonical Formats
| Domain | Format |
|---|---|
| Position | `Q12.4` |
| Velocity | `Q8.8` |
| Angle | `u8` BAM |
| Trig table | `Q0.14` |
| Timers/Cooldowns | integer frames |

## Canonical Operations
### Position step
- `pos_q12_4 += (vel_q8_8 >> 4)` then wrapped per world bounds.

### Thrust
- `vel += (thrust_q8_8 * trig_q0_14) >> 14`

### Drag
- `vel = vel - (vel >> 7)`

### Speed clamp
- Compare squared velocity against fixed threshold in consistent scale.

### Collision
- Use wrapped shortest-delta squared distance.
- Compare against squared collision threshold.

### Ship fire gate (anti-autofire)
- Edge-triggered latch + cooldown model:
  - `fire_pressed_this_frame = fire && !ship_fire_latch`
  - Shot allowed only when `fire_pressed_this_frame && ship_fire_cooldown <= 0 && bullets < SHIP_BULLET_LIMIT`
  - After fire: `ship_fire_cooldown = SHIP_BULLET_COOLDOWN_FRAMES`
  - Latch update tracks hold behavior (`ship_fire_latch = fire` in the steady-state path).

### Saucer fire cadence
- Saucer fire cadence uses pressure-based cooldown ranges (deterministic integer math + game RNG), not a fixed reload constant.
- Higher pressure (wave + anti-lurk) reduces the min/max cooldown window.

## Constants (Consensus-Critical)
- `SHIP_TURN_SPEED_BAM = 3`
- `SHIP_BULLET_LIMIT = 4`
- `SAUCER_BULLET_LIMIT = 2`
- Small-saucer fire cooldown base/floor ranges: `[42..68] -> [22..40]` under max pressure
- Large-saucer fire cooldown base/floor ranges: `[66..96] -> [36..56]` under max pressure
- `SHIP_BULLET_LIFETIME_FRAMES = 72`
- `SHIP_BULLET_COOLDOWN_FRAMES = 10`
- `SHIP_RESPAWN_FRAMES = 75`
- `SHIP_SPAWN_INVULNERABLE_FRAMES = 120`
- `SAUCER_BULLET_LIFETIME_FRAMES = 72`
- `LURK_TIME_THRESHOLD_FRAMES = 360`
- `EXTRA_LIFE_SCORE_STEP = 10000`

## Determinism Rules
- No floating-point operations in consensus-critical updates.
- No wall-clock dependency in simulation transitions.
- No non-deterministic RNG in consensus-critical logic.

## RNG Separation
- `gameplayRng`: affects authoritative state and must be proven.
- `visualRng`: affects cosmetics only and must not influence authoritative state.
