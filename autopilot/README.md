# Autopilot Lab

Rust-based autopilot programs for deterministic Asteroids tape generation, verification, and benchmarking.

This folder is the consolidated home for autopilot work (core bots, benchmarks, tuners, and evolution harnesses).

## Ruleset Hygiene Policy

- The maintained baseline is current rules only (`AST3` today).
- Legacy/incompatible tapes are retired when rules evolve.
- Code, algorithms, and benchmarking workflows are preserved; old result artifacts are not treated as canonical.

## Active Bot Roster

### Search bots

- `omega-marathon`
- `omega-lurk-breaker`
- `omega-ace`
- `omega-alltime-hunter`
- `omega-supernova`
- `evolve-candidate`

### Offline-control bots

- `offline-supernova-hunt`
- `offline-wrap-endurancex`
- `offline-wrap-sniper30`
- `offline-wrap-frugal-ace`
- `offline-wrap-apex-score`
- `offline-wrap-sureshot`

## Code Layout

- `src/bots/mod.rs`: core bot engines and planning/search logic
- `src/bots/roster.rs`: curated bot catalog + construction
- `src/benchmark.rs`: benchmark runner + report/tape export
- `src/runner.rs`: single-run execution + tape verification plumbing
- `tests/provable_tapes.rs`: smoke tests for roster tape generation
- `tests/champion_registry.rs`: registry + keep-list + strict artifact checks
- `records/`: champion provenance and keep-lists

## Quick Start

List bots:

```bash
cd autopilot
cargo run --release -- list-bots
```

Generate one tape:

```bash
cargo run --release -- generate   --bot omega-marathon   --seed 0xDEADBEEF   --max-frames 18000   --output checkpoints/omega-marathon-seeddeadbeef.tape
```

Verify a tape:

```bash
cargo run --release -- verify-tape   --input checkpoints/omega-marathon-seeddeadbeef.tape   --max-frames 108000
```

Run a benchmark:

```bash
cargo run --release -- benchmark   --bots offline-wrap-endurancex,offline-wrap-sniper30,omega-marathon,omega-ace   --seed-start 0x00000001   --seed-count 24   --max-frames 108000   --objective survival   --jobs 8
```

## Tuning and Evolution

- `codex-tuner/`: adaptive-profile tuner for `codex-potential-adaptive`
- `evolve/`: evolution loop for `evolve-candidate`
- `archive/claude-autopilot/`: archived standalone lab code (reference only)

## Scripted Benchmarks

Canonical roster defaults are centralized in `scripts/bot-roster.sh`.

- `./scripts/run-efficiency-elite-suite.sh`
- `./scripts/run-omega-top3-deep.sh`
- `./scripts/run-30m-breakability-hunt.sh`
- `./scripts/run-offline-alltime-parallel-hunt.sh`
- `./scripts/run-runtime-nonoffline-parallel-suite.sh`
- `./scripts/run-wrap-awareness-suite.sh`
- `./scripts/rebench-finalists.sh`
- `./scripts/sync-records.sh`
- `./scripts/prune-artifacts.sh` (`--mode apply` to execute deletion)

## Artifact Retention

- `checkpoints/` and `benchmarks/` are local artifact dirs and are gitignored.
- Promote only AST3-compatible artifacts to `records/champions.json` + keep-lists.
- Use strict checks before committing promoted artifacts:

```bash
AUTOPILOT_STRICT_ARTIFACTS=1 cargo test --release --manifest-path autopilot/Cargo.toml --test champion_registry
```
