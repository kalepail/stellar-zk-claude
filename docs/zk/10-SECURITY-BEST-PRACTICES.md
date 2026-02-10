# Security Best Practices for ZK on Stellar

Last reviewed: February 2026.

## Security Model
A ZK application has three risk layers:
1. Contract logic and storage safety.
2. Proof statement/circuit correctness.
3. Operational governance and deployment hygiene.

All three must pass.

## Contract-Level Checklist
- Strict input decoding and bounds checks.
- Avoid unsafe panics/unwrap patterns in critical paths.
- Keep verifier keys immutable or strongly governed.
- Cap unbounded growth (roots, nullifiers, queues, payload sizes).
- Add fuzz/property tests for malformed proof inputs.

## Statement/Circuit Checklist
- No underconstrained witness paths.
- Explicit range checks where field wrap could violate intent.
- Bind recipient/context/domain values to prevent replay/fraud.
- Test soundness (invalid fails) and completeness (valid passes).

## Deterministic Verification Checklist
For replay/state-transition verification systems:
- Canonical transition order must be explicit and versioned.
- Numeric model must be deterministic (fixed-point/integer where required).
- Authoritative RNG stream must be isolated from visual/non-consensus randomness.
- Final-state commitments (score/RNG/hash) must be enforced.
- Cross-implementation parity tests are mandatory.

## Dev Mode Safety
RISC Zero dev mode (`RISC0_DEV_MODE=1`) generates fake receipts that bypass proving. Essential rules:
- Dev mode receipts **fail standard verification**. Only a dev-mode verifier will accept them.
- Use the `disable-dev-mode` feature flag on `risc0-zkvm` in production builds. If `RISC0_DEV_MODE` is set with this flag, the prover will panic.
- Never deploy with `PROOF_MODE_POLICY=secure-and-dev` in production API servers.
- Dev mode is safe for: cycle count profiling, A/B optimization testing, local development.

## Precompile Timing Considerations
RISC Zero precompiles (SHA-256, RSA, elliptic curve ops) do not currently guarantee constant-time execution. If processing private data, observers could measure proving duration or cycle counts to extract information. Exercise caution when precompile inputs are sensitive.

## Operational Checklist
- Pin cryptographic dependencies and audit changes.
- Track protocol/CAP changes that impact costs and limits.
- Define emergency controls and upgrade procedures.
- Monitor verifier failures and abnormal patterns in production.
- Ensure `RISC0_DEV_MODE` is unset in production environments.
- Verify receipt verification happens on-chain (not just server-side).

## References
- Veridise checklist:
  <https://veridise.com/blog/audit-insights/building-on-stellar-soroban-grab-this-security-checklist-to-avoid-vulnerabilities/>
- OpenZeppelin Noir circuit guide:
  <https://www.openzeppelin.com/news/developer-guide-to-building-safe-noir-circuits>
