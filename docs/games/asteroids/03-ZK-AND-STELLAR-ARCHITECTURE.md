# Asteroids ZK and Stellar Architecture

## End-to-End Flow
1. Client records gameplay tape.
2. Tape is validated and replayed by deterministic verifier logic.
3. Prover generates receipt/proof for replay correctness.
4. Proof is submitted to Stellar verifier contract path.
5. Verified score is committed for leaderboard/settlement usage.

## Proof Statement
Public claim should include at minimum:
- tape hash / commitment
- final score
- final RNG state
- verifier image/program identity

## Settlement Model
- Keep on-chain verification minimal and deterministic.
- Keep heavy replay computation off-chain.
- Keep verifier contract interface stable and versioned.

## Governance and Upgrades
- Verification program IDs and verifier contract IDs must be explicit.
- Any logic change requires version bump and migration plan.
- Avoid mutable critical verification keys without strong governance controls.

## Operational Constraints
- Proof generation latency is part of user experience.
- Verification and submission cost envelope must be monitored per network limits.
- Dev-mode proof paths must never be accepted in production verification flows.
