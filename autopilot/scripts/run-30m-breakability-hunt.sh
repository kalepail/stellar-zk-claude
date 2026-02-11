#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SEEDS="${1:-48}"
MAX_FRAMES="${2:-108000}" # 30 minutes @ 60 FPS
JOBS="${3:-10}"
SEED_START="${4:-0x6afa2869}"
BOTS="${5:-$ACTIVE_FINALISTS}"

OUT_DIR="$ROOT_DIR/benchmarks/breakability-30m-survival-$SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$BOTS" --seed-start "$SEED_START" --seed-count "$SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 100 --jobs "$JOBS" \
  --out-dir "$OUT_DIR"

cat <<EOF_SUMMARY
30-minute breakability hunt complete:
  $OUT_DIR
EOF_SUMMARY
