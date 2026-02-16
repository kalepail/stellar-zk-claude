#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/checkpoints/regenerated}"
PAIRS_FILE="${1:-}"

usage() {
  cat <<'USAGE_EOF'
Usage: autopilot/scripts/generate-champion-tapes.sh <pairs-file>

Pairs file format (CSV, no header):
  bot_id,seed_hex,max_frames,output_basename

Example line:
  omega-marathon,0xDEADBEEF,108000,rank01-omega-marathon-seeddeadbeef

Environment:
  OUT_DIR=<path>   Output directory (default: autopilot/checkpoints/regenerated)
USAGE_EOF
}

if [[ -z "$PAIRS_FILE" || "$PAIRS_FILE" == "-h" || "$PAIRS_FILE" == "--help" ]]; then
  usage
  exit 0
fi

if [[ ! -f "$PAIRS_FILE" ]]; then
  echo "missing pairs file: $PAIRS_FILE" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

while IFS=',' read -r bot seed max_frames name; do
  bot="${bot//[[:space:]]/}"
  seed="${seed//[[:space:]]/}"
  max_frames="${max_frames//[[:space:]]/}"
  name="${name//[[:space:]]/}"

  [[ -z "$bot" || -z "$seed" || -z "$max_frames" || -z "$name" ]] && continue
  [[ "${bot:0:1}" == "#" ]] && continue

  cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
    generate \
    --bot "$bot" \
    --seed "$seed" \
    --max-frames "$max_frames" \
    --output "$OUT_DIR/$name.tape"
done < "$PAIRS_FILE"

echo "Champion regeneration complete: $OUT_DIR"
