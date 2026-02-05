# Protocol Foundations for ZK on Stellar

## Protocol 25 "X-Ray" - The ZK Upgrade

**Status**: Live on Mainnet (January 22, 2026)
**Testnet Vote**: January 7, 2026
**Mainnet Vote**: January 22, 2026

X-Ray is named after the everyday tool used to "show only what needs to be seen and nothing more." It is the first major milestone in Stellar's long-term privacy strategy, outlined by SDF CPO Tomer Weller at Meridian 2025.

### What X-Ray Introduces

Two critical Core Advancement Proposals (CAPs):

---

## CAP-0074: Host Functions for BN254

**GitHub**: https://github.com/stellar/stellar-protocol/blob/master/core/cap-0074.md

BN254 is the most widely used pairing-friendly elliptic curve in the current ZK ecosystem, used by privacy pools, Lighter, Starknet, and ZK Email.

### Host Functions Added

| Function | Purpose |
|----------|---------|
| `bn254_g1_add` | Point addition on BN254 G1 |
| `bn254_g1_mul` | Scalar multiplication on BN254 G1 |
| `bn254_multi_pairing_check` | Multi-pairing check for proof verification |

### Why BN254 Matters

- Provides feature parity with Ethereum's **EIP-196** and **EIP-197** precompiles
- Enables direct migration of EVM-based ZK applications to Stellar
- Most existing ZK tooling and libraries use BN254
- Without it, developers had to rewrite applications for a different curve

---

## CAP-0075: Cryptographic Primitives for Poseidon/Poseidon2

**GitHub**: https://github.com/stellar/stellar-protocol/blob/master/core/cap-0075.md

Poseidon is a family of hash functions designed specifically for ZK proof systems.

### Why Poseidon Matters

- Traditional hashes like SHA-256 are computationally expensive inside ZK circuits
- Poseidon requires substantially fewer constraints when used inside proofs
- Reduces both proving time and verification costs
- Ensures consistent hash behavior between off-chain proving and on-chain verification

### Capabilities Unlocked

- Proofs generated with fewer constraints
- Applications avoid expensive on-chain reimplementation of hashing logic
- Consistent hash logic both on-chain and off-chain

---

## CAP-0059: Host Functions for BLS12-381 (Prior Art)

**GitHub**: https://github.com/stellar/stellar-protocol/blob/master/core/cap-0059.md
**Protocol**: 22 (2024)

BLS12-381 was the first pairing-friendly curve added to Stellar. It offers 128-bit security and is preferred by newer protocols like Ethereum 2.0 and Zcash.

### Host Functions

- `bls12_381_g1_add`, `bls12_381_g1_mul`, `bls12_381_g1_msm`
- `bls12_381_g2_add`, `bls12_381_g2_mul`, `bls12_381_g2_msm`
- `bls12_381_multi_pairing_check`
- `bls12_381_map_fp_to_g1`, `bls12_381_map_fp2_to_g2`
- `bls12_381_hash_to_g1`, `bls12_381_hash_to_g2`

---

## Dual-Curve Strategy

Stellar now supports **both** BN254 and BLS12-381. This is strategic:

| Feature | BN254 | BLS12-381 |
|---------|-------|-----------|
| Security Level | ~100-bit | 128-bit |
| Ecosystem Usage | Most EVM ZK apps | Ethereum 2.0, Zcash |
| EVM Compatibility | Direct (EIP-196/197) | Indirect |
| Proof Systems | Groth16, Plonk, UltraHonk | Groth16, Plonk |
| Recommended For | EVM migration, Noir circuits | New native apps, BLS signatures |

---

## Protocol History Timeline

| Protocol | Name | Year | ZK Relevance |
|----------|------|------|-------------|
| 20 | - | 2024 | Soroban smart contracts launch |
| 22 | - | 2024 | BLS12-381 curve support (CAP-0059) |
| 23 | Whisk | 2024 | Parallel execution, unified events |
| 25 | X-Ray | 2026 | BN254 (CAP-0074) + Poseidon (CAP-0075) |

---

## Emerging CAPs (Post-Protocol 25)

These proposals were discussed around the Protocol 25 mainnet activation (January 22, 2026) and may appear in future protocol upgrades:

### CAP-77: Ledger Key Inaccessibility
- Makes certain ledger keys inaccessible based on network configuration
- Relevant for privacy: could prevent unauthorized reads of encrypted state

### CAP-78: TTL Extension Policies
- New policies for extending Time-To-Live on ledger entries
- Important for long-lived ZK state like Merkle trees and nullifier stores

### CAP-79: Strkey Format Conversion Host Functions
- Host functions for converting between strkey formats
- Utility improvement for ZK contracts that work with multiple address formats

---

## Resource-Slotted Block Model

Stellar's block architecture uses a resource-slotted model where ZK operations are placed in dedicated "slots" within each block. This design:

- Allows up to **3x more transactions per block** when ZK operations are included
- Prevents ZK proof verification from consuming the entire block budget
- Enables concurrent processing of ZK and non-ZK transactions
- Makes verification costs predictable for contract developers

---

## Key GitHub Discussions

- **Discussion #1500**: Support cryptographic primitives for proof verification
  https://github.com/orgs/stellar/discussions/1500
- **Discussion #1780**: Poseidon/Poseidon2 hash function design
  https://github.com/orgs/stellar/discussions/1780
- **Issue #779**: BLS12-381 prototype implementation
  https://github.com/stellar/rs-soroban-env/issues/779

---

## Resources

- **Upgrade Guide**: https://stellar.org/blog/developers/stellar-x-ray-protocol-25-upgrade-guide
- **Protocol Upgrades Page**: https://stellar.org/protocol-upgrades
- **CAPs Repository**: https://github.com/stellar/stellar-protocol/tree/master/core
