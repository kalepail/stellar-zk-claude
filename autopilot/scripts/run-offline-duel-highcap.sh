#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SEED_COUNT="${1:-24}"
MAX_FRAMES="${2:-500000}"
BOTS="${3:-$ACTIVE_OFFLINE_BOTS,omega-marathon}"
JOBS="${4:-8}"

OUT_DIR="$ROOT_DIR/benchmarks/offline-duel-survival-$SEED_COUNT-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$BOTS" --seed-start 0x00000001 --seed-count "$SEED_COUNT" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 35 --jobs "$JOBS" --out-dir "$OUT_DIR"

cat <<EOF_SUMMARY
Offline high-cap duel complete:
  $OUT_DIR
EOF_SUMMARY
