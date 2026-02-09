#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-12}"
FINAL_SEEDS="${2:-24}"
MAX_FRAMES="${3:-250000}"
JOBS="${4:-8}"

SCREEN_BOTS="${5:-$ACTIVE_BOTS}"
FINAL_BOTS="${6:-$ACTIVE_FINALISTS}"

SCREEN_SURV_OUT="$ROOT_DIR/benchmarks/offline-screen-survival-$SCREEN_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/offline-finals-survival-$FINAL_SEEDS-$STAMP"
FINAL_HYBRID_OUT="$ROOT_DIR/benchmarks/offline-finals-hybrid-$FINAL_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/offline-finals-score-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start 0x00000001 --seed-count "$SCREEN_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 25 --jobs "$JOBS" --out-dir "$SCREEN_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective hybrid --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_HYBRID_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective score --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_SCORE_OUT"

cat <<EOF_SUMMARY
Offline optimal-control suite complete:
  $SCREEN_SURV_OUT
  $FINAL_SURV_OUT
  $FINAL_HYBRID_OUT
  $FINAL_SCORE_OUT
EOF_SUMMARY
