#!/usr/bin/env bash
set -euo pipefail

# Submit a single tape to the remote RISC0 prover API and wait for the result.
# Useful for quick one-off proof generation and debugging.
#
# Usage:
#   bash scripts/prove-tape.sh [prover-url] <tape-file> [options]
#
# Options (passed as query params):
#   --seg <n>          segment_limit_po2 (default: 21)
#   --receipt <kind>   composite|succinct|groth16 (default: groth16)
#   --poll <seconds>   Poll interval (default: 5)
#   --cleanup-mode <m> delete|keep (default: delete)
#   -h, --help         Show this help
#
# Examples:
#   bash scripts/prove-tape.sh test-fixtures/test-short.tape
#   bash scripts/prove-tape.sh http://host:8080 test-fixtures/test-short.tape
#   bash scripts/prove-tape.sh http://host:8080 test-fixtures/test-medium.tape --seg 20
#   bash scripts/prove-tape.sh http://host:8080 my-game.tape --receipt succinct --cleanup-mode keep

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/prove-tape.sh [prover-url] <tape-file> [options]

Options:
  --seg <n>          segment_limit_po2 (default: 21)
  --receipt <kind>   composite|succinct|groth16 (default: groth16)
  --poll <seconds>   Poll interval (default: 5)
  --cleanup-mode <m> delete|keep (default: delete)
  -h, --help         Show this help

Examples:
  bash scripts/prove-tape.sh test-fixtures/test-short.tape
  bash scripts/prove-tape.sh http://127.0.0.1:8080 test-fixtures/test-medium.tape --seg 20
  bash scripts/prove-tape.sh https://<vast-host>:<port> test-fixtures/test-medium.tape --receipt groth16
USAGE_EOF
}

if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
  usage
  exit 0
fi

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 1
fi

DEFAULT_PROVER_URL="http://127.0.0.1:8080"
PROVER_URL="$DEFAULT_PROVER_URL"
if [[ $# -ge 2 && "${1:-}" != --* && -f "${2:-}" ]]; then
  PROVER_URL="${1%/}"
  shift
fi

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 1
fi

TAPE_FILE="$1"
shift

SEG=21
RECEIPT="groth16"
POLL_INTERVAL=5
CLEANUP_MODE="delete"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seg) SEG="$2"; shift 2 ;;
    --receipt) RECEIPT="$2"; shift 2 ;;
    --poll) POLL_INTERVAL="$2"; shift 2 ;;
    --cleanup-mode) CLEANUP_MODE="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# shellcheck source=_prover-helpers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_prover-helpers.sh"

if [[ ! -f "$TAPE_FILE" ]]; then
  echo "ERROR: tape file not found: $TAPE_FILE" >&2
  exit 1
fi

if ! [[ "$SEG" =~ ^[0-9]+$ ]] || [[ "$SEG" -lt 1 ]]; then
  echo "ERROR: --seg must be an integer >= 1" >&2
  exit 1
fi

if ! [[ "$POLL_INTERVAL" =~ ^[0-9]+$ ]] || [[ "$POLL_INTERVAL" -lt 1 ]]; then
  echo "ERROR: --poll must be an integer >= 1" >&2
  exit 1
fi

case "$RECEIPT" in
  composite|succinct|groth16) ;;
  *)
    echo "ERROR: --receipt must be one of: composite, succinct, groth16" >&2
    exit 1
    ;;
esac

if [[ "$CLEANUP_MODE" != "delete" && "$CLEANUP_MODE" != "keep" ]]; then
  echo "ERROR: --cleanup-mode must be delete or keep" >&2
  exit 1
fi

FMT_SCRIPT="$(mktemp)"
trap 'rm -f "$FMT_SCRIPT"' EXIT
cat > "$FMT_SCRIPT" << 'PYEOF'
import sys, json
d = json.load(sys.stdin)
r = d.get("result", {})
p = r.get("proof", {})
s = p.get("stats", {})
j = p.get("journal", {})
e = r.get("elapsed_ms", 0)
segs = s.get("segments", "?")
tc = s.get("total_cycles", 0)
uc = s.get("user_cycles", 0)
pc = s.get("paging_cycles", 0)
rc = s.get("reserved_cycles", 0)
score = j.get("final_score", "n/a")
frames = j.get("frame_count", "n/a")
rk = p.get("requested_receipt_kind", "?")
pk = p.get("produced_receipt_kind", "?")
print(f"Result:  SUCCEEDED")
print(f"Elapsed: {e/1000:.1f}s ({e} ms)")
print(f"")
print(f"Stats:")
print(f"  segments:        {segs}")
print(f"  total_cycles:    {tc:,}")
print(f"  user_cycles:     {uc:,}")
print(f"  paging_cycles:   {pc:,}")
print(f"  reserved_cycles: {rc:,}")
print(f"")
print(f"Proof:")
print(f"  receipt:         {rk} -> {pk}")
print(f"  score:           {score}")
print(f"  frame_count:     {frames}")
PYEOF

tape_size=$(wc -c < "$TAPE_FILE" | tr -d ' ')
echo "Tape:    $(basename "$TAPE_FILE") (${tape_size} bytes)"
echo "Prover:  $PROVER_URL"
echo "Params:  seg=$SEG  receipt=$RECEIPT  verify_mode=policy  cleanup_mode=$CLEANUP_MODE"
echo ""

# Check prover health.
health=$(curl -sf --connect-timeout 10 "$PROVER_URL/health" 2>/dev/null) || {
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}
running=$(echo "$health" | json_field running_jobs)
if [[ "$running" != "0" ]]; then
  echo "Waiting for prover to finish current job..."
  wait_for_idle
fi

# Submit.
query="segment_limit_po2=${SEG}&receipt_kind=${RECEIPT}&verify_mode=policy"
resp_raw=$(http_status_and_body -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
  --data-binary "@${TAPE_FILE}" -H "content-type: application/octet-stream") || {
  echo "ERROR: failed to connect to prover at $PROVER_URL" >&2
  exit 1
}
http_code=$(echo "$resp_raw" | tail -1)
resp=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" != "202" ]]; then
  err=$(echo "$resp" | json_field error)
  err_code=$(echo "$resp" | json_field error_code)
  echo "ERROR: submit failed (HTTP $http_code)" >&2
  [[ -n "$err_code" ]] && echo "  error_code: $err_code" >&2
  [[ -n "$err" ]]      && echo "  error: $err" >&2
  exit 1
fi

job_id=$(echo "$resp" | json_field job_id)
if [[ -z "$job_id" ]]; then
  echo "ERROR: submission rejected: $resp" >&2
  exit 1
fi
echo "Job:     $job_id"
echo "Proving..."

# Poll.
while true; do
  sleep "$POLL_INTERVAL"
  jr=$(curl -sf "${PROVER_URL}/api/jobs/${job_id}" 2>/dev/null) || { echo "  (poll error, retrying)"; continue; }
  status=$(echo "$jr" | json_field status)

  if [[ "$status" == "succeeded" ]]; then
    echo ""
    echo "$jr" | python3 "$FMT_SCRIPT"
    if [[ "$CLEANUP_MODE" == "delete" ]]; then
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
    else
      echo ""
      echo "Job retained: GET ${PROVER_URL}/api/jobs/${job_id}"
    fi
    exit 0
  elif [[ "$status" == "failed" ]]; then
    echo ""
    err=$(echo "$jr" | json_field error)
    err_code=$(echo "$jr" | json_field error_code)
    if [[ -n "$err_code" ]]; then
      echo "FAILED [$err_code]: $err" >&2
    else
      echo "FAILED: $err" >&2
    fi
    if [[ "$CLEANUP_MODE" == "delete" ]]; then
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
    fi
    exit 1
  fi
done
