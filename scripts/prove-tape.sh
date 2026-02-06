#!/usr/bin/env bash
set -euo pipefail

# Submit a single tape to the remote RISC0 prover API and wait for the result.
# Useful for quick one-off proof generation and debugging.
#
# Usage:
#   bash scripts/prove-tape.sh <prover-url> <tape-file> [options]
#
# Options (passed as query params):
#   --seg <n>          segment_limit_po2 (default: 21)
#   --receipt <kind>   composite|succinct|groth16 (default: composite)
#   --no-verify        Skip receipt verification on the prover
#   --poll <seconds>   Poll interval (default: 5)
#   --keep             Don't delete the job after completion
#
# Examples:
#   bash scripts/prove-tape.sh http://host:8080 test-fixtures/test-short.tape
#   bash scripts/prove-tape.sh http://host:8080 test-fixtures/test-medium.tape --seg 20
#   bash scripts/prove-tape.sh http://host:8080 my-game.tape --receipt succinct --keep

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <prover-url> <tape-file> [--seg N] [--receipt KIND] [--no-verify] [--poll N] [--keep]" >&2
  exit 1
fi

PROVER_URL="${1%/}"
TAPE_FILE="$2"
shift 2

SEG=21
RECEIPT="composite"
VERIFY="true"
POLL=5
KEEP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seg) SEG="$2"; shift 2 ;;
    --receipt) RECEIPT="$2"; shift 2 ;;
    --no-verify) VERIFY="false"; shift ;;
    --poll) POLL="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if [[ ! -f "$TAPE_FILE" ]]; then
  echo "ERROR: tape file not found: $TAPE_FILE" >&2
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
score = j.get("score", "n/a")
frames = j.get("total_frames", "n/a")
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
print(f"  total_frames:    {frames}")
PYEOF

tape_size=$(wc -c < "$TAPE_FILE" | tr -d ' ')
echo "Tape:    $(basename "$TAPE_FILE") (${tape_size} bytes)"
echo "Prover:  $PROVER_URL"
echo "Params:  seg=$SEG  receipt=$RECEIPT  verify=$VERIFY"
echo ""

# Check prover health.
health=$(curl -sf --connect-timeout 10 "$PROVER_URL/health" 2>/dev/null) || {
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}
running=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['running_jobs'])")
if [[ "$running" != "0" ]]; then
  echo "Waiting for prover to finish current job..."
  while true; do
    sleep "$POLL"
    health=$(curl -sf --connect-timeout 5 "$PROVER_URL/health" 2>/dev/null) || continue
    running=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['running_jobs'])")
    queued=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['queued_jobs'])")
    [[ "$running" == "0" && "$queued" == "0" ]] && break
    echo "  still busy (running=$running, queued=$queued)..."
  done
fi

# Submit.
query="segment_limit_po2=${SEG}&receipt_kind=${RECEIPT}&verify_receipt=${VERIFY}"
resp=$(curl -sf -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
  --data-binary "@${TAPE_FILE}" -H "content-type: application/octet-stream") || {
  echo "ERROR: failed to submit tape" >&2
  exit 1
}

job_id=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('job_id',''))")
if [[ -z "$job_id" ]]; then
  echo "ERROR: submission rejected: $resp" >&2
  exit 1
fi
echo "Job:     $job_id"
echo "Proving..."

# Poll.
while true; do
  sleep "$POLL"
  jr=$(curl -sf "${PROVER_URL}/api/jobs/${job_id}" 2>/dev/null) || { echo "  (poll error, retrying)"; continue; }
  status=$(echo "$jr" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")

  if [[ "$status" == "succeeded" ]]; then
    echo ""
    echo "$jr" | python3 "$FMT_SCRIPT"
    if [[ "$KEEP" -eq 0 ]]; then
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
    else
      echo ""
      echo "Job retained: GET ${PROVER_URL}/api/jobs/${job_id}"
    fi
    exit 0
  elif [[ "$status" == "failed" ]]; then
    echo ""
    err=$(echo "$jr" | python3 -c "import sys,json; print(json.load(sys.stdin).get('error','unknown'))")
    echo "FAILED: $err" >&2
    if [[ "$KEEP" -eq 0 ]]; then
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
    fi
    exit 1
  fi
done
