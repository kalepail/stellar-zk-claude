# Variance Resolution Plan (V-03, V-08, V-09, V-10, V-12, V-13, V-14, V-19, V-20, V-21, V-22)

Date: 2026-02-09

## Status Update (2026-02-10)
Implemented from this plan:
- `V-14` small saucer scoring (`990`)
- `V-19` strict asteroid-cap split behavior
- `V-22` saucer-asteroid collision handling
- `V-08` anti-autofire shift-register fire gating
- `V-13` fixed saucer fire reload cadence (`10` frames)
- deterministic open-area respawn with edge padding

Additional implemented parity changes:
- Saucer bullet hard cap set to `2`
- Saucer and ship bullet lifetime set to `72` frames
- Saucer side-entry and immediate X-bound cull (no offscreen X grace margin)
- Saucer spawn/fire paused while ship is not visible

## Objective
Resolve selected variance items while keeping rules:
- easy to understand for bot/autopilot developers
- deterministic and TS/zk-parity-safe
- close to original arcade feel in early waves
- progressively harder after early human-playable waves

## Design Principles (Locked)
- No dramatic rule switches between early/late waves.
- Deterministic simulation only (no nondeterministic randomness).
- Keep mechanics stable; difficulty should scale through smooth parameter ramps.
- Prefer original behavior where practical unless it harms clarity or bot-competition goals.

## Phase Model (Locked)
- `Phase A` (human-friendly): waves `1..4`
- `Phase B` (bot escalation): waves `5+`

This keeps early behavior close to classic while allowing stronger pressure after wave 4.

## Item Decisions

### V-03 Wave large-asteroid ramp (locked)
- Keep early-wave behavior classic for human playability, then ramp smoothly for bot pressure.
- Deterministic rule:
  - if `wave <= 4`: `largeCount = 4 + 2*(wave - 1)` (classic early pattern)
  - if `wave >= 5`: `largeCount = min(16, 10 + (wave - 4))`
- This yields:
  - waves `1..4`: `4, 6, 8, 10`
  - wave `5`: `11`
  - waves `6..10`: `12, 13, 14, 15, 16`
  - wave `11+`: `16`

### V-08 Ship bullet autofire behavior
- Keep anti-autofire gating for all waves (no mode switch to hold-fire).
- Implement original-style edge-triggered fire gating semantics consistently across the run.
- Rule uses 8-bit shift register gate (press now, not pressed previous frame).

### V-09 Ship bullet lifetime
- Move lifetime closer to classic model.
- Keep deterministic timing/cadence (no random lifetime effects).
- Allow only smooth progression if tuned later; no abrupt phase change.

### V-10 Saucer bullet lifetime
- Move closer to classic behavior.
- Keep deterministic and simple; no special-case wave toggles.
- Pair with slot constraints/cadence tuning rather than inflated lifetime.

### V-12 Small saucer accuracy curve
- Early waves should remain less accurate and more human-playable.
- Accuracy increases progressively with wave and anti-lurk pressure.
- Ramp must be smooth and monotonic.

### V-13 Saucer firing cadence
- Base timing should align with classic rhythm.
- Use fixed deterministic reload (`10` frames) instead of randomized windows.
- Keep cadence stable and simple unless future tuning is explicitly requested.

### V-14 Small saucer scoring
- Switch to arcade-faithful scoring target (`990`).
- Keep scoring rule stable across phases.

### V-19 Asteroid-cap enforcement
- Fix edge case so alive asteroid count never exceeds configured cap.
- Keep enforcement hard and deterministic.
- Keep fixed hard cap `27` at all waves (no wave-based cap ramp).

### V-20 Ship spawn survivability
- Keep spawn invulnerability enabled consistently across all waves.
- No phase-specific removal or drastic shrink by default.

### V-21 Spawn location policy
- Replace center-only spawn wait with "most open valid area" spawn selection.
- Respect a screen-edge padding margin so ship does not spawn on edges.
- Goal: avoid long center-clear waits in high-density late waves.

### V-22 Saucer vs asteroid interaction
- Implement original-like saucer-asteroid collision behavior.
- Saucers are destroyed on collision with asteroids.

### Anti-saucer-farming policy (locked)
- Keep rules simple: no extra anti-farm scoring modifier or per-wave saucer budget initially.
- Rely on escalating pressure channels first (`V-03`, `V-12`, anti-lurk/spawn pressure).
- Revisit only if telemetry shows dominant saucer-farming strategies.

## Asteroid Cap Decision Memo (V-19)

### What original did
- Original code defines `MAXNUMAST = 27` and starts split objects at index `0..26`, i.e. a fixed 27-slot asteroid pool.
- Original wave progression uses `AstPerWave` with `4 + 2*(wave-1)` and a cap of `11` large asteroids, so the asteroid field was bounded both by wave spawn and object slots.

Reference lines:
- `MAXNUMAST = 27`: `https://6502disassembly.com/va-asteroids/Asteroids.html` (line 71)
- `MAXASTP1 = MAXNUMAST+1`: `https://6502disassembly.com/va-asteroids/Asteroids.html` (line 72)
- `AstStatus` fixed asteroid status table sized by max slots: `https://6502disassembly.com/va-asteroids/Asteroids.html` (line 212)
- `MAXASTWAVE = 11`: `https://6502disassembly.com/va-asteroids/Asteroids.html` (line 74)

### What current game does
- Uses `ASTEROID_CAP = 27` in TS and zk (`src/game/constants.ts`, Rust constants mirror).
- Uses a modernized wave spawn cap of `16` large asteroids (harder than original `11`).
- Split behavior is now strict: if at cap, split into one child; otherwise two.
- Current invariant: alive asteroid count never exceeds `ASTEROID_CAP`.

### Why current cap exists (practical intent)
- Keeps entity count bounded for deterministic performance and proving cost.
- Prevents "screen full of only asteroids" states from dominating readability.
- Keeps bot-state search space bounded and legible.
- Preserves some original-slot-model feel while still allowing modernized pressure via saucers/speed.

### Recommendation
- Keep a fixed hard asteroid cap of `27` across all waves.
- Fix split logic to enforce a strict invariant: `alive_asteroids <= ASTEROID_CAP` at all times.
- Do not ramp asteroid cap by wave initially.

### Why this is the best fit for your stated goal
- You explicitly want early waves human-playable and late waves harder without confusing rule switches.
- A fixed cap keeps one simple mental model for bot developers.
- Late-wave challenge should come from deterministic pressure ramps (saucer aim/spawn pressure and speed), not from unbounded asteroid clutter.
- This reduces risk of chaotic, low-signal "asteroid soup" while still allowing strong difficulty escalation.

### If endless-running appears in telemetry
- Do not raise cap first.
- First tune existing pressure channels already in scope (`V-12`, anti-lurk timing, spawn pressure).
- Only consider cap ramp (`27 -> 30`) as a last resort after telemetry confirms pressure tuning is insufficient.

## Implementation Blueprint

1. Input/Fire system (`V-08`)
- Implement anti-autofire gate in both:
  - `src/game/AsteroidsGame.ts`
  - `risc0-asteroids-verifier/asteroids-core/src/sim/game.rs`
- Add parity tests proving identical shot spawn frames for fixed tapes.

2. Projectile timing model (`V-09`, `V-10`)
- Convert ship/saucer bullet expiry to deterministic cadence model near classic timing.
- Keep single shared conceptual model in TS + zk constants and update loops.
- Add fixture tests for bullet birth/death frame parity.

3. Wave large-asteroid ramp (`V-03`)
- Replace wave large-count formula in both TS and zk with locked hybrid ramp:
  - `wave <= 4`: `4 + 2*(wave - 1)`
  - `wave >= 5`: `min(16, 10 + (wave - 4))`
- Add progression tests validating exact wave table for waves `1..12`.

4. Saucer behavior updates (`V-12`, `V-13`)
- Keep deterministic accuracy pressure formula for small saucers.
- Use fixed deterministic fire reload (`10` frames) for saucers.
- Add golden trace tests for representative waves (`1, 4, 5, 10, 20`).

5. Scoring (`V-14`)
- Change small saucer score constant to `990` in TS and zk constants.
- Add regression tests for saucer kill score increments.

6. Asteroid cap invariant (`V-19`)
- Modify split logic to guarantee `alive_asteroids <= ASTEROID_CAP` always.
- Update/replace existing edge-case unit test to assert strict cap.

7. Respawn system (`V-20`, `V-21`)
- Keep existing invulnerability window (or tuned constant), applied consistently.
- Introduce deterministic open-area search:
  - candidate points on a fixed grid within padded bounds
  - score candidate by nearest-hazard distance (asteroids, saucers, bullets)
  - pick highest score; deterministic tie-break by fixed ordering
- Add tests for:
  - deterministic spawn selection given fixed state
  - spawn selection not blocked by center congestion

8. Saucer-asteroid collisions (`V-22`)
- Add collision pass between saucers and asteroids in both engines.
- Resolve outcomes deterministically and mirror scoring/effects policy.
- Add parity tests with scripted collision scenario tapes.

9. Documentation and bot clarity
- Update:
  - `docs/games/asteroids/01-GAME-SPEC.md`
  - `docs/games/asteroids/06-IMPLEMENTATION-STATUS.md`
- Include a small "Difficulty Ramp Table" with exact formulas/constants.
- Document that mechanics are stable; only pressure parameters scale.

## Test Plan
- TS/zk parity:
  - existing tape verifiers (`test-short`, `test-medium`)
  - new targeted fixtures for each changed subsystem
- Rust unit tests:
  - cap invariant
  - spawn-point solver determinism
  - saucer-asteroid collision outcomes
- Optional longer soak fixtures:
  - waves `1-6` (human-phase transition behavior)
  - waves `10+` (bot-escalation pressure behavior)

## Final Locked Decisions Snapshot
- `V-03`: hybrid wave large-count ramp (classic waves `1..4`, smooth rise to `16` by wave `10`)
- `V-08`: edge-triggered shift-register fire gate (release required between shots)
- `V-09`: ship bullet lifetime moved near classic, deterministic
- `V-10`: saucer bullet lifetime near classic, deterministic
- `V-12`: small-saucer accuracy scales progressively with wave/lurk
- `V-13`: saucer fire cadence uses fixed deterministic reload (`10` frames)
- `V-14`: small saucer score set to `990`
- `V-19`: fixed hard asteroid cap `27`, strict no-overflow invariant
- `V-20`: spawn invulnerability retained across all waves
- `V-21`: deterministic "most open area" respawn policy with edge padding
- `V-22`: saucer-asteroid collisions added (saucer destroyed on collision)
- Anti-farming: no extra rule initially; tune only if telemetry proves necessary
