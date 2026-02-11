# Autopilot Lab

Rust-based autopilot programs for deterministic Asteroids tape generation, validation, and benchmarking.

This folder is the consolidated home for all autopilot work (core bots, benchmarks, tuners, and evolution harnesses).

## Active Bot Roster

The project now keeps a curated, high-performing set only.

### Search bots (runtime-first)

- `omega-marathon`
- `omega-lurk-breaker`
- `omega-ace`
- `omega-alltime-hunter`
- `omega-supernova`
- `evolve-candidate` (used by the `evolve/` loop)

### Offline-control bots (deeper per-frame planning)

- `offline-supernova-hunt`
- `offline-wrap-endurancex`
- `offline-wrap-sniper30`
- `offline-wrap-frugal-ace`
- `offline-wrap-apex-score`
- `offline-wrap-sureshot`

### Record-lock bot

- `record-lock-endurancex-6046c93d` (replays the canonical all-time tape for seed `0x6046C93D`)

## Code Layout

- `src/bots/mod.rs`: core bot engines, physics-aware targeting, planning/search logic
- `src/bots/roster.rs`: curated bot catalog + construction (`bot_ids`, `describe_bots`, `create_bot`)
- `src/benchmark.rs`: benchmark runner + report/tape export
- `src/runner.rs`: single-run execution + tape verification plumbing
- `tests/provable_tapes.rs`: smoke tests ensuring all roster bots generate valid tapes
- `tests/champion_registry.rs`: validates champion registry, fingerprints, and artifact keep-lists
- `records/`: champion provenance + keep-lists + roster manifest snapshots

## Quick Start

List bots:

```bash
cd autopilot
cargo run --release -- list-bots
```

Generate one tape:

```bash
cargo run --release -- generate \
  --bot omega-marathon \
  --seed 0xDEADBEEF \
  --max-frames 18000 \
  --output checkpoints/omega-marathon-seeddeadbeef.tape
```

Verify an existing tape against current rules:

```bash
cargo run --release -- verify-tape \
  --input checkpoints/rank01-offline-wrap-endurancex-seed6046c93d-score289810-frames67109.tape \
  --max-frames 108000
```

Run a direct benchmark:

```bash
cargo run --release -- benchmark \
  --bots offline-wrap-endurancex,offline-wrap-sniper30,omega-marathon,omega-ace \
  --seed-start 0x00000001 \
  --seed-count 24 \
  --max-frames 108000 \
  --objective survival \
  --jobs 8
```

Export bot manifest + fingerprints:

```bash
cargo run --release -- roster-manifest --output records/latest-roster-manifest.json
```

## Tuning & Evolution

- `codex-tuner/`: automated adaptive-profile tuner for `codex-potential-adaptive`
  - Run: `./codex-tuner/scripts/run-super-score-loop.sh`
- `evolve/`: harness-agnostic evolution loop for the `evolve-candidate` SearchBot config
  - Run: `./evolve/evolve.sh --harness codex` (or `claude`, `goose`, etc.)
- `archive/claude-autopilot/`: standalone action-search lab kept for reference/comparison

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

## Output Artifacts

Each benchmark output directory contains:

- `summary.json` complete structured benchmark report
- `runs.csv` per-run metrics (`action_frames`, `turn_frames`, `thrust_frames`, `fire_frames` included)
- `rankings.csv` aggregate leaderboard
- `top-objective/`, `top-score/`, `top-survival/` tapes + metadata JSON

## Proof Compatibility

Each generated tape is verified immediately by `asteroids-verifier-core::verify_tape` with the provided frame bound, so all outputs remain rule-abiding and reproducible.
