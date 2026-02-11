#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-8}"
FINAL_SEEDS="${2:-24}"
MAX_FRAMES="${3:-108000}" # 30 minutes @ 60 FPS
JOBS="${4:-10}"
SEED_START="${5:-0x6afa2869}"

SCREEN_BOTS="${6:-$ACTIVE_BOTS}"
FINAL_BOTS="${7:-$ACTIVE_FINALISTS}"

SCREEN_HYBRID_OUT="$ROOT_DIR/benchmarks/efficiency-screen-hybrid-$SCREEN_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/efficiency-finals-survival-$FINAL_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/efficiency-finals-score-$FINAL_SEEDS-$STAMP"
FINAL_HYBRID_OUT="$ROOT_DIR/benchmarks/efficiency-finals-hybrid-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective hybrid --save-top 40 --jobs "$JOBS" \
  --out-dir "$SCREEN_HYBRID_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 60 --jobs "$JOBS" \
  --out-dir "$FINAL_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 60 --jobs "$JOBS" \
  --out-dir "$FINAL_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective hybrid --save-top 60 --jobs "$JOBS" \
  --out-dir "$FINAL_HYBRID_OUT"

cat <<EOF_SUMMARY
Efficiency elite suite complete:
  $SCREEN_HYBRID_OUT
  $FINAL_SURV_OUT
  $FINAL_SCORE_OUT
  $FINAL_HYBRID_OUT
EOF_SUMMARY
