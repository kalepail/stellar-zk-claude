#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AUTOPILOT_DIR="$ROOT_DIR/rust-autopilot"
MAX_FRAMES="${1:-60000}"
VERIFIER_DIR="$ROOT_DIR/risc0-asteroids-verifier"
CLAIMANT_ADDRESS="GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

for tape in "$AUTOPILOT_DIR"/checkpoints/*.tape; do
  echo "==> verifying $tape"
  (
    cd "$VERIFIER_DIR"
    RISC0_DEV_MODE=1 cargo run -p host --release --no-default-features -- \
      --tape "$tape" \
      --max-frames "$MAX_FRAMES" \
      --receipt-kind composite \
      --proof-mode dev \
      --claimant-address "$CLAIMANT_ADDRESS"
  )
  echo
done
