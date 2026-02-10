#!/usr/bin/env bash
# regenerate-proofs.sh
#
# Regenerate mintable Groth16 proof fixtures from source tapes.
# Zero-score tapes are expected to be rejected.
# The prover is single-flight, so tapes are submitted sequentially.
# Each generated proof is verified on-chain via the router.
#
# Usage:
#   ./scripts/regenerate-proofs.sh [prover-url]
#   ./scripts/regenerate-proofs.sh
#   ./scripts/regenerate-proofs.sh https://<vast-host>:<port>
#
# Prerequisites:
#   - `bun` installed
#   - `stellar` CLI v25+
#   - Prover API running and accessible
#   - Source tapes in test-fixtures/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/_helpers.sh"

require_cmds stellar bun curl xxd

GENERATE_SCRIPT="$SCRIPT_DIR/generate-proof.ts"
CALLER_NAME="ast-regen-caller"

PASSED=0
FAILED=0
TOTAL=0

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
usage() {
  cat <<'USAGE_EOF'
Usage: stellar-asteroids-contract/scripts/regenerate-proofs.sh [prover-url]

Defaults:
  prover-url  http://127.0.0.1:8080

Examples:
  ./scripts/regenerate-proofs.sh
  ./scripts/regenerate-proofs.sh https://<vast-host>:<port>
USAGE_EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

PROVER_URL="${1:-http://127.0.0.1:8080}"
CLAIMANT_ADDRESS="${CLAIMANT_ADDRESS:-}"

# ---------------------------------------------------------------------------
# Regenerate + verify a single fixture
# ---------------------------------------------------------------------------
regenerate_fixture() {
  local label="$1" tape_file="$2" out_prefix="$3"

  TOTAL=$((TOTAL + 1))
  info "Regenerating: $label"

  local tape_path="$FIXTURES_DIR/$tape_file"
  if [[ ! -f "$tape_path" ]]; then
    err "Tape not found: $tape_path"
    FAILED=$((FAILED + 1))
    return
  fi

  # Generate proof (writes .seal, .journal_raw, .image_id)
  info "Submitting tape to prover..."
  if ! bun run "$GENERATE_SCRIPT" \
    --tape "$tape_path" \
    --prover "$PROVER_URL" \
    --claimant-address "$CLAIMANT_ADDRESS" \
    --out "$FIXTURES_DIR/${out_prefix}.json"; then
    err "Proof generation failed for $label"
    FAILED=$((FAILED + 1))
    return
  fi

  # Verify the new proof on-chain
  info "Verifying new proof on-chain..."
  local seal_hex journal_hex image_id_hex journal_digest_hex
  seal_hex=$(tr -d '[:space:]' < "$FIXTURES_DIR/${out_prefix}.seal")
  journal_hex=$(tr -d '[:space:]' < "$FIXTURES_DIR/${out_prefix}.journal_raw")
  image_id_hex=$(tr -d '[:space:]' < "$FIXTURES_DIR/${out_prefix}.image_id")
  journal_digest_hex=$(sha256_of_hex "$journal_hex")

  local result
  result=$(stellar contract invoke \
    --send=no -q \
    --id "$RISC0_ROUTER" \
    --source "$CALLER_NAME" \
    --network "$NETWORK" \
    -- \
    verify \
    --image_id "$image_id_hex" \
    --journal "$journal_digest_hex" \
    --seal "$seal_hex" \
    2>&1) && exit_code=0 || exit_code=$?

  if [[ $exit_code -eq 0 ]]; then
    PASSED=$((PASSED + 1))
    ok "$label: generated + verified on-chain"
  else
    FAILED=$((FAILED + 1))
    err "$label: generated but on-chain verification failed"
    echo "  Output: $result"
  fi
}

# ---------------------------------------------------------------------------
# Ensure zero-score tapes are rejected by the prover API
# ---------------------------------------------------------------------------
assert_reject_zero_score_tape() {
  local tape_file="$FIXTURES_DIR/test-short.tape"
  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$tape_file" ]]; then
    err "Short tape not found: $tape_file"
    FAILED=$((FAILED + 1))
    return
  fi

  info "Checking zero-score rejection for short tape..."
  local resp http_code body error_code
  local query="receipt_kind=groth16&verify_mode=policy&claimant_address=${CLAIMANT_ADDRESS}"
  resp=$(curl -sS -X POST \
    "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
    -H "content-type: application/octet-stream" \
    --data-binary "@${tape_file}" \
    -w $'\n%{http_code}')
  http_code=$(echo "$resp" | tail -1)
  body=$(echo "$resp" | sed '$d')
  error_code=$(echo "$body" | python3 -c "import sys, json; print((json.load(sys.stdin).get('error_code') or '').strip())" 2>/dev/null || true)

  if [[ "$http_code" == "400" && "$error_code" == "zero_score_not_allowed" ]]; then
    PASSED=$((PASSED + 1))
    ok "short tape rejected with zero_score_not_allowed"
  else
    FAILED=$((FAILED + 1))
    err "short tape rejection check failed (HTTP $http_code, error_code=${error_code:-<none>})"
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "================================================"
echo "Regenerate Groth16 Proof Fixtures (Non-Zero Scores)"
echo "Prover: $PROVER_URL"
echo "$(date)"
echo "================================================"
echo ""

# Check prover health first
info "Checking prover health..."
health=$(curl -sf "$PROVER_URL/health" 2>&1) || {
  err "Prover not reachable at $PROVER_URL/health"
  exit 1
}
echo "  $health"
echo ""

ensure_funded_key "$CALLER_NAME"
if [[ -z "$CLAIMANT_ADDRESS" ]]; then
  CLAIMANT_ADDRESS=$(stellar keys address "$CALLER_NAME")
fi
if [[ ! "$CLAIMANT_ADDRESS" =~ ^[GC][A-Z2-7]{55}$ ]]; then
  err "CLAIMANT_ADDRESS must be a valid 56-char G... or C... strkey"
  exit 1
fi
info "Using claimant address: $CLAIMANT_ADDRESS"
echo ""

assert_reject_zero_score_tape
echo ""
regenerate_fixture "medium tape"    "test-medium.tape"    "proof-medium-groth16"
echo ""
regenerate_fixture "real game tape" "test-real-game.tape" "proof-real-game-groth16"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "================================================"
if [[ "$FAILED" -eq 0 ]]; then
  echo -e "\033[1;32mALL $TOTAL CHECKS PASSED\033[0m — $(date)"
else
  echo -e "\033[1;31m$FAILED/$TOTAL CHECKS FAILED\033[0m — $(date)"
fi
echo "================================================"

exit "$FAILED"
