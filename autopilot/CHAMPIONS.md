# Champion Bots (Current)

Last reset: 2026-02-11 (AST3 compatibility reset).

## Status

- No canonical champion tapes are currently promoted.
- Legacy AST0 artifacts were intentionally retired.
- Current focus is rebuilding a fresh AST3 leaderboard from current bot families.

## Active Roster (Maintained)

- `omega-marathon`, `omega-lurk-breaker`, `omega-ace`, `omega-alltime-hunter`, `omega-supernova`
- `offline-wrap-endurancex`, `offline-wrap-sniper30`, `offline-wrap-frugal-ace`, `offline-wrap-apex-score`, `offline-wrap-sureshot`, `offline-supernova-hunt`

## Promotion Policy

- Promote only AST3-verifiable tapes.
- Update all of:
  - `records/champions.json`
  - `records/keep-checkpoints.txt`
  - `records/keep-benchmarks.txt`
- Run strict registry checks before commit:

```bash
AUTOPILOT_STRICT_ARTIFACTS=1 cargo test --release --manifest-path autopilot/Cargo.toml --test champion_registry
```
