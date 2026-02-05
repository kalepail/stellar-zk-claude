# Docs Index

Routing layer for all documentation.

## Domains
- `zk/` (12 files) — Stellar + ZK protocol, tooling, ecosystem, onboarding, security.
- `games/asteroids/` (13 files) — Game design, verification rules, integer math, ZK proving strategy.

## Start Here
| Task | Go to |
|------|-------|
| Protocol, proving systems, privacy, toolchain, security | `zk/README.md` |
| Game design, verification, implementation | `games/asteroids/README.md` |
| ZK proving analysis (Noir vs RISC Zero) | `games/asteroids/noir-vs-risczero-analysis.md` |
| Integer math for ZK game logic | `games/asteroids/integer-math-reference.md` |
| Stellar ZK ecosystem overview | `zk/00-OVERVIEW.md` |

## Editing Rules
- Edit files inside the domain you are addressing. Do not mix ZK protocol updates into game docs.
- When adding a new document, link it from the nearest `README.md` in the same change.
