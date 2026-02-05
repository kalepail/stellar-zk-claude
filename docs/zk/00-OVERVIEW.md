# Stellar ZK (Zero-Knowledge) Ecosystem - Complete Reference

## What This Is

A comprehensive collection of every known resource, repository, tool, blog post, and reference related to building zero-knowledge proof applications on the Stellar blockchain. This research was compiled in February 2026.

## Current State (as of Feb 2026)

**Protocol 25 "X-Ray" is LIVE on Mainnet** (activated January 22, 2026). This upgrade introduced native cryptographic primitives for ZK proof verification on Soroban smart contracts.

## Table of Contents

| Document | Description |
|----------|-------------|
| [01 - Protocol Foundations](./01-PROTOCOL-FOUNDATIONS.md) | Protocol 25 X-Ray, CAP-0059, CAP-0074, CAP-0075, elliptic curves, Poseidon hashing |
| [02 - GitHub Repositories](./02-GITHUB-REPOSITORIES.md) | Every known GitHub repo for Stellar ZK development |
| [03 - Proving Systems](./03-PROVING-SYSTEMS.md) | Groth16, RISC Zero/STARKs, Noir/UltraHonk - comparison and usage |
| [04 - Privacy Pools](./04-PRIVACY-POOLS.md) | Privacy Pools implementation deep dive with Association Sets |
| [05 - Developer Tools](./05-DEVELOPER-TOOLS.md) | circom2soroban, coinutils, soroban-verifier-gen, OpenZKTool |
| [06 - Partnerships & Integrations](./06-PARTNERSHIPS.md) | Nethermind, Boundless/RISC Zero, Wormhole, OpenZeppelin |
| [07 - Use Cases](./07-USE-CASES.md) | zkTokens, zkLogin, zkKYC, zkVoting, zkCompute |
| [08 - Articles & Media](./08-ARTICLES-AND-MEDIA.md) | Every blog post, video, article, and social media reference |
| [09 - Getting Started Guide](./09-GETTING-STARTED.md) | Practical guide with code snippets to start building |
| [10 - Security Best Practices](./10-SECURITY-BEST-PRACTICES.md) | Veridise audit insights, OpenZeppelin guidance, Noir circuit safety |

## Key Players

| Entity | Role |
|--------|------|
| **Stellar Development Foundation (SDF)** | Core protocol development, privacy strategy, Privacy Pools prototype, Confidential Tokens |
| **Nethermind** | RISC Zero verifier deployment on Soroban, private payment solutions, 3-phase privacy roadmap |
| **Boundless (by RISC Zero)** | Decentralized ZK proof marketplace, cross-chain verification, ZKC token, multi-zkVM support |
| **Wormhole** | Cross-chain messaging with ZK-verified bridges |
| **Rather Labs** | "Interstellar" - Noir language integration with Stellar |
| **OpenZeppelin** | Stellar Contracts library, Confidential Token Association, security tooling, MCP Server |
| **Zama** | Confidential Token Association, FHE (Fully Homomorphic Encryption) |
| **Inco** | Confidential Token Association member, confidential computing |
| **Xcapit** | OpenZKTool - ZK-SNARK toolkit for Soroban (Circom/Groth16/BN254) |
| **Reclaim Protocol** | zkFetch - ZK proof verification for external data on Stellar |
| **Veridise** | Security audits of Soroban smart contracts, R0VM 2.0 formal verification |
| **Certora** | Formal verification for Soroban (first WASM support), continuous integration |
| **Space and Time** | Data indexing with ZK data verification on Stellar |
| **Human Network** | Decentralized identity with ZK nullifiers for Sybil resistance |

## Key Individuals

| Person | GitHub | Role |
|--------|--------|------|
| **Jay Geng (jayz22)** | [jayz22](https://github.com/jayz22) | SDF — Central ZK architect. Created Groth16 verifier, `rs-soroban-poseidon`, `stellar-confidential-token`, Bulletproofs, privacy pools |
| **Matteo Lisotto (Oghma)** | [Oghma](https://github.com/Oghma) | Nethermind — Primary engineer on stellar-risc0-verifier (router, verifier, timelock, mock) |
| **Nick Dimitriou (NiDimi)** | [NiDimi](https://github.com/NiDimi) | Nethermind — Cryptography researcher. Meridian 2025 speaker |
| **yugocabrio** | [yugocabrio](https://github.com/yugocabrio) | UltraHonk core developer (244+ commits). Deep ZK cryptographer |
| **indextree** | [indextree](https://github.com/indextree) | UltraHonk contract developer (65+ commits) |
| **Pamphile Roy (tupui)** | [tupui](https://github.com/tupui) | NoirCon3 presenter. zk-sudoku demo |
| **Yan Michalevsky (ymcrcat)** | [ymcrcat](https://github.com/ymcrcat) | Privacy researcher. Privacy pools author. FHE/MPC dark pools |
| **teddav** | [teddav](https://github.com/teddav) | @zksecurity — Contributed to `rs-soroban-poseidon`. Noir/Halo2 expert |
| **Willem Olding** | [willemolding](https://github.com/willemolding) | Independent ZK engineer. risc0-verifier contributor |
| **mysteryon88 (Sergey)** | [mysteryon88](https://github.com/mysteryon88) | Built soroban-verifier-gen. Cross-chain ZK tooling |
| **Tyler van der Hoeven (kalepail)** | [kalepail](https://github.com/kalepail) | Stellar ecosystem. ZK explorer across all three proving systems |
| **fboiero** (Xcapit) | — | OpenZKTool author (111 commits). Full BN254 pairing on Soroban |
| **lumenbro** | [lumenbro](https://github.com/lumenbro) | Independent — stellar-groth16, zk-bridge, zk-l2-settlement |

## Supported Elliptic Curves

| Curve | CAP | Protocol | Status |
|-------|-----|----------|--------|
| **BLS12-381** | CAP-0059 | Protocol 22 | Live on Mainnet |
| **BN254** | CAP-0074 | Protocol 25 | Live on Mainnet (Jan 22, 2026) |

## Supported Hash Functions

| Hash | CAP | Protocol | Status |
|------|-----|----------|--------|
| **Poseidon / Poseidon2** | CAP-0075 | Protocol 25 | Live on Mainnet (Jan 22, 2026) |
| **SHA-256** | Native | All | Live (not ZK-optimized) |

## Key Links

- **Stellar ZK Learn Page**: https://stellar.org/learn/zero-knowledge-proof
- **Protocol X-Ray Announcement**: https://stellar.org/blog/developers/announcing-stellar-x-ray-protocol-25
- **Privacy Strategy**: https://stellar.org/blog/ecosystem/strategy-for-privacy-on-blockchain
- **Privacy Pools Blog**: https://stellar.org/blog/ecosystem/prototyping-privacy-pools-on-stellar
- **Financial Privacy Blog**: https://stellar.org/blog/developers/financial-privacy
- **5 ZK Use Cases**: https://stellar.org/blog/developers/5-real-world-zero-knowledge-use-cases
- **GitHub Discussion #1500**: https://github.com/orgs/stellar/discussions/1500
- **Stellar Developer Discord**: https://discord.com/invite/stellardev
- **Meridian 2025 Privacy Talk (Tomer Weller)**: https://www.youtube.com/watch?v=j36W1LUGGrs
- **Meridian 2025 Confidential Tokens Demo (Jay Geng)**: https://www.youtube.com/watch?v=6NnDqVQYOHM
- **Meridian 2025 Private Transactions (Nick Dimitriou)**: https://www.youtube.com/watch?v=JFgDnMBTwAE

## What's New (Second-Pass Research, Feb 2026)

Key discoveries from follow-up research not captured in the initial documentation:

- **Confidential Tokens**: Jay Geng demoed encrypted balances, confidential transfers with ZK range proofs, sigma proofs, view keys, and auditor keys at Meridian 2025
- **Confidential Token Standard**: Being developed by the Confidential Token Association (SDF + OpenZeppelin + Zama + Inco). Note: the formal standard number (sometimes cited as "ERC-7984") has not been verified on GitHub
- **Nethermind 3-Phase Roadmap**: Phase 1 (privacy pools + association sets), Phase 2 (view keys for auditors), Phase 3 (compliant in-pool transfers)
- **R0VM 2.0**: Formally verified RISC-V zkVM by RISC Zero, verified by Veridise/Picus
- **Boundless Updates**: ZKC token launched, Proof of Valid Work (PoVW) mechanism, multi-zkVM support planned (SP1, Boojum, Jolt), Bitcoin integration via BitVM, Steel (zk processor for reading chain state)
- **Resource-Slotted Block Model**: Stellar's block architecture dedicates "slots" for ZK operations, enabling up to 3x more transactions per block
- **OpenZeppelin Stellar Contracts**: Comprehensive library including Smart Accounts, RWA tokens (ERC-3643), Token Vault (SEP-56), Security Detector SDK, Contract Wizard, MCP Server
- **Space and Time**: Data indexing partnership with Stellar for ZK-verified on-chain data
- **Interoperability Standards Organization**: Stellar + MIT + Chainlink + Wormhole + Canton partnership
- **SCF v7.0**: New community fund with 4-tranche milestone-based funding and Instawards up to $15K
- **stellar/slingshot**: Archived (June 2024) historical ZK project with ZkVM, Bulletproofs, confidential assets (424 stars)
- **stellar/stellar-confidential-token**: Official prototype for confidential tokens, created by jayz22 (3 stars)
- **New repos discovered**: `zk-examples/zk-soroban-examples`, `olivmath/stellar-noir-zk`, `0x5ea000000/groth16-soroban`, `AshFrancis/zkvote`, `Gmin2/DuskPool`, `jamesbachini/RiscZero-Experiments`, `lumenbro/stellar-groth16`
- **Open PRs**: `rs-soroban-env #1642` (is_on_curve host functions), `stellar-protocol #1869` (CAP 81 draft), `stellar-dev-skill #2` (ZK skill for P25)
- **Correction**: `OpenZeppelin/openzeppelin-confidential-contracts` is Ethereum FHE only (Zama FHEVM), NOT Stellar-related
