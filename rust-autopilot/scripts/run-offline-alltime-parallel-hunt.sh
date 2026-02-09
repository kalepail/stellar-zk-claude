#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-8}"
FINAL_SEEDS="${2:-24}"
MAX_FRAMES="${3:-108000}" # 30 minutes @ 60 FPS cap
JOBS="${4:-8}"
SEED_START="${5:-0x00000001}"

SCREEN_BOTS="${6:-$ACTIVE_BOTS}"
FINAL_BOTS="${7:-$ACTIVE_FINALISTS}"

SCREEN_SCORE_OUT="$ROOT_DIR/benchmarks/offline-alltime-screen-score-$SCREEN_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/offline-alltime-finals-score-$FINAL_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/offline-alltime-finals-survival-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 40 --jobs "$JOBS" \
  --out-dir "$SCREEN_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 60 --jobs "$JOBS" \
  --out-dir "$FINAL_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 60 --jobs "$JOBS" \
  --out-dir "$FINAL_SURV_OUT"

cat <<EOF_SUMMARY
Offline all-time parallel hunt complete:
  $SCREEN_SCORE_OUT
  $FINAL_SCORE_OUT
  $FINAL_SURV_OUT
EOF_SUMMARY
