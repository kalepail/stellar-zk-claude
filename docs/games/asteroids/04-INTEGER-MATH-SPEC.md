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
- `ship_fire_shift_reg = (ship_fire_shift_reg >> 1) | (fire ? 0x80 : 0x00)`
- Ship shot is allowed only when:
  - bit7 is set (fire pressed this frame), and
  - bit6 is clear (fire was not pressed on previous frame).

### Saucer fire cadence
- Saucer fire timer reload is fixed at `SAUCER_FIRE_RELOAD_FRAMES`.
- Current value is `10` frames.

## Constants (Consensus-Critical)
- `SHIP_TURN_SPEED_BAM = 3`
- `SHIP_BULLET_LIMIT = 4`
- `SAUCER_BULLET_LIMIT = 2`
- `SAUCER_FIRE_RELOAD_FRAMES = 10`
- `SHIP_BULLET_LIFETIME_FRAMES = 72`
- `SHIP_RESPAWN_FRAMES = 0`
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
