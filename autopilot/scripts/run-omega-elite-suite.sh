#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/bot-roster.sh"
STAMP="$(date -u +%s)"

SCREEN_SEEDS="${1:-512}"
FINAL_SEEDS="${2:-2048}"
MAX_FRAMES="${3:-200000}"
JOBS="${4:-8}"

SCREEN_SURV_OUT="$ROOT_DIR/benchmarks/omega-screen-survival-$SCREEN_SEEDS-$STAMP"
SCREEN_HYBRID_OUT="$ROOT_DIR/benchmarks/omega-screen-hybrid-$SCREEN_SEEDS-$STAMP"
FINALS_SURV_OUT="$ROOT_DIR/benchmarks/omega-finals-survival-$FINAL_SEEDS-$STAMP"
FINALS_HYBRID_OUT="$ROOT_DIR/benchmarks/omega-finals-hybrid-$FINAL_SEEDS-$STAMP"
FINALS_SCORE_OUT="$ROOT_DIR/benchmarks/omega-finals-score-$FINAL_SEEDS-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$ACTIVE_OMEGA_BOTS" --seed-start 0x00000001 --seed-count "$SCREEN_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 25 --jobs "$JOBS" --out-dir "$SCREEN_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$ACTIVE_OMEGA_BOTS" --seed-start 0x00000001 --seed-count "$SCREEN_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective hybrid --save-top 25 --jobs "$JOBS" --out-dir "$SCREEN_HYBRID_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$ACTIVE_OMEGA_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 30 --jobs "$JOBS" --out-dir "$FINALS_SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$ACTIVE_OMEGA_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective hybrid --save-top 30 --jobs "$JOBS" --out-dir "$FINALS_HYBRID_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$ACTIVE_OMEGA_BOTS" --seed-start 0x00000001 --seed-count "$FINAL_SEEDS" --max-frames "$MAX_FRAMES" \
  --objective score --save-top 30 --jobs "$JOBS" --out-dir "$FINALS_SCORE_OUT"

cat <<EOF_SUMMARY
Omega elite suite complete:
  $SCREEN_SURV_OUT
  $SCREEN_HYBRID_OUT
  $FINALS_SURV_OUT
  $FINALS_HYBRID_OUT
  $FINALS_SCORE_OUT
EOF_SUMMARY
