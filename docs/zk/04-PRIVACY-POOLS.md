# Privacy Pools on Stellar

Last reviewed: February 2026.

## What This Covers
Privacy Pools let users deposit and withdraw while breaking direct transaction
linkability, with compliance controls through association sets.

## Reference Implementations
- Official example path: `stellar/soroban-examples` (`privacy-pools`)
- Community implementation: `ymcrcat/soroban-privacy-pools`
- Design inspiration: `0xbow-io/privacy-pools-core`

## Canonical Flow
1. User creates commitment from secret material.
2. Commitment is deposited and included in an on-chain Merkle structure.
3. User later submits a proof of inclusion + spend validity.
4. Contract validates proof, checks nullifier uniqueness, and processes withdrawal.
5. Association-set checks enforce policy/compliance constraints.

## Stack Snapshot
- Proof system: Groth16
- Typical circuit tooling: Circom + SnarkJS
- Hashing: Poseidon-family primitives
- Verification path: Soroban verifier contract

## Production Gaps To Handle Explicitly
- Recipient binding / frontrunning resistance must be enforced in proof statements.
- Root management strategy (rolling roots / delayed finality windows) must be defined.
- Nullifier and state growth controls must be in place.
- Governance for policy updates (association sets, allowlists) must be explicit.

## Related Roadmaps
Some ecosystem teams describe a phased path:
1. Privacy pools + compliance sets.
2. View-key/selective disclosure support.
3. In-pool private transfer flows.

Use this as directional context, not protocol guarantee.

## References
- <https://stellar.org/blog/ecosystem/prototyping-privacy-pools-on-stellar>
- <https://github.com/ymcrcat/soroban-privacy-pools>
- <https://github.com/stellar/soroban-examples/tree/main/privacy-pools>
