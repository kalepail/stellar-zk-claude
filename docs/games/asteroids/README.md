# Asteroids Docs Guide

Asteroids-specific game design, verification, ZK proving strategy, and implementation research.

## Canonical Docs
- `asteroids-research-compendium-2026-02-05.md` — Primary synthesis of mechanics, architecture, and final decisions.
- `verification-rules.md` — Validation rules and pass/fail criteria.
- `feature-decisions.md` — Product decisions, tradeoffs, and deliberate omissions.
- `difficulty-scaling.md` — Difficulty progression and balancing model.
- `integer-math-reference.md` — Fixed-point formats (Q12.4, Q8.8, BAM angles), trig tables, and ZK-friendly arithmetic patterns.

## ZK Proving Strategy
- `noir-vs-risczero-analysis.md` — **Primary.** Comprehensive Noir vs RISC Zero comparison with circuit sizing, proving times, Stellar on-chain verification, pseudo code, broader ZK gaming ecosystem, and implementation roadmap.
- `risc0-initial-circuit-build-codex.md` — Initial RISC0 host/guest implementation pass and gap analysis.
- `codex-asteroids-noir-vs-risc0-research-2026-02-05.md` — Earlier Noir vs RISC Zero comparison (Codex-generated).
- `Kimi-NOIR-vs-RISC-Zero-Research-Report.md` — Noir vs RISC Zero report (Kimi-generated).
- `Kimi-ZK-Verification-Strategy-and-Implementation-Guide.md` — End-to-end ZK verification strategy (Kimi-generated).
- `codex-verification-rules-engine.md` — Deterministic rules-engine specification for tape verification.

## Research History
- `asteroids-research-log.md` — Chronological research log with retries and traceability.
- `asteroids-deep-research-2026-02-05.md` — Extended analysis pass for fidelity and performance.

## Edit Routing
| Change | Edit |
|--------|------|
| Gameplay intent | `asteroids-research-compendium-2026-02-05.md` |
| Validation logic | `verification-rules.md` + `codex-verification-rules-engine.md` |
| Product scope | `feature-decisions.md` |
| Difficulty pacing | `difficulty-scaling.md` |
| Fixed-point math | `integer-math-reference.md` |
| Proving-system selection | `noir-vs-risczero-analysis.md` |

## Maintenance
- Date-stamped files are immutable except for corrections.
- Add new files to this README immediately.
