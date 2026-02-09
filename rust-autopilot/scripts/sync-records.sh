#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT_DIR/records/latest-roster-manifest.json"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- roster-manifest --output "$OUT"
echo "synced: $OUT"
