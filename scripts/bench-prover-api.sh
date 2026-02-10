#!/usr/bin/env bash
set -euo pipefail

# Thin compatibility wrapper around the canonical segment-sweep benchmark.
# Keeps one benchmark execution path while preserving the old entrypoint.
#
# Defaults chosen to match prior bench-prover-api behavior:
# - segment_limit_po2: 19..21
# - receipts: composite only
# - tapes: short,medium,real (real is skipped automatically if missing)
# - repeat: 1

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SEGMENT_SWEEP_SCRIPT="$ROOT_DIR/scripts/bench-segment-sweep.sh"

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/bench-prover-api.sh [prover-url] [bench-segment-sweep options]

This command now delegates to scripts/bench-segment-sweep.sh with defaults:
  --seg-floor 19 --seg-ceiling 21 --receipts composite --tapes short,medium,real --repeat 1

Examples:
  bash scripts/bench-prover-api.sh
  bash scripts/bench-prover-api.sh http://127.0.0.1:8080
  bash scripts/bench-prover-api.sh http://127.0.0.1:8080 --repeat 2 --csv-out /tmp/bench.csv
  bash scripts/bench-prover-api.sh https://<vast-host>:<port> --repeat 2 --csv-out /tmp/bench.csv
USAGE_EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ ! -x "$SEGMENT_SWEEP_SCRIPT" ]]; then
  echo "ERROR: required script not found or not executable: $SEGMENT_SWEEP_SCRIPT" >&2
  exit 1
fi

PROVER_URL="http://127.0.0.1:8080"
if [[ $# -gt 0 && "$1" != --* ]]; then
  PROVER_URL="${1%/}"
  shift
fi

DEFAULT_ARGS=(
  --seg-floor 19
  --seg-ceiling 21
  --receipts composite
  --tapes short,medium,real
  --repeat 1
)

exec bash "$SEGMENT_SWEEP_SCRIPT" "$PROVER_URL" "${DEFAULT_ARGS[@]}" "$@"
