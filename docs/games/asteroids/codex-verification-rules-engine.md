# Codex Verification Rules Engine Spec (Asteroids)

Date: 2026-02-05  
Status: Proposed implementation spec (decision-complete for v1 strict verifier)

## 1. Goal

Guarantee, with deterministic replay, that a submitted Asteroids run was played under canonical game rules and cannot pass verification if it used illegal bot behavior (teleporting, illegal fire rate, score forgery, wave skipping, etc.).

This spec defines a **consensus-critical, full-replay verifier**:
- Input: tape (`seed + per-frame inputs + claimed final score/RNG + CRC`).
- Process: replay every frame with canonical simulation and frame-level rule checks.
- Output: accept/reject with exact first failing frame and rule code.

## 2. Current Repository State (Ground Truth)

### Existing verification capabilities
- `scripts/verify-tape.ts`
  - Deserializes tape and verifies CRC via `deserializeTape`.
  - Replays headless simulation frame-by-frame.
  - Compares final score and final RNG state only.

### Existing deterministic gameplay core
- `src/game/AsteroidsGame.ts`
  - Fixed step simulation with deterministic integer math for gameplay-critical logic.
  - Input source abstraction supports live and tape replay.
  - Headless mode avoids rendering/audio side-effects.
- `src/game/constants.ts`
  - Gameplay constants already expressed in frame counts and fixed-point-friendly integers.
- `src/game/rng.ts`, `src/game/math.ts`
  - Xorshift32 gameplay RNG with explicit seed/state.
  - Separate visual RNG stream (`seed ^ 0x12345678`).

### Gap this spec closes
- No standalone frame-level rules engine.
- No structured rejection reasons (beyond final mismatch).
- No explicit validator rule catalog tied to code-level invariants.
- No worker API endpoint for strict verification submissions.

## 3. Threat Model and Fairness Definition

### Adversary capabilities
- Can generate arbitrary tapes (including malformed bytes).
- Can run custom bots and attempt to encode impossible outcomes.
- Can attempt to forge final score/footer fields.
- Can attempt state transitions not reachable from canonical rules.

### Adversary non-capabilities
- Cannot change verifier code/constants.
- Cannot bypass deterministic replay on verifier side.
- Cannot forge CRC for altered header/body without recomputation (CRC is integrity, not cryptographic authentication).

### Fairness definition (v1)
A run is fair iff:
1. Tape structure and body are valid.
2. Starting from canonical initial state seeded with tape seed, applying each frame input under canonical transition order yields only valid state transitions.
3. Final score and final gameplay RNG state match tape footer.

If any rule fails, tape is rejected.

## 4. Scope and Non-Goals

### In scope (consensus-critical)
- Ship movement/turn/thrust/drag/speed clamp.
- Firing rules and bullet limits/cooldowns/lifetimes.
- Asteroid/saucer spawn and movement rules.
- Collision outcomes and destruction ordering.
- Scoring, lives, extra life thresholds, wave progression.
- RNG-driven gameplay transitions and deterministic replay.

### Out of scope (non-consensus)
- Rendering interpolation and visual transforms.
- Particle/debris/star/screen-shake details.
- Audio behavior.
- Browser input event timing details outside recorded frame inputs.

## 5. Canonical Input and Tape Rules

Tape format source: `src/game/tape.ts`.

### Header rules
- `magic == 0x5A4B5450` (`"ZKTP"`).
- `version == 1`.
- `frameCount` must satisfy strict limits:
  - `frameCount > 0`
  - `frameCount <= maxFrames` (default `18_000` in verifier config).

### Body rules
- Exactly `frameCount` bytes.
- Bits `0..3` are inputs: left/right/thrust/fire.
- Bits `4..7` must be `0` (reserved; reject otherwise).

### Footer rules
- Contains `finalScore`, `finalRngState`, `checksum`.
- `checksum == CRC32(header + body)`.

### Deterministic replay setup
- Initialize `AsteroidsGame({ headless: true, seed })`.
- Call `startNewGame(seed)`.
- Set `TapeInputSource(inputs)`.
- Run exactly `frameCount` steps.

## 6. Canonical Simulation Transition Order (Must Match Exactly)

Per `AsteroidsGame.updateSimulation`:
1. `frameCount++`
2. Read input for current frame (`getFrameInput`)
3. Record input byte (`TapeRecorder`)
4. `updateShip`
5. `updateAsteroids`
6. `updateBullets`
7. `updateSaucers`
8. `updateSaucerBullets`
9. (visual updates skipped in headless)
10. `handleCollisions`
11. `pruneDestroyedEntities`
12. `timeSinceLastKill++`
13. `inputSource.advance()`
14. If mode in `{playing,replay}` and no asteroids/saucers alive -> `spawnWave`

Verifier checks must reason about this exact order, not an equivalent reordered model.

## 7. Rule Catalog (v1 Strict)

Rule IDs are grouped by domain for deterministic reject diagnostics.

## 7.1 Tape and frame ingestion rules (`TAPE_*`)

- `TAPE_MAGIC_INVALID`: wrong magic.
- `TAPE_VERSION_UNSUPPORTED`: unsupported version.
- `TAPE_TRUNCATED`: not enough bytes for declared frame count.
- `TAPE_CRC_MISMATCH`: checksum mismatch.
- `TAPE_FRAMECOUNT_OUT_OF_RANGE`: frame count outside configured bounds.
- `TAPE_RESERVED_BITS_NONZERO`: any input byte has high nibble set.

## 7.2 Global transition rules (`GLOBAL_*`)

- `GLOBAL_FRAMECOUNT_MONOTONIC`: frame count must increase by exactly 1 per step.
- `GLOBAL_MODE_TRANSITION_INVALID`: mode changes must follow canonical game logic.
- `GLOBAL_RNG_STATE_DRIFT`: gameplay RNG state diverges from canonical path.
- `GLOBAL_TIMESINCELASTKILL_INVALID`: increments by 1 each frame unless reset by player asteroid kill.

## 7.3 Ship control and physics rules (`SHIP_*`)

Source constants:
- `SHIP_TURN_SPEED_BAM = 3`
- `SHIP_THRUST_Q8_8 = 20`
- `SHIP_MAX_SPEED_SQ_Q16_16 = 1451*1451`
- Drag from `applyDrag`

Rules:
- `SHIP_TURN_RATE_INVALID`:
  - angle delta must match input:
    - left only: `-3 mod 256`
    - right only: `+3 mod 256`
    - both/neither: net `0`
- `SHIP_THRUST_APPLICATION_INVALID`:
  - acceleration added only when thrust true.
- `SHIP_DRAG_APPLICATION_INVALID`:
  - drag applied every controllable frame.
- `SHIP_SPEED_CLAMP_INVALID`:
  - post-clamp speed must not exceed max.
- `SHIP_POSITION_STEP_INVALID`:
  - `x,y` update must be `(prev + (vx>>4))` with wrap (`wrapXQ12_4`,`wrapYQ12_4`).
- `SHIP_CONTROL_WHILE_RESPAWNING`:
  - no control movement/firing when `canControl=false`.
- `SHIP_RESPAWN_POLICY_INVALID`:
  - respawn attempts only through `trySpawnShipAtCenter` conditions.
- `SHIP_INVULN_TIMER_INVALID`:
  - decrements exactly as implemented; set to `SHIP_SPAWN_INVULNERABLE_FRAMES` on successful spawn.

## 7.4 Ship firing and bullets (`PLAYER_BULLET_*`)

Source constants:
- `SHIP_BULLET_LIMIT = 4`
- `SHIP_BULLET_COOLDOWN_FRAMES = 10`
- `SHIP_BULLET_LIFETIME_FRAMES = 51`
- `SHIP_BULLET_SPEED_Q8_8 = 2219`

Rules:
- `PLAYER_BULLET_LIMIT_EXCEEDED`: active player bullets cannot exceed limit.
- `PLAYER_BULLET_COOLDOWN_BYPASS`: spawn only when cooldown `<=0`.
- `PLAYER_BULLET_SPAWN_KINEMATICS_INVALID`:
  - spawn point offset from ship nose (`radius+6`).
  - spawn velocity uses base + ship-speed boost formula.
- `PLAYER_BULLET_LIFETIME_INVALID`:
  - life decrements by 1 per frame and dies at `<=0`.
- `PLAYER_BULLET_POSITION_STEP_INVALID`:
  - movement step and wrap must match canonical update.

## 7.5 Asteroid rules (`ASTEROID_*`)

Source constants:
- speed ranges `ASTEROID_SPEED_Q8_8`
- cap `ASTEROID_CAP = 27`

Rules:
- `ASTEROID_MOVE_INVALID`: position update from velocity with wrapping.
- `ASTEROID_SPIN_INVALID`: angle update uses spin with BAM wrap.
- `ASTEROID_SPAWN_WAVE_COUNT_INVALID`:
  - wave spawn large count `min(16, 4 + (wave-1)*2)`.
- `ASTEROID_SPAWN_SAFE_RADIUS_INVALID`:
  - initial spawn policy around ship center must follow safe-distance retry logic.
- `ASTEROID_SPLIT_INVALID`:
  - large -> medium, medium -> small, small -> removed.
  - split count: `1` if alive asteroid count already at/above cap else `2`.
  - child velocity inheritance `(parent.v * 46)>>8`.

## 7.6 Saucer and saucer bullet rules (`SAUCER_*`)

Source constants:
- spawn range frames: `SAUCER_SPAWN_MIN_FRAMES`, `SAUCER_SPAWN_MAX_FRAMES`
- anti-lurk thresholds: `LURK_TIME_THRESHOLD_FRAMES`, `LURK_SAUCER_SPAWN_FAST_FRAMES`
- speeds: `SAUCER_SPEED_LARGE_Q8_8`, `SAUCER_SPEED_SMALL_Q8_8`
- bullet lifetime/speed: `SAUCER_BULLET_LIFETIME_FRAMES`, `SAUCER_BULLET_SPEED_Q8_8`

Rules:
- `SAUCER_TIMER_INVALID`: spawn timer decrement and reroll range must be valid.
- `SAUCER_COUNT_CAP_INVALID`:
  - max saucers by wave:
    - waves `<4`: 1
    - waves `<7`: 2
    - else: 3
- `SAUCER_SPAWN_PROFILE_INVALID`:
  - entry side, size selection probability branch, start coordinates, initial cooldown/drift ranges.
- `SAUCER_MOVE_INVALID`:
  - x no-wrap movement, y wrap movement.
- `SAUCER_OFFSCREEN_CULL_INVALID`:
  - cull if x beyond `[-80, WORLD_WIDTH+80]`.
- `SAUCER_DRIFT_TIMER_INVALID`:
  - drift timer countdown and reset range `[48,120)`.
- `SAUCER_FIRE_COOLDOWN_INVALID`:
  - cooldown decrement/reset ranges must match branch (small/large + lurking/non-lurking).
- `SAUCER_BULLET_SPAWN_INVALID`:
  - small saucer aimed shot uses `atan2BAM` + bounded random error.
  - large saucer random angle in `[0,256)`.
- `SAUCER_BULLET_LIFETIME_OR_MOVE_INVALID`:
  - same lifetime/movement consistency checks as player bullets.

## 7.7 Collision and destruction rules (`COLLISION_*`)

Rules:
- `COLLISION_ORDER_MISMATCH`:
  - collision loops must be resolved in canonical order:
    1. player bullet vs asteroid
    2. saucer bullet vs asteroid
    3. player bullet vs saucer
    4. ship collisions (if controllable and not invulnerable)
- `COLLISION_DISTANCE_INVALID`:
  - hit thresholds must use canonical fixed-point distance checks and radii math.
- `DESTRUCTION_SIDE_EFFECT_INVALID`:
  - associated side effects (score updates, splits, ship death state changes) must match event type.

## 7.8 Score, lives, progression rules (`PROGRESSION_*`)

Source constants:
- asteroid scores: 20/50/100
- saucer scores: 200/1000
- `EXTRA_LIFE_SCORE_STEP = 10000`
- `STARTING_LIVES = 3`

Rules:
- `PROGRESSION_SCORE_DELTA_INVALID`:
  - score may increase only by legal event values.
- `PROGRESSION_EXTRA_LIFE_INVALID`:
  - life increment and threshold bump only on crossing `nextExtraLifeScore`.
- `PROGRESSION_WAVE_ADVANCE_INVALID`:
  - wave increments only when both asteroids and saucers are empty and mode allows.
- `PROGRESSION_LIVES_UNDERFLOW_INVALID`:
  - lives transitions only through canonical ship death/extra life rules.
- `PROGRESSION_GAME_OVER_INVALID`:
  - game-over transition when lives <= 0 (except replay mode behavior).

## 8. Deterministic State Snapshot for Rule Checks

Add a verifier-facing snapshot (public read-only) to avoid brittle private-field access:

Suggested type file: `src/game/verification/types.ts`

```ts
export interface DeterministicStateSnapshot {
  frameCount: number;
  mode: "menu" | "playing" | "paused" | "game-over" | "replay";
  score: number;
  lives: number;
  wave: number;
  nextExtraLifeScore: number;
  timeSinceLastKill: number;
  saucerSpawnTimer: number;
  rngState: number;
  ship: {
    x: number; y: number; vx: number; vy: number; angle: number;
    canControl: boolean; fireCooldown: number; respawnTimer: number; invulnerableTimer: number;
  };
  asteroids: Array<{ id:number; x:number; y:number; vx:number; vy:number; angle:number; size:"large"|"medium"|"small"; alive:boolean; radius:number; spin:number; }>;
  bullets: Array<{ id:number; x:number; y:number; vx:number; vy:number; angle:number; life:number; alive:boolean; radius:number; fromSaucer:boolean; }>;
  saucers: Array<{ id:number; x:number; y:number; vx:number; vy:number; angle:number; alive:boolean; radius:number; small:boolean; fireCooldown:number; driftTimer:number; }>;
  saucerBullets: Array<{ id:number; x:number; y:number; vx:number; vy:number; angle:number; life:number; alive:boolean; radius:number; fromSaucer:boolean; }>;
}
```

Suggested game API addition:
- `AsteroidsGame.getDeterministicStateSnapshot(): DeterministicStateSnapshot`

## 9. Rules Engine and Verifier Interfaces

### Engine API

Suggested files:
- `src/game/verification/rules.ts`
- `src/game/verification/FrameRulesEngine.ts`

Core interfaces:

```ts
export type VerificationErrorCode = string;

export interface FrameViolation {
  code: VerificationErrorCode;
  frame: number;
  message: string;
  expected?: unknown;
  actual?: unknown;
}

export interface VerificationResult {
  ok: boolean;
  failFrame: number | null;
  violations: FrameViolation[];
  finalScore: number;
  finalRngState: number;
  elapsedMs: number;
}
```

### Strict verifier runner

New script: `scripts/verify-tape-rules.ts`

Process:
1. Deserialize and pre-validate tape.
2. Initialize headless game at seed.
3. For each frame:
   - snapshot `prev`
   - execute one simulation step
   - snapshot `next`
   - run all frame rules
   - on first violation: fail immediately with structured output.
4. After final frame, verify footer score + RNG.
5. Emit machine-readable summary for CI/backend usage.

Keep existing `scripts/verify-tape.ts` for backward compatibility but treat this strict runner as canonical for fairness verification.

## 10. Worker API (Submission Verification)

Current worker is placeholder (`worker/index.ts`).  
Add endpoint:
- `POST /api/verify-tape`

Request:
- raw binary tape or base64 JSON payload.
- optional config: `{ maxFrames?: number, strict?: boolean }`.

Response:
- `{ ok, failFrame, errorCode, message, finalScore, finalRngState, elapsedMs }`

Behavior:
- Strict mode defaults to `true`.
- Reject oversized/invalid payload early.
- Return deterministic error codes for client handling.

## 11. Test Plan

No automated tests currently exist in repo; add a verifier-focused suite.

### 11.1 Positive tests
- Autopilot-generated valid tape passes strict verification.
- Multiple deterministic seeds produce stable pass outcomes.

### 11.2 Negative tests (tamper/malformed)
- bad magic/version
- truncated body/footer
- CRC mismatch
- reserved bits set in input bytes
- footer score mismatch
- footer RNG mismatch

### 11.3 Fault-injection rule tests
Add a harness that mutates post-step snapshots (or simulator wrapper) to assert detection:
- forced ship teleport
- forced angle jump > allowed turn
- forced bullet count > limit
- forced cooldown bypass
- forced illegal score increment
- forced illegal wave advance

### 11.4 Regression fixtures
- Golden tape fixtures (seed/frameCount/final score/RNG) committed for determinism checks.

## 12. Performance and Operational Defaults

- Default max frames: `18_000` (~5 minutes).
- Verification strategy: correctness over latency.
- Early-exit on first violation for efficiency and clear diagnostics.
- Headless replay required to isolate consensus-critical logic from visuals.

## 13. Future-Proofing for Chunked/Recursive Proofs (Not v1)

v1 is full replay.  
Design now for future chunking:
- Add optional checkpoint schema every `K` frames:
  - frame index
  - compact state commitment hash
  - RNG state
- Preserve stable serialization of deterministic state.
- Allow recursive proof layering later without changing rule semantics.

## 14. Implementation Sequence (Recommended)

1. Add this doc and align team on canonical rule set.
2. Add deterministic snapshot API and verification types.
3. Implement frame rules engine and strict script.
4. Add malformed/tamper/fault-injection tests.
5. Add worker verification endpoint.
6. Make strict verifier the default CI path for tape validity.

## 15. Assumptions and Defaults Locked by This Spec

- Verification model: full frame-by-frame replay first.
- Rule strictness: consensus-critical only.
- Canonical logic source: `src/game/AsteroidsGame.ts` + `src/game/constants.ts`.
- No tape format bump required in v1.
- Visual effects and audio remain explicitly out of verification scope.
