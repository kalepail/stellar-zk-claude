# RISC0 Initial Circuit Build (Codex)

Date: 2026-02-06  
Branch: `feature/codex-initial-risco`

## Goal

First implementation pass for a RISC Zero proof pipeline that verifies Asteroids replay tapes (`seed + per-frame inputs + claimed final score/RNG + CRC`) against deterministic game simulation rules.

## What Was Implemented

- New workspace: `risc0-asteroids-verifier/`
- Official CLI scaffold: `cargo risczero new ...` using `cargo-risczero 3.0.5` and `r0vm 3.0.5`.
- Shared core crate: `risc0-asteroids-verifier/asteroids-core/`
  - Tape parsing/validation (`magic/version/reserved bytes/frame bounds/input reserved bits/CRC`).
  - Deterministic fixed-point math (`Q12.4`, `Q8.8`, `BAM`, lookup trig, xorshift32).
  - Deterministic replay engine for ship/asteroids/bullets/saucers/collisions/scoring/progression.
  - Strict verification API that compares computed final score/RNG against tape footer.
- Guest: `risc0-asteroids-verifier/methods/guest/src/main.rs`
  - Reads `GuestInput`, runs strict verifier, commits typed `VerificationJournal`.
- Host: `risc0-asteroids-verifier/host/src/main.rs`
  - CLI for `--tape`, `--max-frames`, optional `--journal-out`.
  - Proves guest execution and verifies receipt against `VERIFY_TAPE_ID`.
- JS bridge script: `scripts/verify-tape-risc0.ts`
  - Runs the Rust host verifier from existing JS workflow (dev mode default, `--real` for real proof).

## Test Coverage Added

- Unit tests in core crate:
  - CRC32 known vector.
  - Tape roundtrip.
  - Determinism sanity check.
  - Reserved input bits rejection.
  - Footer tamper detection.
- Integration tests in core crate:
  - `test-fixtures/test-short.tape`
  - `test-fixtures/test-medium.tape`
  - copied Downloads fixture: `test-fixtures/from-downloads-asteroids-19c2fc80c3b-16270.tape`

## Validation Runs

- `cargo test -p asteroids-verifier-core` passed.
- Dev-mode proof run (medium fixture) passed.
- Real proof run (short fixture) passed.
- Dev-mode run for Downloads tape passed.

## Rule-Set Mapping

This implementation follows the deterministic transition order and rule categories documented in:

- `docs/games/asteroids/verification-rules.md`
- `docs/games/asteroids/codex-verification-rules-engine.md`
- `docs/games/asteroids/integer-math-reference.md`

It explicitly implements and checks:

- Tape structural and CRC integrity rules.
- Input nibble constraints.
- Fixed-point ship movement, drag, clamp, fire cooldown/limit/lifetime.
- Asteroid spawn/split/motion constraints and caps.
- Saucer spawn/cooldown/drift/shot logic with anti-lurking influences.
- Collision ordering domains and resulting score/life transitions.
- Final score and final RNG-state equality with tape claims.

## Remaining Gaps Before “100% Fairness Assurance”

- This pass is a strong initial circuit implementation, but not yet final-hardening complete.
- The Rust simulation should be further parity-tested frame-by-frame against TypeScript state snapshots (not just footer score/RNG).
- Additional explicit rule-code diagnostics (first-failing-frame + stable error code taxonomy) should be committed to guest journal.
- Add property/fuzz tests for malformed tapes and adversarial input streams.
- Add deterministic snapshot hashing checkpoints for future chunked/recursive proof evolution.
- Add on-chain receipt settlement integration path once verifier contract flow is finalized.

## Why This Is Still Valuable

This establishes an end-to-end proving host/guest environment with deterministic replay and strict tape validation, grounded in the existing research docs and rule model, and provides a production-shaped base to harden toward full consensus-grade fairness guarantees.
