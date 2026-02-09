#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
OUT_DIR="${1:-$ROOT_DIR/benchmarks/unbounded-$(date -u +%s)}"
MAX_FRAMES="${2:-200000}"
SEED_COUNT="${3:-2048}"
SEED_START="${4:-0x00000001}"
BOTS="${5:-$ACTIVE_NON_OFFLINE_FINALISTS}"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark \
  --bots "$BOTS" \
  --seed-start "$SEED_START" \
  --seed-count "$SEED_COUNT" \
  --max-frames "$MAX_FRAMES" \
  --objective survival \
  --save-top 30 \
  --out-dir "$OUT_DIR"

echo "unbounded survival benchmark complete: $OUT_DIR"
