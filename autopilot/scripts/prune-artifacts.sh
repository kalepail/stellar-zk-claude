#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="dry-run" # dry-run|apply

usage() {
  cat <<'USAGE_EOF'
Usage: autopilot/scripts/prune-artifacts.sh [options]

Options:
  --mode <mode>   dry-run|apply (default: dry-run)
  -h, --help      Show this help
USAGE_EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$MODE" != "dry-run" && "$MODE" != "apply" ]]; then
  echo "ERROR: --mode must be dry-run or apply" >&2
  exit 1
fi

KEEP_CHECKPOINTS="$ROOT_DIR/records/keep-checkpoints.txt"
KEEP_BENCHMARKS="$ROOT_DIR/records/keep-benchmarks.txt"

if [[ ! -f "$KEEP_CHECKPOINTS" || ! -f "$KEEP_BENCHMARKS" ]]; then
  echo "missing keep list(s) in records/" >&2
  exit 1
fi

echo "mode=$MODE"

for tape in "$ROOT_DIR"/checkpoints/*.tape; do
  [[ -e "$tape" ]] || continue
  base="$(basename "$tape" .tape)"
  if ! rg -qx "$base" "$KEEP_CHECKPOINTS"; then
    echo "prune checkpoint: $base"
    if [[ "$MODE" == "apply" ]]; then
      rm -f "$tape" "$ROOT_DIR/checkpoints/$base.json"
    fi
  fi
done

for dir in "$ROOT_DIR"/benchmarks/*; do
  [[ -d "$dir" ]] || continue
  base="$(basename "$dir")"
  if ! rg -qx "$base" "$KEEP_BENCHMARKS"; then
    echo "prune benchmark: $base"
    if [[ "$MODE" == "apply" ]]; then
      rm -rf "$dir"
    fi
  fi
done

echo "done"
