# RISC0 Asteroids Verifier Status (AST3)

Date: 2026-02-11

This doc captures the current state of the RISC0 “risk‑zero” Asteroids verifier: performance snapshot, what is and is not guaranteed, coverage assessment, and the key work done in this optimization/audit session.

## Non‑Negotiable Requirements

- Strict fairness: verification must prove the game was played according to the AST3 rules as implemented by the verifier (no “trust the client” outcomes).
- Determinism: the replay must be fully deterministic (no floats, no host timing, no nondeterministic iteration).
- Forward‑only tape format: AST3 v1 tapes only (claimant address embedded in the tape header; legacy tapes are rejected).
- Rust is the source of truth for the proof. Any gameplay‑semantic change must be mirrored in the TypeScript engine/serializer so the on‑screen sim and tape generation match what the proof verifies.

## What The Verifier Proves

Verification entrypoint is `verify_tape(bytes, max_frames)` in `risc0-asteroids-verifier/asteroids-core/src/verify.rs`.

At a high level it proves:

1. **Tape integrity and format correctness**
   - Magic/version/rules tag and reserved bytes/bits are validated.
   - CRC32 matches (tampering is rejected).
   - Frame count is bounded by `max_frames`.
   - Claimant address is present in the header and is validated as a Stellar StrKey (no NUL padding).

2. **Deterministic replay of outcomes**
   - The verifier replays from `seed + inputs` and computes `final_score`, `final_rng_state`, and `frame_count`.
   - It rejects if the computed results do not match the tape footer.

3. **Per‑frame rule enforcement (strict mode)**
   - Verification uses `replay_strict`, which for every frame:
     - Captures a `TransitionState` snapshot before and after stepping.
     - Runs `validate_transition` (detects illegal deltas like cooldown bypass, turn-rate jumps, illegal score deltas, speed clamp bypass, etc.).
     - Runs `validate_invariants` (bounds, caps, consistent global invariants).

Net effect: the prover cannot submit a tape that “claims” a score/RNG/frames unless those are produced by the simulator while passing strict validation.

## Determinism Notes

Rust core (`risc0-asteroids-verifier/asteroids-core`) is integer‑only and uses a xorshift32 RNG (`SeededRng`), so replay is deterministic given `seed + inputs`.

TypeScript gameplay sim is also integer‑math for the ZK‑relevant physics; however, the interactive frontend still uses wall‑clock time for choosing a default seed (`Date.now()`) unless a seed is provided. This is not a verifier bug, but it is a fairness/product‑policy risk if players are allowed to grind seeds (see “Gaps”).

## Performance Snapshot (Strict Verifier)

Best measured baseline from `scripts/bench-core-cycles.sh` (strict replay; CPU/dev mode):

- short: `310,952` cycles (500 frames)
- medium: `~4,153,3xx` cycles (3,980 frames)
- real: `~19,149,1xx` cycles (13,829 frames), segments `10`

Profiling (pprof, medium) shows the main cost is the game step loop:

- `Game::step_decoded` dominates (~75% flat)
- `replay_strict` overhead (transition validation + invariants) is the next tier (~10% + ~8-9%)

How to reproduce:

- `bash scripts/bench-core-cycles.sh`
- Optionally: `bash scripts/bench-core-cycles.sh --pprof-case medium`

## Coverage Assessment

What is covered well today:

- Tape parsing/CRC/format checks and claimant validation (unit tests cover many invalid cases).
- Strict replay rejects key classes of cheating/tampering:
  - score/RNG tampering
  - cooldown bypass
  - speed clamp bypass
  - ship teleport / illegal step
  - illegal score deltas
  - cap violations (e.g., saucer bullet cap is enforced in-step and via invariants)
- Determinism checks exist in Rust tests (same seed+inputs produce identical results).
- There is a TS headless verifier script (`scripts/verify-tape.ts`) that can replay a tape and compare score/RNG state to the footer, which is useful for sanity and parity checks.

What is not “fully covered” (engineering gaps, not necessarily rule gaps):

- There is no large‑scale Rust<->TS differential test harness (random seeds/inputs at scale) to continuously detect drift.
- The strict validator intentionally does not re‑derive every internal variable; it validates high‑impact invariants and relies on the simulator implementation for the rest. This is a reasonable tradeoff, but it means:
  - If a simulator bug exists that still satisfies the checked invariants, it could become a “rule hole.”
  - Adding more transition checks would reduce that risk but costs cycles.

## Identified Gaps / Risks (Priority Order)

1. **Seed provenance / seed grinding**
   - The verifier proves “played fairly for a given seed,” but if the client can choose or brute‑force seeds to get favorable early spawns, that may violate intended fairness.
   - If the product rule is “seed must be committed/assigned,” this needs an external protocol (server/contract seed commitment), not a verifier change.

2. **Cross-language drift risk**
   - The proof is the Rust simulator. The frontend must remain aligned.
   - Mitigation: add a differential test harness that compares Rust replay results vs TS headless results over many random tapes (and/or a fixed corpus), run in CI.

3. **Validator completeness vs cost**
   - `validate_transition` focuses on the most cheat-relevant invariants.
   - If you want even stronger guarantees “against simulator bugs,” add more per-frame checks (timers, spawn scheduling, entity lifetimes, etc.). This increases guest cycles.

4. **Line-level pprof attribution**
   - Current guest pprof can fail to resolve `-list` due to “unknown” source paths. Fixing attribution would help drive the next round of optimizations with higher confidence.

## Work Captured In This Session

### Restored Best Performance (Regression Fix)

- Reverted a `shortest_delta_q12_4` rewrite that A/B‑tested worse on RV32 guest cycles.
  - Commit: `f837eeb` (`verifier-core: revert shortest_delta unsigned check`)
  - This restored the best-known cycle counts for strict verification.

### Tape Claimant Threading + Parity Improvements

- Ensured claimant-in-header behavior is consistently treated as the only source of claimant identity (forward-only).
- Updated autopilot tooling to embed a valid claimant in generated tapes so generated artifacts are always provable.
  - Commit: `c98ecd3` (`autopilot: embed claimant in generated tapes`)

### Repo Hygiene

- Ignored `*-autopilot/` local artifact dirs so `git status` stays clean during bench/evolution runs.
  - Commit: `3a5be25` (`chore: ignore autopilot artifact dirs`)

### Next Optimization Roadmap Doc

- Created a focused “what to try next” perf doc that keeps strict fairness intact.
  - `docs/VERIFIER-PERF-NEXT.md`

## Confidence Statement (Honest)

- **Tape integrity + unforgeability of outcome (score/RNG/frames/claimant):** very high.
- **Deterministic rule enforcement (as implemented):** very high.
- **“Absolutely no gaps whatsoever”:** high confidence, but not absolute. The remaining material risks are:
  - seed provenance policy (external to verifier)
  - cross-language drift over time without differential tests
  - the inherent possibility of a simulator bug that passes currently-checked invariants

If you want the strongest possible assurance without loosening strictness, the highest-value engineering investment is a Rust<->TS differential corpus test suite plus CI coverage, and a seed commitment protocol if seed grinding is disallowed by product rules.

