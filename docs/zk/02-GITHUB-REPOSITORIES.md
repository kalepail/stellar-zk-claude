# GitHub Repositories for Stellar ZK Development

---

## Official Stellar Organization

### stellar/soroban-examples
**URL**: https://github.com/stellar/soroban-examples
**Description**: Official Soroban example contracts. Key ZK directories:
- `groth16_verifier` — no_std Groth16 SNARK verifier for BLS12-381. ~40M instructions (~40% testnet budget). **Key starting point.**
- `privacy-pools` — Complete Privacy Pools: Soroban contract, Circom circuits, demo script, groth16_verifier integration
- `import_ark_bn254` — Example importing the `ark_bn254` elliptic curve library
- `bls_signature` — BLS Signatures custom account contract
- `merkle_distribution` — Merkle proof verification for token distribution
**Languages**: Circom, Rust, Shell

### stellar/rs-soroban-poseidon
**URL**: https://github.com/stellar/rs-soroban-poseidon
**Description**: Official Poseidon/Poseidon2 hash function SDK for Soroban. Includes example contracts.
- Supports **BN254** and **BLS12-381** fields
- Configurable state sizes (T=2-6 for Poseidon, T=2-4 for Poseidon2)
- Poseidon matches **Circom's** spec; Poseidon2 matches **Noir's** spec
- `PoseidonSponge` for reusing pre-initialized parameters
- **v25.0.0** released January 29, 2026
- **Contributors**: jayz22 (created), teddav (SDK update)
**Language**: Rust | **License**: Apache-2.0

### stellar/stellar-confidential-token
**URL**: https://github.com/stellar/stellar-confidential-token
**Description**: Official prototype for confidential tokens on Stellar. Created by jayz22.
- **Stars**: 3
- **Last Updated**: October 2025
- Prototype status — experimental

### stellar/stellar-protocol
**URL**: https://github.com/stellar/stellar-protocol
**Description**: Protocol specifications including ZK-related CAPs:
- `core/cap-0059.md` — BLS12-381 host functions
- `core/cap-0074.md` — BN254 host functions
- `core/cap-0075.md` — Poseidon/Poseidon2 hash functions

### stellar/rs-soroban-env
**URL**: https://github.com/stellar/rs-soroban-env
**Description**: Soroban host environment. Contains BLS12-381 and BN254 host function implementations using arkworks crates.
- Key files: `crypto/bn254.rs`, `crypto/poseidon/`, `cost_runner/cost_types/bn254.rs`, `test/bn254.rs`

### stellar/rs-soroban-sdk
**URL**: https://github.com/stellar/rs-soroban-sdk
**Description**: Rust SDK for Soroban contracts. Contains `crypto/bn254.rs` and Poseidon interfaces.

### stellar/stellar-core
**URL**: https://github.com/stellar/stellar-core
**Description**: Core protocol. Protocol 25/26 includes BN254 and BLS12-381 cost parameters.

### stellar/slingshot (Archived)
**Description**: Earlier ZK work predating Protocol 25. 424 stars (historically significant).
- **ZkVM**: Zero-knowledge virtual machine for blockchain transactions
- **Bulletproofs**: Rust implementation of range proofs
- **Confidential Assets**: Hidden transaction amounts
- **Key contributors**: oleganza (249 commits), cathieyun (173), vickiniu (65)
**Note**: Archived June 2024. Superseded by Protocol 25 BN254/BLS12-381 approach.

---

## RISC Zero Verification

### NethermindEth/stellar-risc0-verifier
**URL**: https://github.com/NethermindEth/stellar-risc0-verifier
**Description**: Full RISC Zero proof verification system for Stellar — router, production verifier, mock verifier, timelock governance, and deployment tooling.
- **Language**: Rust | **License**: Apache-2.0 | **Stars**: 4 | **Forks**: 2
- **Created**: September 2, 2025
- **RISC Zero version**: v3.0.0
- **CI Pipeline**: Docs, Lint, Build & Test, Dependency Audit, UB Detection, Coverage

#### Testnet Deployments
| Contract | Address |
|----------|---------|
| **Router** | `CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD` |
| **Groth16 Verifier** | `CB54QOGYJJOSLNHRCHTSVGKJ3D5K6B5YO7DD6CRHRBCRNPF2VX2VCMV7` |
| **Mock Verifier** | `CCKXGODVBNCGZZIKTU2DIPTXPVSLIG5Z67VYPAL4X5HVSED7VI4OD6A3` |

#### Architecture
```
Router (selector-based dispatch)
  ├── Groth16 Verifier (production — BN254 pairing check)
  ├── Mock Verifier (DEV_MODE=1 — no real proofs)
  └── [future verifiers registered by selector]
TimeLock Controller (governance over router changes)
```

#### Contracts (`feature/deployment-script` branch)
- **`contracts/risc0-router`** — Entry point. 4-byte seal selector dispatch. Selector lifecycle: None → Active → Tombstone. OpenZeppelin `Ownable` pattern.
- **`contracts/groth16-verifier`** — Production BN254 Groth16 verifier. VK/parameters embedded at compile time via `build.rs`. Seal: 4B selector + 64B A + 128B B + 64B C = **260 bytes**.
- **`contracts/mock-verifier`** — For `DEV_MODE=1`. Checks `selector || keccak256(claim_digest)`. No cryptographic verification.
- **`contracts/timelock`** — Timelock controller with Proposer/Executor/Canceller roles, batch operations, self-administration. Designed to own the Router.
- **`contracts/interface`** — Shared traits (`RiscZeroVerifierInterface`) and types (`Receipt`, `ReceiptClaim`, `Output`, `VerifierEntry`). RISC Zero tagged hashing.
- **`tools/build-utils`** — VK digest and selector computation.
- **`scripts/deploy_verifier.sh`** — Deployment for local/futurenet/testnet/mainnet with parameter display.

#### Key Contributors
- **Oghma (Matteo Lisotto)** — Primary engineer (9+ commits, 11 PRs). Built the entire verifier stack.
- **NiDimi (Nick Dimitriou)** — Nethermind cryptography researcher (UCL MSc). Background in folding schemes, circom, zkVMs.
- **frozenspider (Alex Abdugafarov)** — Nethermind, internal code reviewer.
- **willemolding (Willem Olding)** — Independent ZK engineer. Fix contributor. Has `risc0-mixer`, `noir_shuffle`, `zk-light-clients`, `steel` fork, halo2 work.

---

## Noir / UltraHonk Integration

### yugocabrio/rs-soroban-ultrahonk (Consolidated Home)
**URL**: https://github.com/yugocabrio/rs-soroban-ultrahonk
**Description**: The **consolidated repository** for UltraHonk (Noir) verification on Soroban. Contains both the core Rust UltraHonk verifier and the Soroban contract wrapper. Long-term ownership still being determined.
- **Language**: Rust | **Stars**: 6 | **Forks**: 3
- **Created**: June 7, 2025 | **Last Updated**: January 28, 2026
- VK set immutably at deploy time via constructor
- Includes scripts for building ZK artifacts using `nargo` and `bb` (Barretenberg)
- TypeScript helper for contract invocation
- References "tornado_classic" sub-project for testing
- **Not audited** | MIT licensed
- **Key contributor**: yugocabrio (244 commits) — deep ZK cryptographer with work across plookup, NTT, Nova, Halo2, SP1, Nexus

### indextree/ultrahonk_soroban_contract
**URL**: https://github.com/indextree/ultrahonk_soroban_contract
**Description**: Soroban UltraHonk verifier contract. Same primary maintainer (Yugo).
- **Stars**: 9 | **Forks**: 12
- **Created**: August 4, 2025 | **Last Updated**: January 28, 2026
- HackMD writeup: https://hackmd.io/@indextree/rJPW3jU6lx
- Includes Tornado Classic mixer circuit and integration tests
- **Key contributors**: yugocabrio (112 commits), indextree (35 commits), jayz22 (1 commit)

### yugocabrio/ultrahonk-rust-verifier
**URL**: https://github.com/yugocabrio/ultrahonk-rust-verifier
**Description**: Mirrors `rs-soroban-ultrahonk`. Original repo before consolidation.
- **Stars**: 6 | **Forks**: 3

### tupui/ultrahonk_soroban_contract (Pamphile Roy — zk-sudoku)
**URL**: https://github.com/tupui/ultrahonk_soroban_contract
**Description**: Full-stack ZK demo presented at **NoirCon3**. Contains a **Noir sudoku verifier circuit**.
- **Languages**: TypeScript, Rust, Noir | **Stars**: 3 | **Forks**: 4
- **Created**: November 10, 2025
- **Circuit** (`circuits/src/main.nr`): Sudoku verifier — private `solution` (81 fields), public `puzzle` (81 fields). Validates digit ranges, row/column/block sums and sum-of-squares for uniqueness.
- **Contracts**: UltraHonk verifier + `guess-the-puzzle` game contract (valid proof = win prize)
- **Frontend**: Scaffold Stellar (Vite + React), client-side proof generation with `bb`
- **Toolchain**: Nargo v1.0.0-beta.9, Barretenberg v0.87.0, Soroban SDK v23.1.0
- **Slides**: `tupui/Academia/Conf/NoirCon3.pdf`
- **Upcoming**: Blog post showcasing the zk-sudoku example (expected February 2026)

### olivmath/stellar-noir-zk
**URL**: https://github.com/olivmath/stellar-noir-zk
**Description**: Noir meets verifier on Soroban. Contains `smartcontracts/contracts/meets_verifier/src/lib.rs`.
- **Language**: JavaScript | **Last Updated**: November 2025

### salazarsebas/noir-stellar
**URL**: https://github.com/salazarsebas/noir-stellar
**Description**: Noir + Soroban PoC proving `x + y = 10` without revealing values. Includes Noir circuit, Barretenberg proof scripts, Soroban verifier contract.
- **Stars**: 2 | **Language**: Rust
- **Note**: README is transparent that the Soroban contract performs **mock verification only** (structural validation, no actual pairing) due to compute budget limits.

### Rather Labs — "Interstellar" (Proposal)
**Twitter**: https://x.com/rather_labs/status/1932081682439397868
**Description**: Proposal to bridge Noir with Stellar. Write in Noir, prove with BLS12-381, verify on Soroban. Announced June 2025.

---

## Groth16 / Circom Verification Tools

### xcapit/openzktool
**URL**: https://github.com/xcapit/openzktool
**Description**: Complete ZK-SNARK toolkit for Soroban using **Circom + Groth16 on BN254**.
- **Stars**: 13 | **Forks**: 1 | **Languages**: TypeScript, Rust
- **Created**: September 30, 2025
- Multiple Circom circuits (KYC, range proof, compliance, solvency)
- Full BN254 pairing implementation in Rust (field arithmetic, Miller loop, final exponentiation)
- **Deployed testnet**: `CBPBVJJW5NMV4UVEDKSR6UO4DRBNWRQEMYKRYZI3CW6YK3O7HAZA43OI`
- SDK in TypeScript, 49+ tests
- **Key contributor**: fboiero (111 commits, Xcapit Labs, Argentina)

### mysteryon88/soroban-verifier-gen
**URL**: https://github.com/mysteryon88/soroban-verifier-gen
**Crate**: https://lib.rs/crates/soroban-verifier-gen
**Description**: Auto-generates Soroban Groth16 verifier contracts from snarkjs verification key JSON. Uses `ark-bls12-381`.
- **Created**: January 26, 2026
- Has open PR #396 on `stellar/soroban-examples` to add usage examples
- **Also by mysteryon88**: `ark-snarkjs` (Arkworks → snarkjs JSON), `gnark-to-snarkjs` (gnark → snarkjs JSON), `zk-hashes` (hash function research for ZK)

### zk-examples/zk-soroban-examples
**URL**: https://github.com/zk-examples/zk-soroban-examples
**Description**: Multi-framework ZK examples for Stellar covering arkworks, circom, gnark, and noname.
- **Created**: January 26, 2026 | **Last Updated**: January 29, 2026
- Topics: arkworks, circom, gnark, noname, soroban, stellar, zkp

### lumenbro/stellar-groth16
**URL**: https://github.com/lumenbro/stellar-groth16
**Description**: Groth16 ZKP verification for Soroban using BN254 (Protocol 25, CAP-0074).
- **Created**: January 18, 2026
- **Also by lumenbro**: `zk-bridge` (ZK bridge to Stellar L2), `zk-l2-settlement` (ZK L2 settlement for Stellar), `mopro-poseidon-bindings` (Poseidon ZK circuit bindings)

### 0x5ea000000/groth16-soroban
**URL**: https://github.com/0x5ea000000/groth16-soroban
**Description**: Groth16 on Soroban. Minimal.
- **Last Updated**: November 2025

### rsinha/soroban_groth16_verifier
**URL**: https://github.com/rsinha/soroban_groth16_verifier
**Description**: Earliest known Groth16 verifier for Soroban (pre-Protocol 25, June 2024).

---

## Privacy Pools Ecosystem

### ymcrcat/soroban-privacy-pools
**URL**: https://github.com/ymcrcat/soroban-privacy-pools
**Description**: Full Privacy Pools for Soroban. Circom circuits, circom2soroban tool, coinutils CLI.
- **Stars**: 2 | **Language**: Circom
- Referenced in official SDF Privacy Pools blog post
- **Author**: ymcrcat (Yan Michalevsky) — privacy researcher also exploring FHE dark pools, MPC dark pools, and SGX TEEs

### jayz22/soroban-privacy-pools-local
**URL**: https://github.com/jayz22/soroban-privacy-pools-local
**Description**: Local variant of privacy pools on Soroban by Jay Geng.
- **Language**: Circom | **Last Updated**: October 2025

### 0xbow-io/privacy-pools-core
**URL**: https://github.com/0xbow-io/privacy-pools-core
**Description**: Original Privacy Pools monorepo (Ethereum). Inspired the Stellar implementation.
- **Stars**: 117 | Active

### Gmin2/DuskPool
**URL**: https://github.com/Gmin2/DuskPool
**Description**: Privacy pool variant with Poseidon, orderbook, and settlement contracts on Soroban.
- **Created**: February 2026

---

## External ZK Integrations

### reclaimprotocol/zkfetch-stellar-example
**URL**: https://github.com/reclaimprotocol/zkfetch-stellar-example
**Description**: ZK proof generation/verification using Reclaim Protocol on Stellar. Fetches external API data with cryptographic proofs, verifies on-chain.
- **Last Push**: January 21, 2026 (active)
- **Docs**: https://docs.reclaimprotocol.org/zkfetch/stellar

### jamesbachini/RiscZero-Experiments
**URL**: https://github.com/jamesbachini/RiscZero-Experiments
**Description**: RISC Zero experiments with Stellar verifier contracts.
- **Language**: Rust | **Last Updated**: January 2, 2026

### jamesbachini/Stellar-BLS
**URL**: https://github.com/jamesbachini/Stellar-BLS
**Description**: Privacy experiments with BLS signatures and Soroban smart contracts. Demonstrates kLogin for anonymous ring signature verification.
- **Stars**: 1
- **Tutorial**: https://jamesbachini.com/privacy-on-stellar/
- **Video**: https://youtu.be/32xCKGrf3MI

### jamesbachini/Selective-Disclosure-KYC
**URL**: https://github.com/jamesbachini/Selective-Disclosure-KYC
**Description**: Stellar-based selective disclosure KYC system using BLS ring signatures. Lets users prove attributes privately without revealing full identity.
- **Tutorial**: https://jamesbachini.com/selective-disclosure/
- **Video**: https://youtu.be/ajy3G_Y4l1w

### jamesbachini/Noirlang-Experiments
**URL**: https://github.com/jamesbachini/Noirlang-Experiments
**Description**: Example Noir zero-knowledge circuits including private limit orders, secret-word proofs, and strong-password constraints.
- **Video**: https://youtu.be/K0anQ9gQD1E

---

## Game Development with ZK

### kalepail/ohloss
**URL**: https://github.com/kalepail/ohloss
**Description**: OHLOSS games built using AI on Stellar. Demonstrates game development with blockchain integration.
- **Video**: https://www.youtube.com/watch?v=7-a1vxGm9vc

---

## ZK Application Projects on Stellar

### AshFrancis/zkvote
**URL**: https://github.com/AshFrancis/zkvote
**Description**: Zero-Knowledge DAO governance dApp with membership-tree, poseidon_params, and pairing security tests on Soroban.
- **Language**: TypeScript | **Last Updated**: December 2025

### MarxMad/puma-pay-campus-wallet
**URL**: https://github.com/MarxMad/puma-pay-campus-wallet
**Description**: Campus wallet (UNAM, Mexico) with **4 Noir circuits** (savings-proof, achievements, course-completion, user-verification) and an UltraHonk verifier contract using `rs-soroban-ultrahonk`.
- **Stars**: 1 | **Language**: TypeScript
- ZK features in development; genuinely consumes the UltraHonk library

### AradhyaMaheshwari/K-smos-The-Trust-Protocol
**URL**: https://github.com/AradhyaMaheshwari/K-smos-The-Trust-Protocol
**Description**: Privacy-first reputation/identity on Stellar using DIDs, VCs, and ZKPs.
- **Stars**: 1

---

## Key People — ZK on Stellar

### jayz22 (Jay Geng) — SDF, Central ZK Architect
**GitHub**: https://github.com/jayz22
**Role**: Stellar Development Foundation engineer. Created Groth16 verifier PoC, `rs-soroban-poseidon`, `stellar-confidential-token`, `stellar-confidential-transfer`, BLS signature example, `import_ark_bn254` example. Personal repos span Bulletproofs, range proofs, Poseidon2, privacy pools, UltraHonk, and ZK blackjack.
**Key repos**:
- `jayz22/stellar-confidential-transfer` (2 stars)
- `jayz22/stellar-confidential-token`
- `jayz22/zk-blackjack` (1 star)
- `jayz22/bulletproofs` — Pure-Rust Bulletproofs
- `jayz22/rangeproof` — Fork of Solana zk-sdk range_proof
- `jayz22/poseidon2`
- `jayz22/soroban-poseidon-contract-example`
- `jayz22/soroban-privacy-pools`, `jayz22/soroban-privacy-pools-local`
- `jayz22/ultrahonk_soroban_contract` (fork)

### Oghma (Matteo Lisotto) — Nethermind, RISC Zero Verifier Engineer
**GitHub**: https://github.com/Oghma
**Role**: Primary engineer on `NethermindEth/stellar-risc0-verifier`. Built the entire verifier stack (router, Groth16 verifier, mock verifier, timelock, interface, build utils).
**Key repos**: Forks of `risc0`, `rs-soroban-env`, `rs-soroban-sdk`, `stellar-core`

### NiDimi (Nick Dimitriou) — Nethermind, Cryptography Researcher
**GitHub**: https://github.com/NiDimi
**Role**: Applied cryptography at Nethermind. UCL MSc. Meridian 2025 speaker on "Private Transactions for Stellar."
**Key repos**: `CircomLOC`, `lasso` (a16z zkVM fork), `sonobe` (folding schemes), ZK Hack puzzle solutions

### ymcrcat (Yan Michalevsky) — Privacy Researcher
**GitHub**: https://github.com/ymcrcat
**Website**: michalevsky.com
**Role**: Authored the privacy-pools example in soroban-examples. Explores ZK, FHE, MPC, and TEE approaches to privacy on Stellar.
**Key repos**: `soroban-privacy-pools`, `dark-pool-fhe`, `dark-pool-mpc`, `stellar-dark-pool`, `MASHaBLE`

### teddav — @zksecurity, ZK Security Researcher
**GitHub**: https://github.com/teddav
**Role**: Contributed to `rs-soroban-poseidon`. Extensive ZK portfolio.
**Key repos**: `halo2-starter` (15 stars), `noir-recursive` (11 stars), `co-match.noir` (9 stars), `halo2-soundness-bugs` (9 stars), `tornado-halo2` (8 stars), `tdd.nr` (6 stars), `zk-tenant` (5 stars), `co-snarks`, `poseidon-gadget`, `awesome-noir`, `semaphore`, `sigma-proofs`

### yugocabrio — UltraHonk Core Developer
**GitHub**: https://github.com/yugocabrio
**Role**: Primary maintainer of UltraHonk Soroban verifier (244+ commits). Deep ZK cryptographer.
**Key repos**: `rs-soroban-ultrahonk`, `oreno-lookup` (5 stars, plookup/logup), `NTT_implementation` (3 stars), `arkworks_dapp_icp` (2 stars). Studies of Binius, Plonky3, Nova, Halo2, SP1, Nexus.

### indextree — UltraHonk Contract Developer
**GitHub**: https://github.com/indextree
**Role**: Key contributor to UltraHonk Soroban contract layer (65+ commits).
**Key repos**: `ultrahonk_soroban_contract` (9 stars), `gkr-approx-sumcheck` (6 stars), `lean_gaussianelimination`, `chiquito` (Halo2 DSL)

### tupui (Pamphile Roy) — NoirCon3 Presenter
**GitHub**: https://github.com/tupui
**Role**: Presented Noir/UltraHonk on Soroban at NoirCon3. Built zk-sudoku demo.
**Key repos**: `ultrahonk_soroban_contract`, `soroban-versioning` (Tansu, 26 stars, SCF Awards 28 & 30), `Academia` (NoirCon3 slides)

### willemolding (Willem Olding) — Independent ZK Engineer
**GitHub**: https://github.com/willemolding
**Role**: Contributor to stellar-risc0-verifier. Extraordinarily broad ZK experience.
**Key repos**: `risc0-mixer` (1 star), `noir_shuffle`, `zktron` (Noir light client), `zk-light-clients`, `steel` (fork), `ProofBoy` (4 stars), halo2 work, BLS12-381 work

### mysteryon88 (Sergey) — Web3 Security / ZK Tooling
**GitHub**: https://github.com/mysteryon88
**Role**: Built `soroban-verifier-gen`. Open PR on soroban-examples.
**Key repos**: `soroban-verifier-gen`, `ark-snarkjs` (1 star), `gnark-to-snarkjs`, `export-ton-verifier` (3 stars), `zk-hashes` (1 star), `zkToken`

### kalepail (Tyler van der Hoeven) — Stellar Ecosystem Builder
**GitHub**: https://github.com/kalepail
**Role**: Active ZK explorer across all three proving systems (Groth16, RISC Zero, Noir/UltraHonk).
**ZK repos**:
- `groth16_verifier` (3 stars) — Complete Groth16 ecosystem with React frontend. [Featured in Dev Meeting 12/19/2024](https://www.youtube.com/watch?v=51SitOUZySk)
- `zkp-maze` (1 star) — Noir/Barretenberg + RISC Zero maze game
- `zkp-pong` — Provably fair Pong with RISC Zero zkVM
- `risc-zero-test` — Early RISC Zero exploration
- `ultrahonk_soroban_contract` — Fork of indextree's verifier

### lumenbro — Independent ZK Infrastructure Builder
**GitHub**: https://github.com/lumenbro
**Role**: Building ZK infrastructure for Stellar independently.
**Key repos**: `stellar-groth16`, `zk-bridge`, `zk-l2-settlement`, `mopro-poseidon-bindings`, `stellar-smart-account`

### fboiero (Xcapit Labs) — OpenZKTool Author
**Role**: Primary developer of `xcapit/openzktool` (111 commits). Full BN254 pairing implementation in Rust for Soroban.

---

## OpenZeppelin Stellar Ecosystem

### OpenZeppelin/stellar-contracts
**URL**: https://github.com/OpenZeppelin/stellar-contracts
**Stars**: 75 | **Last Push**: February 2, 2026 (actively maintained)
**Description**: Production-grade Soroban contract library.

### OpenZeppelin/soroban-security-detectors-sdk
**URL**: https://github.com/OpenZeppelin/soroban-security-detectors-sdk
**Stars**: 6 | Automated vulnerability detection for Soroban

### OpenZeppelin/openzeppelin-monitor
**URL**: https://github.com/OpenZeppelin/openzeppelin-monitor
**Stars**: 119 | On-chain activity monitoring (supports Stellar)

### OpenZeppelin/ui-builder
**URL**: https://github.com/OpenZeppelin/ui-builder
**Stars**: 31 | Frontend component library (supports Stellar)

### OpenZeppelin/stellar-upgrader-cli
**URL**: https://github.com/OpenZeppelin/stellar-upgrader-cli
**Stars**: 3 | Contract upgrade CLI

### OpenZeppelin/soroban-helpers
**URL**: https://github.com/OpenZeppelin/soroban-helpers
**Stars**: 1 | Soroban utility helpers

### OpenZeppelin/role-manager
**URL**: https://github.com/OpenZeppelin/role-manager
**Stars**: 1 | Access control management (supports Stellar)

**Note**: `OpenZeppelin/openzeppelin-confidential-contracts` (143 stars) is Ethereum FHE (Zama FHEVM), **not** Stellar-related.

---

## Noir Lang Discussions

- **#8509**: UltraHonk Verifier for Soroban — https://github.com/orgs/noir-lang/discussions/8509
- **#8560**: Resource budget constraints on Soroban — https://github.com/orgs/noir-lang/discussions/8560
- **NRG#4 Grants**: Funding for UltraHonk Soroban Verifier (yugocabrio) and Interstellar (Rather Labs)

---

## Stellar Protocol Issues & PRs

- **rs-soroban-env #1642**: Add `is_on_curve` host functions (open)
- **stellar-protocol #1869**: CAP 81 draft (open)
- **stellar-dev-skill #2**: Add dedicated ZK skill for Protocol 25 X-Ray (open)

---

## Research & Academic

### Pamphile Roy (tupui) — NoirCon 3 Devconnect 2025
**Video**: https://www.youtube.com/watch?v=aUa8VeVdGGY
**GitHub**: https://github.com/tupui
**Slides**: `tupui/Academia/Conf/NoirCon3.pdf`
**Demo**: Noir sudoku verifier on Soroban. **Blog post expected February 2026.**

### ResearchGate — Verifying circuits on Stellar
**URL**: https://www.researchgate.net/publication/398247255

### Stellar Academic Research Grants
**URL**: https://research.stellar.org/research-grants
