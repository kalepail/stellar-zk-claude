# Asteroids Docs Parity Checklist (AST3)

Date: 2026-02-11

## Purpose
Code-backed checklist confirming that Asteroids docs match the current TS/Rust/Worker/Contract implementation.

## Verified Against Code
- Tape format and constants:
  - `src/game/tape.ts`
  - `risc0-asteroids-verifier/asteroids-core/src/tape.rs`
  - `risc0-asteroids-verifier/asteroids-core/src/constants.rs`
- Gameplay rules/math:
  - `src/game/AsteroidsGame.ts`
  - `src/game/constants.ts`
  - `risc0-asteroids-verifier/asteroids-core/src/sim/mod.rs`
  - `risc0-asteroids-verifier/asteroids-core/src/sim/game.rs`
- Proof gateway + prover contract:
  - `worker/api/routes.ts`
  - `worker/prover/client.ts`
  - `risc0-asteroids-verifier/api-server/src/config.rs`
  - `risc0-asteroids-verifier/api-server/src/types.rs`
- On-chain score settlement:
  - `stellar-asteroids-contract/contracts/asteroids_score/src/lib.rs`

## Parity Checks
1. Tape contract
- `magic = 0x5A4B5450`, `version = 2`, `rules_tag = 3 (AST3)`.
- Header reserved bytes `[6..7]` and input high nibble must be zero.
- Length is exact (`header + frameCount + footer`), CRC must match.

2. Deterministic gameplay constants
- `SHIP_RESPAWN_FRAMES = 75`, `SHIP_SPAWN_INVULNERABLE_FRAMES = 120`.
- `SHIP_BULLET_LIMIT = 4`, `SAUCER_BULLET_LIMIT = 2`.
- `SHIP_BULLET_LIFETIME_FRAMES = 72`, `SAUCER_BULLET_LIFETIME_FRAMES = 72`.
- `SCORE_SMALL_SAUCER = 990`.

3. Difficulty/ramp behavior
- Wave asteroids: `4,6,8,10`, then ramps to cap `16`.
- Max concurrent saucers by wave tier: `1` (`<4`), `2` (`4..6`), `3` (`>=7`).
- Saucer fire cadence is pressure-based cooldown ranges (deterministic math + RNG), not fixed reload.

4. Fire gate semantics
- Ship fire is edge-triggered latch + cooldown (`shipFireLatch`/`ship_fire_latch`), not shift-register.

5. Verifier journal/output
- Success journal is 24 bytes / 6 fields:
  - `seed`, `frame_count`, `final_score`, `final_rng_state`, `tape_checksum`, `rules_digest`.
- Rules digest is `0x4153_5433` (`AST3`).

6. Gateway/prover/claim path
- Worker requires `x-claimant-address` on `POST /api/proofs/jobs`.
- Worker submits prover jobs with `receipt_kind=groth16`, `verify_mode=policy`, `segment_limit_po2`.
- Prover `proof_mode` is forced from `RISC0_DEV_MODE` (not request-driven).
- Score contract call remains `submit_score(seal, journal_raw, claimant)`.

## Docs Updated In This Pass
- `docs/games/asteroids/README.md`
- `docs/games/asteroids/01-GAME-SPEC.md`
- `docs/games/asteroids/02-VERIFICATION-SPEC.md`
- `docs/games/asteroids/04-INTEGER-MATH-SPEC.md`
- `docs/games/asteroids/06-IMPLEMENTATION-STATUS.md`
- `docs/games/asteroids/13-ORIGINAL-RULESET-VARIANCE-AUDIT.md`
- `docs/games/asteroids/14-VARIANCE-RESOLUTION-PLAN.md`

## Notes
- `13-ORIGINAL-RULESET-VARIANCE-AUDIT.md` and `14-VARIANCE-RESOLUTION-PLAN.md` include historical planning context. When values conflict, treat `01-GAME-SPEC.md` plus implementation code as canonical.
