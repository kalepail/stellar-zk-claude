#!/usr/bin/env bash
set -euo pipefail

# Quick smoke test: prove a tape with selected receipt kinds.
# Reports frame count, timing, and basic proof stats for each stage.
#
# Usage:
#   bash scripts/smoke-test-prover.sh [options]
#
# Examples:
#   bash scripts/smoke-test-prover.sh
#   bash scripts/smoke-test-prover.sh --tape test-fixtures/test-short.tape
#   bash scripts/smoke-test-prover.sh --url https://<vast-host>:<port> --receipts composite,groth16

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/smoke-test-prover.sh [options]

Defaults:
  --url       http://127.0.0.1:8080
  --tape      test-fixtures/test-real-game.tape
  --receipts  composite
  --poll      5

Examples:
  bash scripts/smoke-test-prover.sh
  bash scripts/smoke-test-prover.sh --tape test-fixtures/test-short.tape
  bash scripts/smoke-test-prover.sh --url https://<vast-host>:<port> --receipts composite,groth16
USAGE_EOF
}

PROVER_URL="http://127.0.0.1:8080"
TAPE_FILE="$ROOT_DIR/test-fixtures/test-real-game.tape"
RECEIPTS_CSV="composite"
POLL_INTERVAL=5
declare -a RECEIPTS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url)
      PROVER_URL="${2%/}"
      shift 2
      ;;
    --tape)
      TAPE_FILE="$2"
      shift 2
      ;;
    --receipts)
      RECEIPTS_CSV="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --poll)
      POLL_INTERVAL="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ! "$POLL_INTERVAL" =~ ^[0-9]+$ || "$POLL_INTERVAL" -lt 1 ]]; then
  echo "ERROR: --poll must be an integer >= 1" >&2
  exit 1
fi

if [[ -z "$RECEIPTS_CSV" ]]; then
  echo "ERROR: --receipts cannot be empty" >&2
  exit 1
fi
IFS=',' read -r -a RECEIPTS <<< "$RECEIPTS_CSV"
if [[ "${#RECEIPTS[@]}" -eq 0 ]]; then
  echo "ERROR: --receipts must include at least one value" >&2
  exit 1
fi
declare -a NORMALIZED_RECEIPTS=()
for receipt in "${RECEIPTS[@]}"; do
  receipt="$(echo "$receipt" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  if [[ -z "$receipt" ]]; then
    echo "ERROR: --receipts contains an empty entry" >&2
    exit 1
  fi
  case "$receipt" in
    composite|succinct|groth16)
      NORMALIZED_RECEIPTS+=("$receipt")
      ;;
    *)
      echo "ERROR: unsupported receipt kind: $receipt (allowed: composite|succinct|groth16)" >&2
      exit 1
      ;;
  esac
done
RECEIPTS=("${NORMALIZED_RECEIPTS[@]}")

# shellcheck source=_prover-helpers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_prover-helpers.sh"

if [[ ! -f "$TAPE_FILE" ]]; then
  echo "ERROR: tape file not found: $TAPE_FILE" >&2
  exit 1
fi

# ── Parse tape header ────────────────────────────────────────────────
tape_info=$(read_tape_header "$TAPE_FILE")
FRAMES=$(echo "$tape_info" | awk '{print $1}')
SCORE=$(echo "$tape_info"  | awk '{print $2}')
SEED=$(echo "$tape_info"   | awk '{print $3}')
SIZE=$(echo "$tape_info"   | awk '{print $4}')

echo "================================================"
echo "Prover Smoke Test"
echo "$(date)"
echo "================================================"
echo ""
echo "Tape:    $(basename "$TAPE_FILE") (${SIZE} bytes)"
echo "Frames:  $FRAMES"
echo "Score:   $SCORE"
echo "Seed:    $SEED"
echo "Prover:  $PROVER_URL"
echo "Receipts:${RECEIPTS[*]}"

# ── Health check ─────────────────────────────────────────────────────
health=$(curl -sf --connect-timeout 10 "$PROVER_URL/health" 2>/dev/null) || {
  echo ""
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}

accel=$(echo "$health" | json_field accelerator)
image=$(echo "$health" | json_field image_id | cut -c1-16)
echo "Accel:   $accel"
echo "ImageID: ${image}..."
echo ""

# ── Proof runner ─────────────────────────────────────────────────────

# Submit a proving job and poll until finished. Prints stats.
# Args: $1=receipt_kind  $2=label
run_proof() {
  local receipt="$1" label="$2"
  echo "--- $label ($receipt) ---"

  wait_for_idle

  local query
  query="receipt_kind=${receipt}&verify_mode=policy"
  local resp_raw http_code body
  resp_raw=$(http_status_and_body -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
    --data-binary "@${TAPE_FILE}" -H "content-type: application/octet-stream")
  http_code=$(echo "$resp_raw" | tail -1)
  body=$(echo "$resp_raw" | sed '$d')

  if [[ "$http_code" != "202" ]]; then
    local err err_code
    err=$(echo "$body" | json_field error)
    err_code=$(echo "$body" | json_field error_code)
    echo "  SUBMIT FAILED (HTTP $http_code)" >&2
    [[ -n "$err_code" ]] && echo "  error_code: $err_code" >&2
    [[ -n "$err" ]]      && echo "  error: $err" >&2
    return 1
  fi

  local job_id
  job_id=$(echo "$body" | json_field job_id)
  if [[ -z "$job_id" ]]; then
    echo "  REJECTED: $body" >&2
    return 1
  fi
  echo "  job: $job_id"

  local wall_start
  wall_start=$(date +%s)

  while true; do
    sleep "$POLL_INTERVAL"
    local jr status
    jr=$(curl -sf "${PROVER_URL}/api/jobs/${job_id}" 2>/dev/null) || { echo "  (poll error)"; continue; }
    status=$(echo "$jr" | json_field status)

    if [[ "$status" == "succeeded" ]]; then
      local wall_end wall_secs
      wall_end=$(date +%s)
      wall_secs=$((wall_end - wall_start))

      echo "$jr" | python3 -c "
import sys, json
d = json.load(sys.stdin)
r = d.get('result', {})
p = r.get('proof', {})
s = p.get('stats', {})
e = r.get('elapsed_ms', 0)
segs = s.get('segments', '?')
tc = s.get('total_cycles', 0)
rk = p.get('requested_receipt_kind', '?')
pk = p.get('produced_receipt_kind', '?')
print(f'  prover time:  {e/1000:.1f}s')
print(f'  wall time:    ${wall_secs}s')
print(f'  receipt:      {rk} -> {pk}')
print(f'  segments:     {segs}')
print(f'  total_cycles: {tc:,}')
"
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
      echo ""
      return 0

    elif [[ "$status" == "failed" ]]; then
      local err err_code
      err=$(echo "$jr" | json_field error)
      err_code=$(echo "$jr" | json_field error_code)
      if [[ -n "$err_code" ]]; then
        echo "  FAILED [$err_code]: $err" >&2
      else
        echo "  FAILED: $err" >&2
      fi
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
      return 1
    fi
  done
}

# ── Run selected stages ──────────────────────────────────────────────
total_start=$(date +%s)
failures=0

stage_index=1
for receipt in "${RECEIPTS[@]}"; do
  run_proof "$receipt" "Stage ${stage_index}" || failures=$((failures + 1))
  stage_index=$((stage_index + 1))
done

total_end=$(date +%s)
total_secs=$((total_end - total_start))

echo "================================================"
if [[ $failures -eq 0 ]]; then
  echo "PASS - ${#RECEIPTS[@]} stage(s) succeeded (${total_secs}s total)"
else
  echo "FAIL - $failures stage(s) failed (${total_secs}s total)"
fi
echo "================================================"
exit "$failures"
