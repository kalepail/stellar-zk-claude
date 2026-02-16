#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${1:-$ROOT_DIR/shared/stellar/bindings/asteroids-score}"

env_value() {
  local key="$1"
  local file="$2"
  if [[ ! -f "$file" ]]; then
    return 1
  fi
  local value
  value="$(grep "^${key}=" "$file" | head -n 1 | cut -d= -f2- || true)"
  if [[ -z "$value" ]]; then
    return 1
  fi
  value="${value%\"}"
  value="${value#\"}"
  printf '%s' "$value"
}

SCORE_CONTRACT_ID="${SCORE_CONTRACT_ID:-$(env_value "VITE_SCORE_CONTRACT_ID" "$ROOT_DIR/.env")}"
RPC_URL="${RPC_URL:-$(env_value "VITE_RPC_URL" "$ROOT_DIR/.env")}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-$(env_value "VITE_NETWORK_PASSPHRASE" "$ROOT_DIR/.env")}"

if [[ -z "${SCORE_CONTRACT_ID:-}" ]]; then
  echo "missing SCORE_CONTRACT_ID (or VITE_SCORE_CONTRACT_ID in .env)" >&2
  exit 1
fi
if [[ -z "${RPC_URL:-}" ]]; then
  echo "missing RPC_URL (or VITE_RPC_URL in .env)" >&2
  exit 1
fi
if [[ -z "${NETWORK_PASSPHRASE:-}" ]]; then
  echo "missing NETWORK_PASSPHRASE (or VITE_NETWORK_PASSPHRASE in .env)" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT_DIR")"

echo "Generating TypeScript bindings for $SCORE_CONTRACT_ID"
echo "Output: $OUTPUT_DIR"

stellar contract bindings typescript \
  --contract-id "$SCORE_CONTRACT_ID" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --output-dir "$OUTPUT_DIR" \
  --overwrite

echo "Done."
