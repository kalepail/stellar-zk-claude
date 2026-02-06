# ZK Use Cases on Stellar

Last reviewed: February 2026.

## Primary Patterns
| Use Case | Core Value | Typical Stack |
|---|---|---|
| Confidential tokens / private payments | Hide balances/amounts while preserving policy controls | Privacy pools, confidential token designs, selective disclosure |
| zkKYC / selective disclosure | Prove compliance attributes without exposing raw identity data | Credential + proof verification contracts |
| zkLogin / private auth | Authenticate without exposing account-level identity details | Proof-based auth logic, optional ring-signature patterns |
| zkVoting / private governance | Private ballots with public verifiability | Membership proofs + private vote logic |
| zkCompute / replay verification | Prove expensive off-chain execution on-chain | zkVM path (e.g., RISC Zero) + settlement verifier |
| zkData / oracle proofs | Verify external facts/data with cryptographic guarantees | Reclaim-like proof ingestion flows |

## Readiness Guidance
- **Build now**: private-payment primitives, zkKYC-style attribute proofs,
  verifier-backed zkCompute patterns.
- **Build with caution**: full confidential-token stacks still evolving across
  standards and ecosystem implementations.

## Product Design Notes
- ZK adds privacy and integrity, not automatic compliance.
- Explicit policy layers (association sets, view keys, governance controls) are
  required for production regulatory alignment.
- User experience (proof generation latency, wallet flow, error handling) is a
  first-class product constraint.
