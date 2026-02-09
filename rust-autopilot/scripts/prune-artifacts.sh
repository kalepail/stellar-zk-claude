#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPLY=false
if [[ "${1:-}" == "--apply" ]]; then
  APPLY=true
fi

KEEP_CHECKPOINTS="$ROOT_DIR/records/keep-checkpoints.txt"
KEEP_BENCHMARKS="$ROOT_DIR/records/keep-benchmarks.txt"

if [[ ! -f "$KEEP_CHECKPOINTS" || ! -f "$KEEP_BENCHMARKS" ]]; then
  echo "missing keep list(s) in records/" >&2
  exit 1
fi

echo "mode=$([[ "$APPLY" == true ]] && echo apply || echo dry-run)"

for tape in "$ROOT_DIR"/checkpoints/*.tape; do
  [[ -e "$tape" ]] || continue
  base="$(basename "$tape" .tape)"
  if ! rg -qx "$base" "$KEEP_CHECKPOINTS"; then
    echo "prune checkpoint: $base"
    if [[ "$APPLY" == true ]]; then
      rm -f "$tape" "$ROOT_DIR/checkpoints/$base.json"
    fi
  fi
done

for dir in "$ROOT_DIR"/benchmarks/*; do
  [[ -d "$dir" ]] || continue
  base="$(basename "$dir")"
  if ! rg -qx "$base" "$KEEP_BENCHMARKS"; then
    echo "prune benchmark: $base"
    if [[ "$APPLY" == true ]]; then
      rm -rf "$dir"
    fi
  fi
done

echo "done"
