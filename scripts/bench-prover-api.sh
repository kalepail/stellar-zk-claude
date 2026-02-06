#!/usr/bin/env bash
set -euo pipefail

# Benchmark the remote RISC0 prover API across different parameter configs.
# Submits jobs sequentially (single-flight), waits for each to complete,
# and prints timing + cycle stats.
#
# Usage:
#   bash scripts/bench-prover-api.sh <prover-url>
#   bash scripts/bench-prover-api.sh http://145.236.164.238:47063
#
# The test matrix is defined in TESTS below. Edit it to add/remove configs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAPE_DIR="$ROOT_DIR/test-fixtures"
POLL_INTERVAL=5

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <prover-url>" >&2
  echo "  e.g. $0 http://145.236.164.238:47063" >&2
  exit 1
fi

PROVER_URL="${1%/}"

# Test configs: "label|tape_file|segment_limit_po2|receipt_kind|verify_receipt"
# Tape files are relative to test-fixtures/.
TESTS=(
  "short-seg19|test-short.tape|19|composite|true"
  "short-seg20|test-short.tape|20|composite|true"
  "short-seg21|test-short.tape|21|composite|true"
  "medium-seg19|test-medium.tape|19|composite|true"
  "medium-seg20|test-medium.tape|20|composite|true"
  "medium-seg21|test-medium.tape|21|composite|true"
)

# Add the large tape if it exists.
LARGE_TAPE="$(ls "$TAPE_DIR"/test-real-game.tape 2>/dev/null | head -n 1)"
if [[ -n "$LARGE_TAPE" ]]; then
  large_basename="$(basename "$LARGE_TAPE")"
  TESTS+=(
    "large-seg19|${large_basename}|19|composite|true"
    "large-seg20|${large_basename}|20|composite|true"
    "large-seg21|${large_basename}|21|composite|true"
  )
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
print(f"  {e/1000:.1f}s ({e} ms)")
print(f"  segments:        {segs}")
print(f"  total_cycles:    {tc:,}")
print(f"  user_cycles:     {uc:,}")
print(f"  paging_cycles:   {pc:,}")
print(f"  reserved_cycles: {rc:,}")
print(f"  receipt:         {rk} -> {pk}")
print(f"  score:           {score}")
print(f"  total_frames:    {frames}")
PYEOF

wait_for_prover_free() {
  while true; do
    local health
    health=$(curl -sf --connect-timeout 5 "$PROVER_URL/health" 2>/dev/null) || { sleep "$POLL_INTERVAL"; continue; }
    local running queued
    running=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['running_jobs'])")
    queued=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['queued_jobs'])")
    if [[ "$running" == "0" && "$queued" == "0" ]]; then
      return 0
    fi
    echo "    waiting (running=$running, queued=$queued)..."
    sleep "$POLL_INTERVAL"
  done
}

run_test() {
  local label="$1" tape_file="$2" seg_po2="$3" receipt="$4" verify="$5"
  local tape_path="${TAPE_DIR}/${tape_file}"

  if [[ ! -f "$tape_path" ]]; then
    echo "  SKIP: tape not found: $tape_path"
    return 1
  fi

  local tape_size
  tape_size=$(wc -c < "$tape_path" | tr -d ' ')

  echo ""
  echo "--- $label ---"
  echo "  tape=$tape_file (${tape_size} bytes)  seg=$seg_po2  receipt=$receipt  verify=$verify"

  wait_for_prover_free

  local query="segment_limit_po2=${seg_po2}&receipt_kind=${receipt}&verify_receipt=${verify}"
  local resp
  resp=$(curl -sf -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
    --data-binary "@${tape_path}" -H "content-type: application/octet-stream" 2>&1) || {
    echo "  SUBMIT FAILED (curl error)"
    return 1
  }

  local job_id
  job_id=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('job_id',''))")
  if [[ -z "$job_id" ]]; then
    echo "  SUBMIT REJECTED: $resp"
    return 1
  fi
  echo "  job=$job_id"

  while true; do
    sleep "$POLL_INTERVAL"
    local jr
    jr=$(curl -sf "${PROVER_URL}/api/jobs/${job_id}" 2>/dev/null) || { echo "    poll error"; continue; }
    local status
    status=$(echo "$jr" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")

    if [[ "$status" == "succeeded" ]]; then
      echo "$jr" | python3 "$FMT_SCRIPT"
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
      return 0
    elif [[ "$status" == "failed" ]]; then
      local err
      err=$(echo "$jr" | python3 -c "import sys,json; print(json.load(sys.stdin).get('error','unknown')[:200])")
      echo "  FAILED: $err"
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" > /dev/null 2>&1 || true
      return 1
    else
      echo "    $status..."
    fi
  done
}

echo "================================================"
echo "RISC0 Prover API Benchmark"
echo "$(date)"
echo "================================================"

health=$(curl -sf --connect-timeout 10 "$PROVER_URL/health") || {
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}
echo "$health" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(f'Prover:      $PROVER_URL')
print(f'Accelerator: {d.get(\"accelerator\", \"unknown\")}')
print(f'Segment po2: [{d[\"min_segment_limit_po2\"]}..={d[\"max_segment_limit_po2\"]}]')
print(f'Max frames:  {d[\"max_frames\"]}')
"

passed=0
failed=0
for tc in "${TESTS[@]}"; do
  IFS='|' read -r label tape seg receipt verify <<< "$tc"
  if run_test "$label" "$tape" "$seg" "$receipt" "$verify"; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    echo "  (continuing after failure)"
  fi
done

echo ""
echo "================================================"
echo "DONE: $passed passed, $failed failed - $(date)"
echo "================================================"
