# Asteroids Game Spec

## Deterministic Gameplay Contract
Game state transitions must be deterministic given:
- initial seed
- per-frame input bytes
- canonical update order

## Input Model
- 4 action bits per frame: `left`, `right`, `thrust`, `fire`.
- High nibble reserved and must be zero.

## Core Mechanics
### Ship
- Rotation uses 8-bit BAM.
- Thrust and drag operate in fixed-point integer space.
- Speed is clamped to configured max.

### Player bullets
- Bullet cap is enforced.
- Fire cooldown is enforced.
- Bullet lifetime is frame-based.

### Asteroids
- Wave-based spawning with cap.
- Size transitions: large -> medium -> small -> removed.
- Split behavior respects live-entity caps.

### Saucers
- Spawn cadence and count scale with progression.
- Anti-lurk behavior increases pressure when player stalls.
- Small saucers aim with bounded error; large saucers are less accurate.

### Scoring and progression
- Score increments only from valid destruction events.
- Extra life threshold uses fixed score step.
- Wave progression triggers only when required entities are cleared.

## Difficulty and Session-Length Policy
This build targets roughly 2-5 minute high-skill runs to keep proving costs
reasonable while preserving arcade pressure.

### Difficulty controls
- Asteroid wave count scales up to a cap of 16.
- Saucer concurrency scales by wave (`1` early, then `2`, then `3`).
- Saucer spawn cadence accelerates with wave and anti-lurk pressure.
- Small-saucer aim tightens with wave progression.
- Asteroid speed increases by wave with an upper cap.
- Anti-lurk threshold is fixed at 6 seconds (`360` frames at 60 FPS).

### Baseline progression constants
- Starting lives: `3`
- Extra life step: `10,000`
- Asteroid score bands: `20 / 50 / 100`
- Saucer score bands: `200 / 1000`

## Deliberate Scope Decisions
- Hyperspace is omitted in this version to keep controls, verification surface,
  and deterministic state space tighter.

## Scope Exclusions (Non-Consensus)
- Visual effects
- Audio behavior
- Rendering interpolation and cosmetic randomness
