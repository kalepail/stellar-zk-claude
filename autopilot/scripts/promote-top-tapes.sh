#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BENCH_DIR="${1:-$ROOT_DIR/benchmarks/finalists-latest}"
COUNT="${2:-5}"
SRC_DIR="$BENCH_DIR/top-objective"
DST_DIR="$ROOT_DIR/checkpoints"

if [[ ! -d "$SRC_DIR" ]]; then
  echo "missing top-objective dir: $SRC_DIR" >&2
  exit 1
fi

mkdir -p "$DST_DIR"

find "$SRC_DIR" -maxdepth 1 -type f -name 'rank*.tape' | sort | head -n "$COUNT" | while read -r tape; do
  base="$(basename "$tape" .tape)"
  json="$SRC_DIR/$base.json"
  cp "$tape" "$DST_DIR/"
  if [[ -f "$json" ]]; then
    cp "$json" "$DST_DIR/"
  fi
  echo "promoted: $base"
done

ls -1 "$DST_DIR" | sed 's/^/  /'
