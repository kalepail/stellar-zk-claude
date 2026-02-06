# Protocol Foundations for ZK on Stellar

Last reviewed: February 2026.

## Core CAPs
| CAP | Protocol | Status | What It Adds |
|---|---|---|---|
| `CAP-0059` | 22 | Live | BLS12-381 host crypto ops |
| `CAP-0074` | 25 | Live | BN254 host crypto ops |
| `CAP-0075` | 25 | Live | Poseidon/Poseidon2 host primitives |

## Host Functions Most Relevant to ZK
### BN254 (CAP-0074)
- `bn254_g1_add`
- `bn254_g1_mul`
- `bn254_multi_pairing_check`

### BLS12-381 (CAP-0059)
- G1/G2 add, mul, msm
- multi-pairing check
- hash/map-to-curve helpers

### Poseidon/Poseidon2 (CAP-0075)
- Host-level primitives to avoid expensive custom hashing in contracts.

## Dual-Curve Strategy
Stellar currently supports both curves for practical reasons:
- **BN254**: easier migration from EVM-oriented ZK stacks.
- **BLS12-381**: broader cryptographic usage and stronger security margin.

## Near-Term Optimization Watch
### CAP-0080 (BN254 MSM)
Expected impact if adopted:
- Replaces many repeated add/mul calls with a dedicated MSM pathway.
- Major verifier-cost reduction for BN254-heavy contracts.
- Particularly important for Noir/UltraHonk verifier economics.

## Practical Constraints to Design Around
- Soroban instruction budgets still shape verifier architecture decisions.
- On-chain verification cost is often dominated by pairings/MSM.
- Hash choice matters: Poseidon-family primitives are ZK-friendly; SHA-256 is usually more expensive in proof contexts.

## Design Guidance
- Use protocol-native host primitives first.
- Keep verification keys immutable (or heavily governed).
- Budget for CAP evolution; avoid hard-coding assumptions that depend on temporary limits.

## References
- CAP-0059: <https://github.com/stellar/stellar-protocol/blob/master/core/cap-0059.md>
- CAP-0074: <https://github.com/stellar/stellar-protocol/blob/master/core/cap-0074.md>
- CAP-0075: <https://github.com/stellar/stellar-protocol/blob/master/core/cap-0075.md>
- CAP-0080: <https://github.com/stellar/stellar-protocol/blob/master/core/cap-0080.md>
