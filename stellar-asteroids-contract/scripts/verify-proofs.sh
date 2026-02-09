#!/usr/bin/env bash
# verify-proofs.sh
#
# Quick on-chain verification of existing Groth16 proof fixtures.
# No deployment, no prover needed — just reads fixture files and calls
# the RISC Zero router's `verify` on testnet (read-only, no tx).
#
# Usage:
#   ./scripts/verify-proofs.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/_helpers.sh"

require_cmds stellar xxd

# Need a funded key to sign the simulate-only invocation
CALLER_NAME="ast-verify-caller"

PASSED=0
FAILED=0
TOTAL=0

# ---------------------------------------------------------------------------
# Verify a single fixture
# ---------------------------------------------------------------------------
verify_fixture() {
  local label="$1" fixture_prefix="$2"

  info "Verifying: $label"

  local seal_file="$FIXTURES_DIR/${fixture_prefix}.seal"
  local journal_file="$FIXTURES_DIR/${fixture_prefix}.journal_raw"
  local image_id_file="$FIXTURES_DIR/${fixture_prefix}.image_id"

  # Check files exist
  for f in "$seal_file" "$journal_file" "$image_id_file"; do
    if [[ ! -f "$f" ]]; then
      err "Missing fixture: $f"
      TOTAL=$((TOTAL + 1))
      FAILED=$((FAILED + 1))
      return
    fi
  done

  local seal_hex journal_hex image_id_hex journal_digest_hex
  seal_hex=$(tr -d '[:space:]' < "$seal_file")
  journal_hex=$(tr -d '[:space:]' < "$journal_file")
  image_id_hex=$(tr -d '[:space:]' < "$image_id_file")
  journal_digest_hex=$(sha256_of_hex "$journal_hex")

  echo "  Seal: ${#seal_hex} hex chars ($(( ${#seal_hex} / 2 )) bytes)"
  echo "  Journal: ${#journal_hex} hex chars"
  echo "  Image ID: $image_id_hex"
  echo "  Journal digest: $journal_digest_hex"

  # Call router verify (read-only, --send=no)
  TOTAL=$((TOTAL + 1))
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
    ok "$label: verified on-chain"
  else
    FAILED=$((FAILED + 1))
    err "$label: verification failed"
    echo "  Output: $result"
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "================================================"
echo "Groth16 Proof Fixture Verification"
echo "$(date)"
echo "================================================"
echo ""

ensure_funded_key "$CALLER_NAME"
echo ""

verify_fixture "medium tape"    "proof-medium-groth16"
echo ""
verify_fixture "real game tape" "proof-real-game-groth16"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "================================================"
if [[ "$FAILED" -eq 0 ]]; then
  echo -e "\033[1;32mALL $TOTAL PROOFS VERIFIED\033[0m — $(date)"
else
  echo -e "\033[1;31m$FAILED/$TOTAL PROOFS FAILED\033[0m — $(date)"
fi
echo "================================================"

exit "$FAILED"
