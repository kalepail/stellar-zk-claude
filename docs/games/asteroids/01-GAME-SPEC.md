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
- On death, the ship re-enters on the next simulation step with spawn invulnerability.

### Player bullets
- Bullet cap is enforced.
- Fire gating uses an anti-autofire shift register (edge-triggered hold behavior).
- Bullet lifetime is frame-based.
- Ship bullet lifetime is `72` frames.

### Asteroids
- Wave-based spawning with cap.
- Size transitions: large -> medium -> small -> removed.
- Split behavior respects live-entity caps.

### Saucers
- Spawn cadence and count scale with progression (up to `6` concurrent by late wave).
- Anti-lurk behavior increases pressure when player stalls.
- Small saucers aim with bounded error; large saucers are less accurate.
- Saucers only enter from left/right edges and are culled as soon as they leave X bounds.
- Saucer spawn/fire is paused while the ship is not visible.
- Saucer fire cadence is deterministic with fixed reload (`10` frames), no random fire window.
- Saucer bullet hard cap is `2`, with lifetime `72` frames.

### Scoring and progression
- Score increments only from valid destruction events.
- Extra life threshold uses fixed score step.
- Wave progression triggers only when required entities are cleared.

## Difficulty and Session-Length Policy
This build targets roughly 2-5 minute high-skill runs to keep proving costs
reasonable while preserving arcade pressure.

### Difficulty controls
- Asteroid wave count scales up to a cap of 16.
- Saucer concurrency scales by wave (`1`, then `2`, then `3+` in late waves, capped at `6`).
- Saucer spawn cadence accelerates with wave and anti-lurk pressure.
- Saucer firing cadence uses fixed deterministic reload (`10` frames).
- Small-saucer aim tightens with wave progression.
- Asteroid speed increases by wave with an upper cap.
- Anti-lurk threshold is fixed at 6 seconds (`360` frames at 60 FPS).

### Baseline progression constants
- Starting lives: `3`
- Extra life step: `10,000`
- Asteroid score bands: `20 / 50 / 100`
- Saucer score bands: `200 / 990`

## Deliberate Scope Decisions
- Hyperspace is omitted in this version to keep controls, verification surface,
  and deterministic state space tighter.

## Scope Exclusions (Non-Consensus)
- Visual effects
- Audio behavior
- Rendering interpolation and cosmetic randomness
