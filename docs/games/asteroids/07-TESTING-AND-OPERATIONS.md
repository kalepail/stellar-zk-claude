# Asteroids Testing and Operations

## Test Matrix
### Determinism
- Same seed + same input bytes -> same final score and RNG state.
- Cross-implementation parity (TS replay vs Rust replay).

### Format and tamper rejection
- Invalid magic/version.
- Truncated payload.
- Reserved bits set.
- Checksum mismatch.
- Footer score mismatch.
- Footer RNG mismatch.

### Rule enforcement
- Cooldown bypass attempts.
- Bullet cap overflow attempts.
- Illegal score increments.
- Invalid wave progression.
- RNG call-order drift.

## Fixture Strategy
- Maintain short/medium/long canonical fixtures.
- Keep expected outputs (score/RNG/hash) under version control.
- Add adversarial fixture corpus for regressions.

## Operational Defaults
- `maxFrames = 18_000`
- Headless replay for authoritative verification.
- Early exit on first violation for diagnostics/perf.
- Strict mode enabled by default.

## Safety Controls
- Distinct dev-mode flags and production-mode enforcement.
- Never accept mock receipts in production.
- Monitor error-code distribution and latency envelopes.
