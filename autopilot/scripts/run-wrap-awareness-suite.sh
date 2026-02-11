#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-4}"
FINAL_SEEDS="${2:-8}"
MAX_FRAMES="${3:-40000}"
JOBS="${4:-8}"

SCREEN_BOTS="offline-wrap-endurancex,offline-wrap-sniper30,offline-wrap-frugal-ace,offline-wrap-apex-score,offline-wrap-sureshot,omega-marathon"
FINAL_BOTS="offline-wrap-endurancex,offline-wrap-sniper30,offline-wrap-frugal-ace,offline-wrap-apex-score,offline-wrap-sureshot,offline-supernova-hunt,omega-marathon"
DUEL_BOTS="offline-wrap-endurancex,offline-wrap-sniper30,offline-wrap-frugal-ace,omega-marathon"

SCREEN_SURV_OUT="$ROOT_DIR/benchmarks/wrap-screen-survival-$SCREEN_SEEDS-$STAMP"
FINAL_SURV_OUT="$ROOT_DIR/benchmarks/wrap-finals-survival-$FINAL_SEEDS-$STAMP"
FINAL_HYBRID_OUT="$ROOT_DIR/benchmarks/wrap-finals-hybrid-$FINAL_SEEDS-$STAMP"
FINAL_SCORE_OUT="$ROOT_DIR/benchmarks/wrap-finals-score-$FINAL_SEEDS-$STAMP"
DUEL_SURV_OUT="$ROOT_DIR/benchmarks/wrap-duel-survival-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$SCREEN_BOTS" --seed-start 0x00000001 --seed-count "$SCREEN_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 20 --jobs "$JOBS" --out-dir "$SCREEN_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective hybrid --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_HYBRID_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$FINAL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective score --save-top 30 --jobs "$JOBS" --out-dir "$FINAL_SCORE_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$DUEL_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 30 --jobs "$JOBS" --out-dir "$DUEL_SURV_OUT"

cat <<EOF_SUMMARY
Wrap-awareness suite complete:
  $SCREEN_SURV_OUT
  $FINAL_SURV_OUT
  $FINAL_HYBRID_OUT
  $FINAL_SCORE_OUT
  $DUEL_SURV_OUT
EOF_SUMMARY
