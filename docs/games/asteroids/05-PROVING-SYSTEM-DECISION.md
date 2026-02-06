# Asteroids Proving System Decision

## Decision
- Primary implementation: **RISC Zero**.
- Secondary R&D: Noir/UltraHonk only for explicit product needs.

## Why RISC Zero
- Replay workload is program-like and maps directly to Rust execution.
- Avoids immediate circuit chunking/aggregation complexity for long runs.
- Current ecosystem path to Stellar verification is more mature.

## Why Noir Is Not Primary (Today)
- Long replay proofs require heavier circuit architecture and optimization work.
- Soroban resource and contract-size constraints remain important.

## Revisit Triggers
Re-evaluate decision if:
- Soroban verifier economics/limits materially improve.
- Noir verifier deployments become routine within acceptable limits.
- Browser-native proving becomes a hard product requirement.

## Implementation Rule
Do not split production correctness across proving systems in the same release;
ship one canonical verification path per version.
