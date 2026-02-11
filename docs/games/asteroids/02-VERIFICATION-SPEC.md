# Asteroids Deterministic Verification Spec

## Acceptance Criteria
A tape is valid only if all conditions hold:
1. Tape format and checksum are valid.
2. Replay executes exactly `frameCount` frames.
3. Canonical transition order is preserved.
4. All invariant groups pass.
5. Final score and final gameplay RNG state match footer claims.

## Tape Rules
### Header
- `magic == 0x5A4B5450` (`ZKTP`)
- `version == 1`
- `frameCount > 0`
- `frameCount <= configured max` (default 18,000)

### Body
- Exactly `frameCount` bytes.
- Bits `0..3` are control bits.
- Bits `4..7` must be zero.

### Footer
- Contains `finalScore`, `finalRngState`, `checksum`.
- `checksum` must equal CRC32 of header+body.

## Canonical Transition Order
1. Increment frame counter.
2. Read frame input.
3. Update ship.
4. Update asteroids.
5. Update player bullets.
6. Update saucers.
7. Update saucer bullets.
8. Resolve collisions.
9. Prune destroyed entities.
10. Update progression timers.
11. Advance input cursor.
12. Spawn wave when progression conditions are met.

Any reorder is invalid.

## Rule Groups
- `TAPE_*`: parsing, limits, checksum, reserved bits.
- `GLOBAL_*`: frame monotonicity, mode transitions, RNG consistency.
- `SHIP_*`: turn/thrust/drag/clamp/position step.
- `PLAYER_BULLET_*`: cap/fire-gate/spawn/lifetime.
- `ASTEROID_*`: motion/split/caps/wave spawn count.
- `SAUCER_*`: spawn cadence/count/fire behavior.
- `COLLISION_*`: canonical collision order and side effects.
- `PROGRESSION_*`: score deltas, extra life, wave advance, lives/game-over.

## RNG Integrity
- Gameplay RNG algorithm and call sequence are consensus-critical.
- Visual RNG must be isolated and non-authoritative.
- Footer RNG check is required and catches divergence cascades.

## Required Verification Output
- `ok`
- `failFrame`
- `errorCode`
- `message`
- `finalScore`
- `finalRngState`
- `elapsedMs`
