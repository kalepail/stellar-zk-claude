# Games Docs Guide

Game-specific product documentation, verification logic, and ZK proving strategy.

Does not own generic Stellar + ZK ecosystem docs — those live in `docs/zk/`.

## Game Domains
- `asteroids/` — Asteroids implementation, design decisions, verification rules, and ZK proving strategy (13 files).

## Start Here
- Full file map: `asteroids/README.md`
- Game design synthesis: `asteroids/asteroids-research-compendium-2026-02-05.md`
- ZK proving analysis: `asteroids/noir-vs-risczero-analysis.md`
- Integer math reference: `asteroids/integer-math-reference.md`

## Maintenance
- Each game subfolder must have its own `README.md`.
- When adding a new game folder, link it here in the same change.
