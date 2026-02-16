#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-8}"
FINAL_SEEDS="${2:-16}"
MAX_FRAMES="${3:-108000}" # 30 minutes @ 60 FPS
JOBS="${4:-8}"
SEED_START="${5:-0x00000001}"

SCREEN_BOTS="${6:-$ACTIVE_OMEGA_BOTS}"
FINAL_BOTS="${7:-$ACTIVE_NON_OFFLINE_FINALISTS}"

SCREEN_SURV_OUT="$ROOT_DIR/benchmarks/runtime-screen-survival-$SCREEN_SEEDS-$STAMP"
SCREEN_SCORE_OUT="$ROOT_DIR/benchmarks/runtime-screen-score-$SCREEN_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/runtime-finals-survival-$FINAL_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/runtime-finals-score-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 30 --jobs "$JOBS" \
  --out-dir "$SCREEN_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 30 --jobs "$JOBS" \
  --out-dir "$SCREEN_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 40 --jobs "$JOBS" \
  --out-dir "$FINAL_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 40 --jobs "$JOBS" \
  --out-dir "$FINAL_SCORE_OUT"

cat <<EOF_SUMMARY
Runtime non-offline parallel suite complete:
  $SCREEN_SURV_OUT
  $SCREEN_SCORE_OUT
  $FINAL_SURV_OUT
  $FINAL_SCORE_OUT
EOF_SUMMARY
