# Asteroids ZK Overview

## Product Goal
Provide a provably fair Asteroids score submission flow where a player's tape is
replayed deterministically, proven, and settled on Stellar.

## Core Decisions
- Verification model: **full deterministic replay** from seed + frame inputs.
- Authoritative math: **integer/fixed-point only** in consensus-critical logic.
- Primary proving path: **RISC Zero**.
- Secondary R&D path: Noir/UltraHonk only if product constraints require it.
- Integrity anchors: final score and final gameplay RNG state.

## Canonical Tape Shape
- Header: magic/version/seed/frameCount
- Body: one input byte per frame (left/right/thrust/fire)
- Footer: finalScore/finalRngState/checksum

## Security Model (Short)
- Player controls only frame inputs, never direct state values.
- Verifier recomputes all state transitions.
- Any transition/order mismatch is a rejection.

## Where To Read Next
1. `01-GAME-SPEC.md`
2. `02-VERIFICATION-SPEC.md`
3. `04-INTEGER-MATH-SPEC.md`
4. `05-PROVING-SYSTEM-DECISION.md`
