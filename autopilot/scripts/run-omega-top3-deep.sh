#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%s)"
SEED_COUNT="${1:-8192}"
MAX_FRAMES="${2:-200000}"
BOTS="${3:-omega-marathon,omega-ace,omega-lurk-breaker}"

SURV_OUT="$ROOT_DIR/benchmarks/omega-top3-survival-$SEED_COUNT-$STAMP"
SCORE_OUT="$ROOT_DIR/benchmarks/omega-top3-score-$SEED_COUNT-$STAMP"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$BOTS" --seed-start 0x00000001 --seed-count "$SEED_COUNT" --max-frames "$MAX_FRAMES" \
  --objective survival --save-top 40 --out-dir "$SURV_OUT"

cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
  benchmark --bots "$BOTS" --seed-start 0x00000001 --seed-count "$SEED_COUNT" --max-frames "$MAX_FRAMES" \
  --objective score --save-top 40 --out-dir "$SCORE_OUT"

cat <<EOF_SUMMARY
Omega top-3 deep suite complete:
  survival: $SURV_OUT
  score:    $SCORE_OUT
EOF_SUMMARY
