#!/usr/bin/env bash
# regenerate-proofs.sh
#
# Batch-regenerate all 3 Groth16 proof fixtures from their source tapes.
# The prover is single-flight, so tapes are submitted sequentially.
# After each proof is generated, it's verified on-chain via the router.
#
# Usage:
#   ./scripts/regenerate-proofs.sh <prover-url>
#   ./scripts/regenerate-proofs.sh https://risc0-kalien.stellar.buzz
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
if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <prover-url>"
  echo "  e.g. $0 https://risc0-kalien.stellar.buzz"
  exit 1
fi

PROVER_URL="$1"

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
# Main
# ---------------------------------------------------------------------------
echo "================================================"
echo "Regenerate All Groth16 Proof Fixtures"
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
echo ""

regenerate_fixture "short tape"     "test-short.tape"     "proof-short-groth16"
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
  echo -e "\033[1;32mALL $TOTAL PROOFS REGENERATED + VERIFIED\033[0m — $(date)"
else
  echo -e "\033[1;31m$FAILED/$TOTAL PROOFS FAILED\033[0m — $(date)"
fi
echo "================================================"

exit "$FAILED"
