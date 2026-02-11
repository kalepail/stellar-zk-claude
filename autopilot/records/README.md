# Records Registry

This folder is the source of truth for retained autopilot artifacts.

## Files

- `champions.json`: promoted champion run registry for the current ruleset.
- `keep-checkpoints.txt`: checkpoint basenames to retain locally.
- `keep-benchmarks.txt`: benchmark directories to retain locally.
- `latest-roster-manifest.json`: exported roster + config fingerprints.

## Ruleset Policy

- Registry starts empty after a ruleset reset (AST3 reset completed on 2026-02-11).
- Legacy/incompatible artifacts are not retained as canonical.
- Promote only tapes that parse and verify under current rules.

## Workflow

1. Run new benchmarks under current rules.
2. Promote new tapes/benchmarks into `champions.json` and keep-lists.
3. Sync manifest:
   - `cargo run --release --manifest-path autopilot/Cargo.toml -- roster-manifest --output autopilot/records/latest-roster-manifest.json`
4. Run checks:
   - `cargo test --release --manifest-path autopilot/Cargo.toml`
   - `AUTOPILOT_STRICT_ARTIFACTS=1 cargo test --release --manifest-path autopilot/Cargo.toml --test champion_registry`
