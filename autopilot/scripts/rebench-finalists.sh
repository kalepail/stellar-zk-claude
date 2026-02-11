#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"

OUT_DIR="${1:-$ROOT_DIR/benchmarks/finalists-$(date -u +%s)}"
BOTS="${2:-$ACTIVE_FINALISTS}"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark \
  --bots "$BOTS" \
  --seed-file "$ROOT_DIR/seeds/survival-seeds.txt" \
  --max-frames 54000 \
  --objective hybrid \
  --save-top 8 \
  --out-dir "$OUT_DIR"

echo "finalists benchmark complete: $OUT_DIR"
