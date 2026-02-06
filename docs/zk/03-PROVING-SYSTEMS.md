# Proving Systems on Stellar

Last reviewed: February 2026.

## Selection by Workload
| Workload | Best First Choice | Why |
|---|---|---|
| Compact statement verification | Groth16 | Mature Soroban path and small on-chain footprint |
| Deterministic replay / state transition proving | RISC Zero | Program model matches replay loops and complex logic |
| Circuit-native apps with Noir stack | Noir/UltraHonk | Strong DSL ergonomics, but integration constraints still apply |

## Comparison (Builder View)
| Dimension | Groth16 (Circom) | RISC Zero (zkVM) | Noir / UltraHonk |
|---|---|---|---|
| Maturity on Stellar | High | Medium-High | Medium |
| Programming model | Circuit DSL | Rust program | Circuit DSL |
| Trusted setup | Yes | No | No |
| Replay-heavy workload fit | Medium | High | Medium (chunking often required) |
| Main risk | Ceremony/process overhead | Cross-engine determinism parity | Cost/size optimization complexity |

## Deterministic Replay Guidance
For workload shapes like long replay tapes or large transition traces:
- Prefer zkVM path first unless circuit-native proving is a hard requirement.
- Keep canonical transition order and numeric model explicit.
- Treat proving-system swap as a versioned architectural migration.

## RISC Zero Recursion Pipeline

RISC Zero uses recursive proving to achieve constant proof size regardless of computation:

```
Execution → Segments → SegmentReceipts (composite)
                            ↓ lift
                       SuccinctReceipts (STARK)
                            ↓ join (pairwise)
                       Single SuccinctReceipt (~200 KB)
                            ↓ identity_p254
                            ↓ groth16 wrap
                       Groth16Receipt (~300 bytes)
```

Three circuit layers:
1. **RISC-V Circuit** — STARK proving correct program execution.
2. **Recursion Circuit** — Specialized STARK for lift/join/resolve operations.
3. **STARK-to-SNARK Circuit** — R1CS circuit converting to Groth16 for on-chain verification.

Receipt types and when to use them:
| Type | Size | Use case |
|------|------|----------|
| Composite | ~hundreds of KB | Development, testing, fastest to generate |
| Succinct | ~200 KB | Off-chain verification, intermediate step |
| Groth16 | ~300 bytes | **On-chain verification** (required for Stellar) |

For Stellar, use **groth16** receipts. The Stellar verifier contracts
(NethermindEth/stellar-risc0-verifier) only accept Groth16 proofs.

## Known Signals (As of Feb 2026)
- Groth16 verification remains the most operationally stable path on Soroban.
- RISC Zero has an active Stellar verifier stack in ecosystem repos.
- UltraHonk on Soroban is promising but still sensitive to resource budgets.

Treat cost numbers as time-bound; benchmark before release.
