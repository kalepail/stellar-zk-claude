# Records Registry

This folder is the source of truth for artifact retention and provenance.

## Files

- `champions.json`: canonical champion/record run registry with bot fingerprint + source reference.
- `keep-checkpoints.txt`: checkpoint basenames that must be retained.
- `keep-benchmarks.txt`: benchmark directories that must be retained.
- `latest-roster-manifest.json`: exported roster + config fingerprints (`cargo run --release -- roster-manifest`).

## Workflow

1. Export latest bot manifest:
   - `cargo run --release -- roster-manifest --output records/latest-roster-manifest.json`
2. Update `records/champions.json` whenever a new record/champion is promoted.
3. Keep `records/keep-checkpoints.txt` and `records/keep-benchmarks.txt` aligned with `champions.json`.
4. Run `cargo test --release` to enforce registry integrity checks (bot roster + fingerprints).
5. Optional strict mode (also enforces artifact directories/files exist):
   - `AUTOPILOT_STRICT_ARTIFACTS=1 cargo test --release`
6. Optional cleanup:
   - `./scripts/prune-artifacts.sh` (dry-run)
   - `./scripts/prune-artifacts.sh --mode apply`

## Safety guarantee

`tests/champion_registry.rs` verifies that champion bots still exist and fingerprints still match. This prevents accidental bot refactors/deletions from silently invalidating promoted records.

When `AUTOPILOT_STRICT_ARTIFACTS=1` is set, the tests also verify that all referenced tapes/metadata and benchmark directories exist on disk.

Note: `checkpoints/` and `benchmarks/` are gitignored by default (see `.gitignore`). Strict mode is intended for local workspaces where those artifacts are present.
