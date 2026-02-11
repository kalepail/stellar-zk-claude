#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAB_DIR="$ROOT_DIR/codex-tuner"

ITERATIONS="${1:-6}"
CANDIDATES="${2:-6}"
MAX_FRAMES="${3:-108000}"
JOBS="${4:-8}"
SELECTION_METRIC="${5:-score}"

SEARCH_OUTPUT="$($LAB_DIR/scripts/iterative-search.py \
  --iterations "$ITERATIONS" \
  --candidates "$CANDIDATES" \
  --max-frames "$MAX_FRAMES" \
  --jobs "$JOBS" \
  --selection-metric "$SELECTION_METRIC" \
  --seeds-file "$LAB_DIR/seeds/screen-seeds.txt")"

echo "$SEARCH_OUTPUT"

SESSION_DIR="$(echo "$SEARCH_OUTPUT" | awk -F= '/^SESSION_DIR=/{print $2}')"
if [ -z "$SESSION_DIR" ]; then
  echo "failed to detect SESSION_DIR from iterative-search output" >&2
  exit 1
fi

VALIDATION_OUT="$SESSION_DIR/validation-score"

"$ROOT_DIR/target/release/rust-autopilot" benchmark \
  --bots codex-potential-adaptive,omega-marathon,omega-supernova,offline-wrap-endurancex \
  --seed-file "$LAB_DIR/seeds/validation-seeds.txt" \
  --max-frames 108000 \
  --objective score \
  --save-top 6 \
  --jobs "$JOBS" \
  --out-dir "$VALIDATION_OUT"

echo "VALIDATION_OUT=$VALIDATION_OUT"
