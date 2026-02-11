#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/benchmarks/score-$(date -u +%s)}"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark \
  --seed-file "$ROOT_DIR/seeds/score-seeds.txt" \
  --max-frames 18000 \
  --objective score \
  --save-top 5 \
  --out-dir "$OUT_DIR"

echo "score benchmark complete: $OUT_DIR"
