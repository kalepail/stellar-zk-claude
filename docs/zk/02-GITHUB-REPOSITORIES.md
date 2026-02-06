# GitHub Repositories for Stellar ZK Development

Last reviewed: February 2026.

This page keeps a **high-signal list** of active repositories. It intentionally
avoids exhaustive people catalogs and duplicate listings.

## Official Stellar Repositories
| Repository | Why It Matters |
|---|---|
| `stellar/soroban-examples` | Canonical example contracts (`groth16_verifier`, `privacy-pools`, crypto examples). |
| `stellar/stellar-protocol` | CAP specs and protocol-level source of truth. |
| `stellar/rs-soroban-env` | Host implementation details for crypto ops and cost models. |
| `stellar/rs-soroban-sdk` | Contract SDK used by all Rust/Soroban implementations. |
| `stellar/rs-soroban-poseidon` | Official Poseidon/Poseidon2 SDK for Soroban. |
| `stellar/stellar-confidential-token` | Confidential token prototype workstream. |

## RISC Zero on Stellar
| Repository | Why It Matters |
|---|---|
| `NethermindEth/stellar-risc0-verifier` | Main deployed verifier architecture (router, production verifier, mock verifier, governance timelock). |

Known deployment references (as documented in this repo's research set):
- Futurenet verifier: `CBO2CWWK6QWRTODDTHLADLZI6HDBTBCG34J5S4G7Y6ZAUUMH7RY3X2SZ`
- Testnet router/verifier/mock addresses are tracked in `03-PROVING-SYSTEMS.md` and `09-GETTING-STARTED.md`.

## Noir / UltraHonk on Stellar
| Repository | Why It Matters |
|---|---|
| `yugocabrio/rs-soroban-ultrahonk` | Consolidated home for UltraHonk verifier + Soroban integration work. |
| `indextree/ultrahonk_soroban_contract` | Early active contract-level implementation and experimentation. |
| `tupui/ultrahonk_soroban_contract` | NoirCon demo path and practical end-to-end example. |

## Groth16 / Circom Tooling
| Repository | Why It Matters |
|---|---|
| `ymcrcat/soroban-privacy-pools` | Full privacy pool implementation with supporting tools. |
| `mysteryon88/soroban-verifier-gen` | Contract generation from verification keys. |
| `xcapit/openzktool` | Broader toolkit and SDK pattern for Groth16 flows. |
| `zk-examples/zk-soroban-examples` | Multi-framework examples targeting Soroban. |

## External Integrations
| Repository | Why It Matters |
|---|---|
| `reclaimprotocol/zkfetch-stellar-example` | ZK-verified external data workflow on Stellar. |

## How To Use This List
- Start with official repos.
- Pick one proving stack and follow its dedicated implementation repo.
- Track protocol CAP changes before locking production assumptions.

