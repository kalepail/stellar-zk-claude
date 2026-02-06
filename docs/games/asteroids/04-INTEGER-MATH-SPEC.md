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

## Constants (Consensus-Critical)
- `SHIP_TURN_SPEED_BAM = 3`
- `SHIP_BULLET_LIMIT = 4`
- `SHIP_BULLET_COOLDOWN_FRAMES = 10`
- `SHIP_BULLET_LIFETIME_FRAMES = 51`
- `SHIP_RESPAWN_FRAMES = 75`
- `SHIP_SPAWN_INVULNERABLE_FRAMES = 120`
- `SAUCER_BULLET_LIFETIME_FRAMES = 84`
- `LURK_TIME_THRESHOLD_FRAMES = 360`
- `EXTRA_LIFE_SCORE_STEP = 10000`

## Determinism Rules
- No floating-point operations in consensus-critical updates.
- No wall-clock dependency in simulation transitions.
- No non-deterministic RNG in consensus-critical logic.

## RNG Separation
- `gameplayRng`: affects authoritative state and must be proven.
- `visualRng`: affects cosmetics only and must not influence authoritative state.
