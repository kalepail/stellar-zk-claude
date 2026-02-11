#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-4}"
FINAL_SEEDS="${2:-8}"
MAX_FRAMES="${3:-108000}" # 30 minutes at 60 FPS
SEED_START="${4:-0x00000001}"
JOBS="${5:-8}"

SCREEN_BOTS="${6:-$ACTIVE_BOTS}"
FINAL_BOTS="${7:-$ACTIVE_FINALISTS}"

SCREEN_SURV_OUT="$ROOT_DIR/benchmarks/objective2-screen-survival-$SCREEN_SEEDS-$STAMP"
SCREEN_SCORE_OUT="$ROOT_DIR/benchmarks/objective2-screen-score-$SCREEN_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/objective2-finals-survival-$FINAL_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/objective2-finals-score-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 30 --jobs "$JOBS" --out-dir "$SCREEN_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start "$SEED_START" --seed-count "$SCREEN_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 30 --jobs "$JOBS" --out-dir "$SCREEN_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective survival --save-top 40 --jobs "$JOBS" --out-dir "$FINAL_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start "$SEED_START" --seed-count "$FINAL_SEEDS" \
  --max-frames "$MAX_FRAMES" --objective score --save-top 40 --jobs "$JOBS" --out-dir "$FINAL_SCORE_OUT"

cat <<EOF_SUMMARY
Objective 2 (runtime-only, 30m cap) suite complete:
  $SCREEN_SURV_OUT
  $SCREEN_SCORE_OUT
  $FINAL_SURV_OUT
  $FINAL_SCORE_OUT
EOF_SUMMARY
