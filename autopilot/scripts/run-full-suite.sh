#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%s)"

SCORE_OUT="$ROOT_DIR/benchmarks/full-score-$STAMP"
SURV_OUT="$ROOT_DIR/benchmarks/full-survival-$STAMP"
FINAL_OUT="$ROOT_DIR/benchmarks/full-finalists-$STAMP"

"$ROOT_DIR/scripts/run-score-bench.sh" "$SCORE_OUT"
"$ROOT_DIR/scripts/run-survival-bench.sh" "$SURV_OUT"
"$ROOT_DIR/scripts/rebench-finalists.sh" "$FINAL_OUT"

cat <<EOF_SUMMARY
Full suite complete:
  score:     $SCORE_OUT
  survival:  $SURV_OUT
  finalists: $FINAL_OUT
EOF_SUMMARY
