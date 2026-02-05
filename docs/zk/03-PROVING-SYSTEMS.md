# Proving Systems on Stellar

## Overview

Stellar supports multiple ZK proving systems. Each has different tradeoffs in proof size, verification cost, programming model, and ecosystem compatibility.

---

## 1. Groth16 (SNARKs)

### Status: Production-ready on Soroban

**Curve Support**: BLS12-381 (since Protocol 22), BN254 (since Protocol 25)

### How It Works on Stellar

1. Write circuits in **Circom** language
2. Compile with Circom compiler
3. Generate proofs with **SnarkJS**
4. Convert outputs using **circom2soroban** tool
5. Deploy **Groth16 verifier** contract on Soroban
6. Verify proofs on-chain

### Key Characteristics

| Property | Value |
|----------|-------|
| Proof Size | ~200 bytes |
| Verification Cost | ~40M Soroban instructions (~40% testnet budget) |
| Trusted Setup | Required (Powers of Tau ceremony) |
| Circuit Language | Circom |
| Curve | BLS12-381 or BN254 |

### Example: Privacy Pools

```
circuits/main.circom -> Circom compiler -> SnarkJS -> circom2soroban -> Soroban contract
```

### When to Use Groth16

- You need the smallest proof sizes
- Verification cost must be minimal
- You're okay with a trusted setup
- Building privacy pools, mixers, or simple ZK verification
- Porting from Ethereum (especially with BN254)

---

## 2. RISC Zero zkVM (STARKs)

### Status: Deployed on Soroban testnet (Sep 2025), production via Boundless

**Partner**: Nethermind (deployed the verifier), Boundless (marketplace), Wormhole (cross-chain)

### How It Works on Stellar

1. Write arbitrary programs in **Rust** (or any RISC-V language)
2. Execute off-chain in the RISC Zero zkVM
3. Generate **STARK proof** (called a "receipt")
4. Recursion layer aggregates STARKs into compact proofs
5. Settlement layer converts to **Groth16 format** for on-chain verification
6. Verify on Soroban via the **RISC Zero verifier** contract

### Key Characteristics

| Property | Value |
|----------|-------|
| Proof Size | ~200KB (after recursion to Groth16) |
| Programming | Any language compiling to RISC-V |
| Trusted Setup | None (transparent) |
| Quantum Resistance | Yes (STARK-based) |
| Programming Model | General purpose (write Rust, not circuits) |

### Architecture Layers

```
zkVM Layer (STARK proofs) -> Recursion Layer (aggregation) -> Settlement Layer (Groth16 on-chain)
```

### On-Chain Architecture (NethermindEth/stellar-risc0-verifier)

The Nethermind verifier system uses a **layered router pattern**:

```
Router (selector-based dispatch)
  ├── Groth16 Verifier (production — BN254 pairing: e(-A,B) * e(α,β) * e(vk_x,γ) * e(C,δ) == 1)
  ├── Mock Verifier (DEV_MODE=1 — for development/testing)
  └── [future verifiers registered by 4-byte selector]
TimeLock Controller (governed updates to router)
```

- **Seal format**: 4-byte selector + 64B point A + 128B point B + 64B point C = **260 bytes**
- **Verification key and parameters** are embedded at compile time (no storage costs)
- **RISC Zero v3.0.0** parameters with 5 public signals (control_root split + claim_digest split + bn254_control_id)
- **DEV_MODE**: Mock verifier accepts `selector || keccak256(claim_digest)` — no real proofs needed for local dev
- **Governance**: TimeLock controller owns the Router, ensuring verifier registration changes go through a time-delayed approval

#### Testnet Contract IDs
| Contract | Address |
|----------|---------|
| Router | `CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD` |
| Groth16 Verifier | `CB54QOGYJJOSLNHRCHTSVGKJ3D5K6B5YO7DD6CRHRBCRNPF2VX2VCMV7` |
| Mock Verifier | `CCKXGODVBNCGZZIKTU2DIPTXPVSLIG5Z67VYPAL4X5HVSED7VI4OD6A3` |

### When to Use RISC Zero

- You need to prove arbitrary computations
- You want to write in Rust, not circuit DSLs
- Cross-chain proof verification is needed
- You need quantum-resistant proofs
- Complex business logic that doesn't fit circuit models

### Boundless Integration

Boundless is RISC Zero's decentralized proof marketplace:
- Submit proof requests via API
- Third-party provers generate proofs competitively
- Multi-chain settlement (Stellar, Ethereum, etc.)
- "The Signal" initiative: proving every blockchain

---

## 3. Noir / UltraHonk

### Status: Active development, proof-of-concept on Soroban

**Developer**: Aztec Network (Noir language), community (Soroban integration)

### How It Works on Stellar

1. Write circuits in **Noir** language (Rust-like syntax)
2. Compile with **Nargo** compiler
3. Generate proofs using **Barretenberg** backend
4. Verify using **UltraHonk verifier** contract on Soroban

### Key Characteristics

| Property | Value |
|----------|-------|
| Language | Noir (Rust-like, developer-friendly) |
| Backend | Barretenberg (UltraHonk proving system) |
| Curve | BN254 (primary), BLS12-381 (secondary) |
| Recursive Proofs | Supported |
| Contract Size | Near Soroban 128 KiB Wasm limit |

### Current Implementation Status

- UltraHonk verifier deployed to Soroban localnet
- Tornado Classic mixer implemented as reference
- Uses Nargo v1.0.0-beta.9 and Barretenberg v0.87.0
- Resource budget optimization ongoing
- Future: integrate Soroban's BN254 and Poseidon2 precompiles to reduce fees

### When to Use Noir

- You prefer Rust-like syntax over Circom
- You need recursive proofs
- You're building privacy-preserving applications
- You want a more developer-friendly circuit language
- Migrating from Aztec/Ethereum Noir projects

### Key Discussions

- Noir-lang discussion #8509: UltraHonk Verifier for Soroban
- Noir-lang discussion #8560: Resource constraints

---

## 4. SP1 (Succinct) zkVM

### Status: Not yet integrated with Stellar; relevant emerging system

**Developer**: Succinct Labs
**Website**: https://succinct.xyz

### Overview

SP1 is a high-performance zkVM that could become relevant to Stellar's ZK ecosystem. Key capabilities:

- **SP1 Hypercube**: Can prove 99.7% of Ethereum blocks in under 12 seconds
- **Real-time proving**: Approaching sub-second proof generation for many workloads
- **RISC-V based**: Similar architecture to RISC Zero, write programs in Rust
- **Open source**: Fully open-source prover and verifier

### Relevance to Stellar

- Boundless (RISC Zero) has announced plans for **multi-zkVM support** including SP1, Boojum, and Jolt
- If integrated via Boundless, SP1 proofs could settle on Stellar without native integration
- Represents the broader trend of zkVM competition driving down proving costs

### When to Watch SP1

- If Boundless adds SP1 as a supported prover backend
- If proving speed is critical for your application
- If you need the most cost-efficient proving for large computations

---

## RISC Zero R0VM 2.0

The latest version of RISC Zero's zkVM includes significant improvements relevant to Stellar developers:

- **Formally verified**: First zkVM to be formally verified (by Veridise using Picus)
- **RISC-V compliance**: Full RISC-V instruction set support
- **Steel**: A "zk coprocessor" for reading blockchain state inside the zkVM
  - Enables ZK proofs that reference on-chain data
  - Works across chains (Ethereum, Stellar via adapters)
- **Boundless integration**: Native support for the decentralized proof marketplace

---

## Boundless Platform Updates (2026)

Boundless has evolved significantly since initial Stellar integration:

- **ZKC Token**: Utility token for the proof marketplace
- **Proof of Valid Work (PoVW)**: Consensus mechanism for proof validation
- **Multi-zkVM Support**: Plans to support SP1, Boojum, and Jolt alongside R0VM
- **Bitcoin Integration**: ZK proofs for Bitcoin via BitVM
- **Steel**: ZK processor for reading chain state, enabling cross-chain state proofs
- **"The Signal" Initiative**: Partnership to ZK-prove every blockchain's finality

---

## Comparison Matrix

| Feature | Groth16 | RISC Zero | Noir/UltraHonk | SP1 (Future) |
|---------|---------|-----------|-----------------|--------------|
| **Maturity on Stellar** | Production | Testnet/Boundless | PoC/Development | Not integrated |
| **Proof Size** | Smallest (~200B) | Medium (~200KB) | Small | Medium |
| **Verification Cost** | ~40M instructions | Moderate | Near budget limit | TBD |
| **Programming Model** | Circom circuits | Any Rust/RISC-V | Noir circuits | Any Rust/RISC-V |
| **Trusted Setup** | Yes | No | No | No |
| **Quantum Resistant** | No | Yes | No | Yes |
| **Recursive Proofs** | No (natively) | Yes | Yes | Yes |
| **Formal Verification** | N/A | Yes (R0VM 2.0) | N/A | N/A |
| **Best For** | Simple ZK apps | Complex computation | Developer-friendly ZK | Speed-critical |
| **Curve** | BLS12-381 / BN254 | Groth16 settlement | BN254 | Groth16 settlement |

---

## Proving System Selection Guide

```
Need to prove ARBITRARY COMPUTATION?
  -> RISC Zero zkVM (R0VM 2.0, formally verified)

Need SMALLEST PROOFS and lowest verification cost?
  -> Groth16 with Circom

Want DEVELOPER-FRIENDLY circuit language?
  -> Noir / UltraHonk

Porting from ETHEREUM?
  -> Groth16 (BN254) for direct compatibility
  -> Noir if already using Aztec tooling

Need QUANTUM RESISTANCE?
  -> RISC Zero (STARK-based)

Building PRIVACY POOLS?
  -> Groth16 (proven implementation exists)

Need CROSS-CHAIN proofs?
  -> RISC Zero via Boundless + Wormhole

Need FASTEST PROVING SPEED?
  -> Watch SP1 (not yet on Stellar, but coming via Boundless multi-zkVM)

Building CONFIDENTIAL TOKENS?
  -> Confidential Token Association standard (ERC-7984)
  -> FHE + ZK range proofs (in development)

Need FORMALLY VERIFIED prover?
  -> RISC Zero R0VM 2.0 (verified by Veridise/Picus)
```
