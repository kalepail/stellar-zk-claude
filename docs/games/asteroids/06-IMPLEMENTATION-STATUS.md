# Asteroids Implementation Status

## Current Baseline
- Deterministic replay and tape verification logic exists.
- RISC0 workspace exists with host/guest/core split.
- Strict verification model and rule groups are defined.
- AST3 tape contract is active (`version=2`, `rules_tag=3`, reserved bytes/bits enforced).
- Proof gateway requires claimant address header and drives claim relay after proof success.
- Score contract enforces AST3 rules digest and claimant-scoped best-score minting.

### RISC0 workspace shape
- Shared deterministic core crate for replay and tape parsing.
- Guest program that commits verification journal outputs.
- Host runner for proving, receipt verification, and journal extraction.
- API server path for async proof jobs.

## Required for Production-Grade Verification
- Keep engine and prover replay behavior bit-for-bit aligned.
- Expand malformed/tamper corpus and property-based tests.
- Define and freeze verifier IDs/program IDs for release version.
- Ensure production path rejects dev-mode/unverified receipts.

## Open Engineering Priorities
1. Determinism parity tests across TypeScript and Rust replay paths.
2. Rule violation diagnostics and stable error-code surface.
3. Proof submission + settlement integration path hardening.
4. End-to-end operational benchmarking under realistic tape lengths.

## Production Readiness Gates
- Dev-mode receipts are blocked in production verification path.
- Receipt kind and verifier route are fixed per release.
- Journal decoding and field offsets are validated against real prover outputs.
- Replay-protection and image-ID pinning are enforced on-chain.

## Non-Goals for Current Release
- Multi-proof hybrid verifier architecture.
- Browser-native full replay proving.
