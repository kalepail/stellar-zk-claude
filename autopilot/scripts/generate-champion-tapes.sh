#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/checkpoints/regenerated}"

mkdir -p "$OUT_DIR"

run_gen() {
  local bot="$1"
  local seed="$2"
  local max_frames="$3"
  local name="$4"

  cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -- \
    generate \
    --bot "$bot" \
    --seed "$seed" \
    --max-frames "$max_frames" \
    --output "$OUT_DIR/$name.tape"
}

# Global record regenerations.
run_gen offline-wrap-endurancex 0x6046C93D 108000 rank01-offline-wrap-endurancex-seed6046c93d
run_gen offline-wrap-endurancex 0xFB2A7978 108000 rank02-offline-wrap-endurancex-seedfb2a7978
run_gen offline-wrap-endurancex 0xE35B682C 108000 rank03-offline-wrap-endurancex-seede35b682c
run_gen omega-marathon          0x6AFA2869 108000 rank01-omega-marathon-seed6afa2869

# Current elite challengers.
run_gen offline-wrap-frugal-ace 0x2CDBE3D6 108000 rank01-offline-wrap-frugal-ace-seed2cdbe3d6
run_gen offline-wrap-sniper30   0x855D1FD0 108000 rank02-offline-wrap-sniper30-seed855d1fd0
run_gen offline-wrap-apex-score 0x0CF06D60 108000 rank02-offline-wrap-apex-score-seed0cf06d60
run_gen offline-wrap-sureshot   0xA7105B28 108000 rank05-offline-wrap-sureshot-seeda7105b28
run_gen omega-alltime-hunter    0x9D00301A 108000 rank01-omega-alltime-hunter-seed9d00301a
run_gen omega-supernova         0x5EB7221A 108000 rank01-omega-supernova-seed5eb7221a

echo "Champion regeneration complete: $OUT_DIR"
