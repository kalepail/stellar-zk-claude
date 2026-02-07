#!/usr/bin/env bash
set -uo pipefail

# ============================================================================
# Stress Test: RISC0 Prover API – SQLite Persistence & Error Handling
# ============================================================================
#
# Exercises the full job lifecycle with emphasis on:
#   1. SQLite persistence – can completed jobs be retrieved 5-10 min later?
#   2. Error handling – are error states stored and reported correctly?
#   3. Edge cases – invalid tapes, concurrent submissions, delete semantics
#   4. Job accumulation – multiple failed/succeeded jobs coexist correctly
#   5. Single-flight enforcement – 429 when prover is busy
#
# Usage:
#   bash scripts/stress-test-api.sh <prover-url> [options]
#
# Options:
#   --delay <minutes>   How long to wait before delayed retrieval (default: 5)
#   --tape <path>       Tape file for real proving (default: test-fixtures/test-medium.tape)
#   --short-tape <path> Short tape for fast error tests (default: test-fixtures/test-short.tape)
#
# Examples:
#   bash scripts/stress-test-api.sh https://risc0-kalien.stellar.buzz
#   bash scripts/stress-test-api.sh https://risc0-kalien.stellar.buzz --delay 10
# ============================================================================

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAPE_FILE="$ROOT_DIR/test-fixtures/test-medium.tape"
SHORT_TAPE="$ROOT_DIR/test-fixtures/test-short.tape"
LONG_TAPE="$ROOT_DIR/test-fixtures/test-real-game.tape"
POLL_INTERVAL=5
DELAY_MINUTES=5

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <prover-url> [--delay <minutes>] [--tape <path>] [--short-tape <path>]" >&2
  exit 1
fi

PROVER_URL="${1%/}"
shift

while [[ $# -gt 0 ]]; do
  case "$1" in
    --delay) DELAY_MINUTES="$2"; shift 2 ;;
    --tape) TAPE_FILE="$2"; shift 2 ;;
    --short-tape) SHORT_TAPE="$2"; shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if [[ ! -f "$TAPE_FILE" ]]; then
  echo "ERROR: tape file not found: $TAPE_FILE" >&2
  exit 1
fi
if [[ ! -f "$SHORT_TAPE" ]]; then
  echo "ERROR: short tape file not found: $SHORT_TAPE" >&2
  exit 1
fi

# ── Helpers ──────────────────────────────────────────────────────────────────

PASS=0
FAIL=0
WARN=0
TESTS_RUN=0
JOBS_TO_CLEAN=()

pass() { PASS=$((PASS + 1)); TESTS_RUN=$((TESTS_RUN + 1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); TESTS_RUN=$((TESTS_RUN + 1)); echo "  FAIL: $1"; }
warn() { WARN=$((WARN + 1)); echo "  WARN: $1"; }
info() { echo "  INFO: $1"; }

json_field() {
  python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('$1',''))" 2>/dev/null
}

json_field_nested() {
  python3 -c "
import sys, json
d = json.load(sys.stdin)
keys = '$1'.split('.')
for k in keys:
    if isinstance(d, dict):
        d = d.get(k, '')
    else:
        d = ''
        break
print(d)
" 2>/dev/null
}

http_status_and_body() {
  # Returns "BODY\nHTTP_STATUS" — caller splits them.
  curl -s -w '\n%{http_code}' "$@"
}

wait_for_idle() {
  while true; do
    local h
    h=$(curl -sf --connect-timeout 5 "$PROVER_URL/health" 2>/dev/null) || { sleep "$POLL_INTERVAL"; continue; }
    local r q
    r=$(echo "$h" | json_field running_jobs)
    q=$(echo "$h" | json_field queued_jobs)
    [[ "$r" == "0" && "$q" == "0" ]] && return 0
    info "waiting for idle (running=$r, queued=$q)..."
    sleep "$POLL_INTERVAL"
  done
}

# Submit garbage and wait for it to fail. Sets LAST_GARBAGE_ID.
submit_garbage_and_wait() {
  local label="$1"
  local gfile
  gfile=$(mktemp)
  dd if=/dev/urandom of="$gfile" bs=128 count=1 2>/dev/null

  local resp_raw http_code body
  resp_raw=$(http_status_and_body -X POST "$PROVER_URL/api/jobs/prove-tape/raw?segment_limit_po2=20" \
    --data-binary "@$gfile" -H "content-type: application/octet-stream")
  http_code=$(echo "$resp_raw" | tail -1)
  body=$(echo "$resp_raw" | sed '$d')
  rm -f "$gfile"

  if [[ "$http_code" != "202" ]]; then
    info "$label: rejected with $http_code (expected 202)"
    LAST_GARBAGE_ID=""
    return 1
  fi

  LAST_GARBAGE_ID=$(echo "$body" | json_field job_id)
  JOBS_TO_CLEAN+=("$LAST_GARBAGE_ID")

  # Wait for failure
  local start elapsed
  start=$(date +%s)
  while true; do
    sleep 2
    elapsed=$(( $(date +%s) - start ))
    local jr status
    jr=$(curl -sf "$PROVER_URL/api/jobs/$LAST_GARBAGE_ID" 2>/dev/null) || continue
    status=$(echo "$jr" | json_field status)
    if [[ "$status" == "failed" ]]; then
      info "$label: failed in ${elapsed}s (job=$LAST_GARBAGE_ID)"
      return 0
    elif [[ "$status" == "succeeded" ]]; then
      info "$label: unexpectedly succeeded"
      return 1
    fi
    if [[ $elapsed -gt 120 ]]; then
      info "$label: stuck in $status for ${elapsed}s"
      return 1
    fi
  done
}

cleanup_jobs() {
  for jid in "${JOBS_TO_CLEAN[@]}"; do
    curl -sf -X DELETE "$PROVER_URL/api/jobs/$jid" > /dev/null 2>&1 || true
  done
}

# ── Phase 0: Banner ─────────────────────────────────────────────────────────

echo "================================================================"
echo "RISC0 Prover API — Stress Test"
echo "$(date)"
echo "================================================================"
echo ""
echo "Target:      $PROVER_URL"
echo "Tape:        $(basename "$TAPE_FILE") ($(wc -c < "$TAPE_FILE" | tr -d ' ') bytes)"
echo "Short tape:  $(basename "$SHORT_TAPE") ($(wc -c < "$SHORT_TAPE" | tr -d ' ') bytes)"
if [[ -f "$LONG_TAPE" ]]; then
echo "Long tape:   $(basename "$LONG_TAPE") ($(wc -c < "$LONG_TAPE" | tr -d ' ') bytes)"
fi
echo "Delay test:  ${DELAY_MINUTES} minutes"
echo ""

# ── Phase 1: Health Check ───────────────────────────────────────────────────

echo "── Phase 1: Health Check ──────────────────────────────────────"

health_resp=$(curl -sf --connect-timeout 10 "$PROVER_URL/health" 2>/dev/null) || {
  echo "FATAL: prover unreachable at $PROVER_URL" >&2
  exit 1
}

echo "$health_resp" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(f'  Service:     {d[\"service\"]}')
print(f'  Status:      {d[\"status\"]}')
print(f'  Accelerator: {d[\"accelerator\"]}')
print(f'  Dev mode:    {d[\"dev_mode\"]}')
print(f'  Segment po2: [{d[\"min_segment_limit_po2\"]}..={d[\"max_segment_limit_po2\"]}]')
print(f'  Max frames:  {d[\"max_frames\"]}')
print(f'  Max jobs:    {d[\"max_jobs\"]}')
print(f'  Stored jobs: {d[\"stored_jobs\"]}')
print(f'  Auth req:    {d[\"auth_required\"]}')
"

INITIAL_STORED=$(echo "$health_resp" | json_field stored_jobs)

status=$(echo "$health_resp" | json_field status)
if [[ "$status" == "healthy" ]]; then
  pass "health endpoint returns healthy"
else
  fail "health endpoint returned: $status"
fi

# Wait for idle before starting.
running=$(echo "$health_resp" | json_field running_jobs)
queued=$(echo "$health_resp" | json_field queued_jobs)
if [[ "$running" == "0" && "$queued" == "0" ]]; then
  pass "prover is idle (no active jobs)"
else
  warn "prover has active jobs — waiting..."
  wait_for_idle
  pass "prover is now idle"
fi

echo ""

# ── Phase 2: Error Handling (fast, no proving) ──────────────────────────────

echo "── Phase 2: Error Handling Tests ──────────────────────────────"

# Test 2a: Lookup non-existent job → 404
echo ""
echo "[2a] GET non-existent job ID"
fake_id="00000000-0000-0000-0000-000000000000"
resp_raw=$(http_status_and_body "$PROVER_URL/api/jobs/$fake_id")
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "404" ]]; then
  pass "non-existent job returns 404"
else
  fail "non-existent job returned HTTP $http_code (expected 404)"
fi
info "response: $body"

# Test 2b: Submit empty body → 400
echo ""
echo "[2b] POST empty body"
resp_raw=$(http_status_and_body -X POST "$PROVER_URL/api/jobs/prove-tape/raw" \
  -H "content-type: application/octet-stream" -d '')
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "400" ]]; then
  pass "empty body returns 400"
else
  fail "empty body returned HTTP $http_code (expected 400), body: $body"
fi
info "error: $(echo "$body" | json_field error)"

# Test 2c: Submit garbage bytes → should fail during proving
echo ""
echo "[2c] POST garbage tape (128 random bytes)"
submit_garbage_and_wait "garbage-1"
GARBAGE_JOB_1="$LAST_GARBAGE_ID"

if [[ -n "$GARBAGE_JOB_1" ]]; then
  gjr=$(curl -sf "$PROVER_URL/api/jobs/$GARBAGE_JOB_1" 2>/dev/null)
  garbage_error=$(echo "$gjr" | json_field error)
  pass "garbage tape job failed as expected"
  info "error stored: \"$garbage_error\""

  # Verify error is non-empty and descriptive
  if [[ -n "$garbage_error" && "$garbage_error" != "None" && "$garbage_error" != "" ]]; then
    pass "error message is descriptive and stored in DB"
  else
    fail "error message is empty or missing for failed job"
  fi

  # Verify timestamps
  started=$(echo "$gjr" | json_field started_at_unix_s)
  finished=$(echo "$gjr" | json_field finished_at_unix_s)
  if [[ -n "$started" && "$started" != "None" && "$started" != "null" ]]; then
    pass "started_at_unix_s is set on failed job"
  else
    warn "started_at_unix_s is missing on failed job"
  fi
  if [[ -n "$finished" && "$finished" != "None" && "$finished" != "null" ]]; then
    pass "finished_at_unix_s is set on failed job"
  else
    fail "finished_at_unix_s is missing on failed job"
  fi
fi

# Test 2d: Invalid job ID format → 404 or 400
echo ""
echo "[2d] GET invalid job ID format"
resp_raw=$(http_status_and_body "$PROVER_URL/api/jobs/not-a-uuid")
http_code=$(echo "$resp_raw" | tail -1)

if [[ "$http_code" == "400" || "$http_code" == "404" ]]; then
  pass "invalid UUID returns $http_code"
else
  fail "invalid UUID returned HTTP $http_code (expected 400 or 404)"
fi

# Test 2e: DELETE non-existent job → 404
echo ""
echo "[2e] DELETE non-existent job"
resp_raw=$(http_status_and_body -X DELETE "$PROVER_URL/api/jobs/$fake_id")
http_code=$(echo "$resp_raw" | tail -1)

if [[ "$http_code" == "404" ]]; then
  pass "delete non-existent job returns 404"
else
  fail "delete non-existent job returned HTTP $http_code"
fi

# Test 2f: Out-of-range segment_limit_po2 → 400
echo ""
echo "[2f] POST with out-of-range segment_limit_po2"
resp_raw=$(http_status_and_body -X POST "$PROVER_URL/api/jobs/prove-tape/raw?segment_limit_po2=99" \
  --data-binary "@$SHORT_TAPE" -H "content-type: application/octet-stream")
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "400" ]]; then
  pass "out-of-range segment_limit_po2 returns 400"
  info "error: $(echo "$body" | json_field error)"
else
  fail "out-of-range segment_limit_po2 returned HTTP $http_code (expected 400)"
fi

echo ""

# ── Phase 3: Job Accumulation ───────────────────────────────────────────────

echo "── Phase 3: Job Accumulation (multiple failed jobs) ───────────"
echo ""
echo "Submitting 5 garbage tapes sequentially to accumulate failed jobs..."

ACCUMULATED_IDS=()
for i in $(seq 1 5); do
  wait_for_idle
  submit_garbage_and_wait "accumulate-$i"
  if [[ -n "$LAST_GARBAGE_ID" ]]; then
    ACCUMULATED_IDS+=("$LAST_GARBAGE_ID")
  fi
done

echo ""
# Check stored_jobs count increased
health_resp=$(curl -sf "$PROVER_URL/health" 2>/dev/null)
stored_now=$(echo "$health_resp" | json_field stored_jobs)
# We have: initial + garbage_1 + 5 accumulated = initial + 6
expected_min=$((INITIAL_STORED + 6))
info "stored_jobs now: $stored_now (started at $INITIAL_STORED, expect >= $expected_min)"

if [[ "$stored_now" -ge "$expected_min" ]]; then
  pass "stored_jobs reflects accumulated failed jobs ($stored_now >= $expected_min)"
else
  fail "stored_jobs too low: $stored_now (expected >= $expected_min)"
fi

# Verify all accumulated jobs are still individually retrievable
echo ""
echo "[3b] Verify all accumulated jobs are retrievable"
all_retrievable=true
for jid in "${ACCUMULATED_IDS[@]}"; do
  jr=$(curl -sf "$PROVER_URL/api/jobs/$jid" 2>/dev/null) || { all_retrievable=false; break; }
  s=$(echo "$jr" | json_field status)
  if [[ "$s" != "failed" ]]; then
    all_retrievable=false
    break
  fi
done

if [[ "$all_retrievable" == "true" ]]; then
  pass "all ${#ACCUMULATED_IDS[@]} accumulated failed jobs retrievable with correct status"
else
  fail "some accumulated jobs not retrievable or wrong status"
fi

echo ""

# ── Phase 4: Real Proof with Medium Tape ────────────────────────────────────

echo "── Phase 4: Submit Real Proof Job (medium tape) ───────────────"

wait_for_idle

echo ""
echo "[4a] Submit medium tape for proving"
tape_size=$(wc -c < "$TAPE_FILE" | tr -d ' ')
info "tape: $(basename "$TAPE_FILE") ($tape_size bytes)"

resp=$(curl -sf -X POST "$PROVER_URL/api/jobs/prove-tape/raw?segment_limit_po2=20&receipt_kind=composite" \
  --data-binary "@$TAPE_FILE" -H "content-type: application/octet-stream" 2>&1)

if [[ $? -ne 0 ]]; then
  fail "failed to submit tape"
  echo "FATAL: cannot continue without a proof job" >&2
  exit 1
fi

JOB_ID=$(echo "$resp" | json_field job_id)
sub_status=$(echo "$resp" | json_field status)
JOBS_TO_CLEAN+=("$JOB_ID")

if [[ -n "$JOB_ID" && "$sub_status" == "queued" ]]; then
  pass "tape submitted, job_id=$JOB_ID, status=queued"
else
  fail "unexpected submission response: $resp"
  exit 1
fi
info "status_url: $(echo "$resp" | json_field status_url)"

# Test 4b: Concurrent submission while busy → 429
# Wait a moment to ensure the job has been picked up, then try submitting.
echo ""
echo "[4b] Concurrent submission while prover is busy"
sleep 1

# Poll until we see 'running' or 'succeeded' to confirm prover is active
for attempt in $(seq 1 10); do
  jr_check=$(curl -sf "$PROVER_URL/api/jobs/$JOB_ID" 2>/dev/null) || continue
  cur_status=$(echo "$jr_check" | json_field status)
  if [[ "$cur_status" == "running" || "$cur_status" == "queued" ]]; then
    break
  fi
  sleep 1
done

# Now try to submit a second job while the prover is busy
resp_raw=$(http_status_and_body -X POST "$PROVER_URL/api/jobs/prove-tape/raw?segment_limit_po2=20" \
  --data-binary "@$SHORT_TAPE" -H "content-type: application/octet-stream")
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "429" ]]; then
  pass "concurrent submission rejected with 429 (single-flight enforced)"
  info "error: $(echo "$body" | json_field error)"
else
  # If the medium tape finished before we could test, note it as a warning
  if [[ "$cur_status" == "succeeded" ]]; then
    warn "medium tape proved too fast to test 429 (completed before concurrent submit)"
  else
    fail "concurrent submission returned HTTP $http_code (expected 429): $body"
  fi
  # Clean up if it was accepted
  extra_id=$(echo "$body" | json_field job_id)
  if [[ -n "$extra_id" && "$extra_id" != "$JOB_ID" ]]; then
    info "cleaning up extra job $extra_id"
    JOBS_TO_CLEAN+=("$extra_id")
  fi
fi

# Test 4c: Poll for status transitions
echo ""
echo "[4c] Polling for job completion"
saw_running=false
prove_start=$(date +%s)

while true; do
  sleep "$POLL_INTERVAL"
  elapsed=$(( $(date +%s) - prove_start ))
  jr=$(curl -sf "$PROVER_URL/api/jobs/$JOB_ID" 2>/dev/null) || { info "poll error (${elapsed}s)"; continue; }
  status=$(echo "$jr" | json_field status)

  if [[ "$status" == "running" && "$saw_running" == "false" ]]; then
    saw_running=true
    pass "observed status transition to 'running'"
  fi

  if [[ "$status" == "succeeded" ]]; then
    prove_elapsed=$(( $(date +%s) - prove_start ))
    pass "proof completed in ${prove_elapsed}s"

    # Validate result structure
    result_elapsed=$(echo "$jr" | json_field_nested result.elapsed_ms)
    score=$(echo "$jr" | json_field_nested result.proof.journal.final_score)
    frames=$(echo "$jr" | json_field_nested result.proof.journal.frame_count)
    seed=$(echo "$jr" | json_field_nested result.proof.journal.seed)
    segs=$(echo "$jr" | json_field_nested result.proof.stats.segments)
    total_cycles=$(echo "$jr" | json_field_nested result.proof.stats.total_cycles)
    receipt_kind=$(echo "$jr" | json_field_nested result.proof.produced_receipt_kind)
    finished=$(echo "$jr" | json_field finished_at_unix_s)
    started=$(echo "$jr" | json_field started_at_unix_s)

    info "final_score=$score, frame_count=$frames, seed=$seed"
    info "segments=$segs, total_cycles=$total_cycles, receipt=$receipt_kind"
    info "elapsed_ms=$result_elapsed, started=$started, finished=$finished"

    if [[ -n "$score" && "$score" != "" && "$score" != "None" ]]; then
      pass "result contains final_score in journal"
    else
      fail "result missing final_score"
    fi

    if [[ -n "$frames" && "$frames" != "" && "$frames" != "None" && "$frames" != "0" ]]; then
      pass "result contains frame_count in journal ($frames)"
    else
      fail "result missing or zero frame_count"
    fi

    if [[ -n "$segs" && "$segs" != "" && "$segs" != "None" ]]; then
      pass "result contains stats.segments ($segs)"
    else
      fail "result missing stats.segments"
    fi

    if [[ -n "$finished" && "$finished" != "None" && "$finished" != "null" ]]; then
      pass "finished_at_unix_s set on succeeded job"
    else
      fail "finished_at_unix_s missing on succeeded job"
    fi

    if [[ -n "$started" && "$started" != "None" && "$started" != "null" ]]; then
      pass "started_at_unix_s set on succeeded job"
    else
      fail "started_at_unix_s missing on succeeded job"
    fi

    if [[ "$saw_running" == "true" ]]; then
      pass "full lifecycle observed: queued → running → succeeded"
    else
      warn "never observed 'running' status (proof completed between polls)"
    fi

    break

  elif [[ "$status" == "failed" ]]; then
    error_msg=$(echo "$jr" | json_field error)
    fail "proof job failed: $error_msg"
    echo "FATAL: cannot continue stress test without a successful proof" >&2
    cleanup_jobs
    exit 1
  else
    info "$status (${elapsed}s)..."
    if [[ $elapsed -gt 900 ]]; then
      fail "proof job stuck for over 15 minutes"
      cleanup_jobs
      exit 1
    fi
  fi
done

echo ""

# ── Phase 5: Running Status Observation (long tape) ─────────────────────────

echo "── Phase 5: Running Status Observation (long tape) ────────────"

if [[ -f "$LONG_TAPE" ]]; then
  wait_for_idle

  echo ""
  echo "[5a] Submit long tape to observe 'running' status"
  long_tape_size=$(wc -c < "$LONG_TAPE" | tr -d ' ')
  info "tape: $(basename "$LONG_TAPE") ($long_tape_size bytes)"

  resp=$(curl -sf -X POST "$PROVER_URL/api/jobs/prove-tape/raw?segment_limit_po2=20&receipt_kind=composite" \
    --data-binary "@$LONG_TAPE" -H "content-type: application/octet-stream" 2>&1)

  if [[ $? -ne 0 ]]; then
    fail "failed to submit long tape"
  else
    LONG_JOB_ID=$(echo "$resp" | json_field job_id)
    JOBS_TO_CLEAN+=("$LONG_JOB_ID")

    if [[ -n "$LONG_JOB_ID" ]]; then
      pass "long tape submitted, job_id=$LONG_JOB_ID"

      # Poll aggressively at 1s intervals to catch 'running'
      echo ""
      echo "[5b] Polling at 1s intervals for 'running' status"
      saw_queued=false
      saw_running=false
      long_start=$(date +%s)

      while true; do
        sleep 1
        elapsed=$(( $(date +%s) - long_start ))
        jr=$(curl -sf "$PROVER_URL/api/jobs/$LONG_JOB_ID" 2>/dev/null) || { info "poll error (${elapsed}s)"; continue; }
        status=$(echo "$jr" | json_field status)

        if [[ "$status" == "queued" && "$saw_queued" == "false" ]]; then
          saw_queued=true
          info "observed 'queued' at ${elapsed}s"
        fi

        if [[ "$status" == "running" && "$saw_running" == "false" ]]; then
          saw_running=true
          pass "observed 'running' status at ${elapsed}s"
        fi

        if [[ "$status" == "succeeded" ]]; then
          long_elapsed=$(( $(date +%s) - long_start ))
          pass "long tape proof completed in ${long_elapsed}s"

          long_score=$(echo "$jr" | json_field_nested result.proof.journal.final_score)
          long_frames=$(echo "$jr" | json_field_nested result.proof.journal.frame_count)
          long_segs=$(echo "$jr" | json_field_nested result.proof.stats.segments)
          info "final_score=$long_score, frame_count=$long_frames, segments=$long_segs"

          if [[ "$saw_running" == "true" ]]; then
            pass "full lifecycle observed: queued → running → succeeded"
          else
            fail "never observed 'running' status despite ${long_elapsed}s proving time"
          fi
          break

        elif [[ "$status" == "failed" ]]; then
          error_msg=$(echo "$jr" | json_field error)
          fail "long tape proof failed: $error_msg"
          break
        else
          # Print status every 5s to avoid spam
          if [[ $((elapsed % 5)) -eq 0 ]]; then
            info "$status (${elapsed}s)..."
          fi
          if [[ $elapsed -gt 900 ]]; then
            fail "long tape proof stuck for over 15 minutes"
            break
          fi
        fi
      done
    else
      fail "no job_id in submission response"
    fi
  fi
else
  info "long tape not found at $LONG_TAPE — skipping running-status test"
  warn "skipped running-status observation (no long tape)"
fi

echo ""

# ── Phase 6: Immediate Retrieval ────────────────────────────────────────────

echo "── Phase 6: Immediate Retrieval ───────────────────────────────"

echo ""
echo "[6a] GET completed job immediately after proving"
jr=$(curl -sf "$PROVER_URL/api/jobs/$JOB_ID" 2>/dev/null)
if [[ $? -eq 0 ]]; then
  status=$(echo "$jr" | json_field status)
  if [[ "$status" == "succeeded" ]]; then
    pass "completed job retrievable immediately"
  else
    fail "completed job has wrong status: $status"
  fi
else
  fail "failed to retrieve completed job"
fi

# Verify the full result envelope is still present
score_check=$(echo "$jr" | json_field_nested result.proof.journal.final_score)
if [[ -n "$score_check" && "$score_check" != "" && "$score_check" != "None" ]]; then
  pass "full result envelope (with proof data) loaded from disk"
else
  fail "result envelope missing or incomplete on retrieval"
fi

# Verify the earliest garbage job is STILL retrievable
echo ""
echo "[6b] GET first garbage job (cross-job persistence)"
if [[ -n "$GARBAGE_JOB_1" ]]; then
  gjr=$(curl -sf "$PROVER_URL/api/jobs/$GARBAGE_JOB_1" 2>/dev/null)
  if [[ $? -eq 0 ]]; then
    gstatus=$(echo "$gjr" | json_field status)
    if [[ "$gstatus" == "failed" ]]; then
      pass "earliest failed job still retrievable after proving another job"
    else
      fail "earliest failed job status changed to $gstatus"
    fi
  else
    fail "earliest failed job not retrievable"
  fi
fi

echo ""

# ── Phase 7: Post-Prove Health ──────────────────────────────────────────────

echo "── Phase 7: Post-Prove Health ─────────────────────────────────"

health_resp=$(curl -sf "$PROVER_URL/health" 2>/dev/null)
stored=$(echo "$health_resp" | json_field stored_jobs)
running=$(echo "$health_resp" | json_field running_jobs)
queued=$(echo "$health_resp" | json_field queued_jobs)

info "stored_jobs=$stored, running=$running, queued=$queued"

if [[ "$running" == "0" && "$queued" == "0" ]]; then
  pass "prover returned to idle after job completed"
else
  fail "prover not idle: running=$running, queued=$queued"
fi

# stored should be initial + garbage_1 + 5 accumulated + medium proof (+ long proof if run) = initial + 7 or 8
expected_stored=$((INITIAL_STORED + 7))
if [[ "$stored" -ge "$expected_stored" ]]; then
  pass "stored_jobs count accurate ($stored >= $expected_stored)"
else
  fail "stored_jobs is $stored (expected >= $expected_stored)"
fi

echo ""

# ── Phase 8: Delayed Retrieval ──────────────────────────────────────────────

echo "── Phase 8: Delayed Retrieval (${DELAY_MINUTES} minute wait) ──────────────"

delay_secs=$((DELAY_MINUTES * 60))
echo ""
info "Waiting ${DELAY_MINUTES} minutes before re-fetching completed job..."
info "Job ID: $JOB_ID"
info "Started waiting at: $(date)"
info "Will resume at: $(date -v+${DELAY_MINUTES}M 2>/dev/null || date -d "+${DELAY_MINUTES} minutes" 2>/dev/null || echo "~${DELAY_MINUTES} min from now")"

# Count down with periodic health checks
elapsed_wait=0
while [[ $elapsed_wait -lt $delay_secs ]]; do
  sleep_chunk=30
  remaining=$((delay_secs - elapsed_wait))
  if [[ $sleep_chunk -gt $remaining ]]; then
    sleep_chunk=$remaining
  fi
  sleep "$sleep_chunk"
  elapsed_wait=$((elapsed_wait + sleep_chunk))
  remaining=$((delay_secs - elapsed_wait))

  # Periodic health check every 60s
  if [[ $((elapsed_wait % 60)) -eq 0 || $remaining -eq 0 ]]; then
    h=$(curl -sf --connect-timeout 5 "$PROVER_URL/health" 2>/dev/null)
    if [[ $? -eq 0 ]]; then
      stored_now=$(echo "$h" | json_field stored_jobs)
      info "${remaining}s remaining — server healthy, stored_jobs=$stored_now"
    else
      warn "${remaining}s remaining — health check failed (server may be restarting?)"
    fi
  fi
done

echo ""
echo "[8a] GET succeeded job after ${DELAY_MINUTES} minute delay"
resp_raw=$(http_status_and_body "$PROVER_URL/api/jobs/$JOB_ID")
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "200" ]]; then
  status=$(echo "$body" | json_field status)
  if [[ "$status" == "succeeded" ]]; then
    pass "CRITICAL: succeeded job retrievable after ${DELAY_MINUTES} minute delay"
  else
    fail "CRITICAL: job status changed to '$status' after delay (expected 'succeeded')"
  fi
else
  fail "CRITICAL: succeeded job returned HTTP $http_code after ${DELAY_MINUTES} min (expected 200)"
  info "body: $body"
fi

# Verify the result envelope is still fully intact
score_delayed=$(echo "$body" | json_field_nested result.proof.journal.final_score)
if [[ -n "$score_delayed" && "$score_delayed" != "" && "$score_delayed" != "None" ]]; then
  pass "result envelope fully intact after delay (final_score=$score_delayed)"
  if [[ "$score_delayed" == "$score_check" ]]; then
    pass "final_score matches immediate retrieval ($score_check)"
  else
    fail "final_score mismatch: immediate=$score_check, delayed=$score_delayed"
  fi
else
  fail "result envelope missing or corrupted after delay"
fi

# Check timestamps haven't changed
finished_delayed=$(echo "$body" | json_field finished_at_unix_s)
if [[ "$finished_delayed" == "$finished" ]]; then
  pass "finished_at_unix_s unchanged after delay"
else
  warn "finished_at_unix_s changed: was=$finished, now=$finished_delayed"
fi

# Re-check ALL accumulated failed jobs
echo ""
echo "[8b] GET all accumulated failed jobs after ${DELAY_MINUTES} minute delay"
failed_still_ok=0
failed_missing=0
for jid in "${ACCUMULATED_IDS[@]}"; do
  jr_d=$(curl -sf "$PROVER_URL/api/jobs/$jid" 2>/dev/null)
  if [[ $? -eq 0 ]]; then
    s=$(echo "$jr_d" | json_field status)
    e=$(echo "$jr_d" | json_field error)
    if [[ "$s" == "failed" && -n "$e" && "$e" != "" && "$e" != "None" ]]; then
      failed_still_ok=$((failed_still_ok + 1))
    else
      failed_missing=$((failed_missing + 1))
    fi
  else
    failed_missing=$((failed_missing + 1))
  fi
done

info "$failed_still_ok/${#ACCUMULATED_IDS[@]} failed jobs still intact with error messages"

if [[ $failed_missing -eq 0 ]]; then
  pass "all ${#ACCUMULATED_IDS[@]} failed jobs persist with errors after ${DELAY_MINUTES} min"
else
  fail "$failed_missing/${#ACCUMULATED_IDS[@]} failed jobs missing or corrupted after delay"
fi

# Re-check the very first garbage job
if [[ -n "$GARBAGE_JOB_1" ]]; then
  echo ""
  echo "[8c] GET first garbage job after ${DELAY_MINUTES} minute delay"
  gjr_delayed=$(curl -sf "$PROVER_URL/api/jobs/$GARBAGE_JOB_1" 2>/dev/null)
  if [[ $? -eq 0 ]]; then
    gstatus=$(echo "$gjr_delayed" | json_field status)
    gerror=$(echo "$gjr_delayed" | json_field error)
    if [[ "$gstatus" == "failed" ]]; then
      pass "first garbage job still retrievable after delay"
      pass "error message preserved: \"$(echo "$gerror" | head -c 100)...\""
    else
      fail "first garbage job status changed to $gstatus after delay"
    fi
  else
    fail "first garbage job not retrievable after delay"
  fi
fi

echo ""

# ── Phase 9: Delete & Verify ───────────────────────────────────────────────

echo "── Phase 9: Delete & Verify ───────────────────────────────────"

echo ""
echo "[9a] DELETE succeeded job"
resp_raw=$(http_status_and_body -X DELETE "$PROVER_URL/api/jobs/$JOB_ID")
http_code=$(echo "$resp_raw" | tail -1)
body=$(echo "$resp_raw" | sed '$d')

if [[ "$http_code" == "200" ]]; then
  success=$(echo "$body" | json_field success)
  if [[ "$success" == "True" || "$success" == "true" ]]; then
    pass "succeeded job deleted"
  else
    fail "delete returned 200 but success=$success"
  fi
else
  fail "delete returned HTTP $http_code"
fi

echo ""
echo "[9b] GET deleted job (should be 404)"
resp_raw=$(http_status_and_body "$PROVER_URL/api/jobs/$JOB_ID")
http_code=$(echo "$resp_raw" | tail -1)

if [[ "$http_code" == "404" ]]; then
  pass "deleted job returns 404"
else
  fail "deleted job returned HTTP $http_code (expected 404)"
fi

echo ""
echo "[9c] DELETE already-deleted job (should be 404)"
resp_raw=$(http_status_and_body -X DELETE "$PROVER_URL/api/jobs/$JOB_ID")
http_code=$(echo "$resp_raw" | tail -1)

if [[ "$http_code" == "404" ]]; then
  pass "re-delete returns 404 (idempotent)"
else
  fail "re-delete returned HTTP $http_code (expected 404)"
fi

echo ""

# ── Phase 10: Cleanup ──────────────────────────────────────────────────────

echo "── Phase 10: Cleanup ────────────────────────────────────────"
echo ""
echo "Deleting all test jobs..."

deleted_count=0
for jid in "${JOBS_TO_CLEAN[@]}"; do
  resp_raw=$(http_status_and_body -X DELETE "$PROVER_URL/api/jobs/$jid")
  http_code=$(echo "$resp_raw" | tail -1)
  if [[ "$http_code" == "200" ]]; then
    deleted_count=$((deleted_count + 1))
  fi
done
info "deleted $deleted_count jobs"

health_resp=$(curl -sf "$PROVER_URL/health" 2>/dev/null)
stored_after=$(echo "$health_resp" | json_field stored_jobs)
info "stored_jobs after cleanup: $stored_after (was $INITIAL_STORED at start)"

echo ""

# ── Phase 11: Summary ──────────────────────────────────────────────────────

echo "================================================================"
echo "STRESS TEST SUMMARY"
echo "$(date)"
echo "================================================================"
echo ""
echo "  Tests run:  $TESTS_RUN"
echo "  Passed:     $PASS"
echo "  Failed:     $FAIL"
echo "  Warnings:   $WARN"
echo ""

if [[ $FAIL -eq 0 ]]; then
  echo "  Result: ALL TESTS PASSED"
else
  echo "  Result: $FAIL FAILURE(S)"
fi

echo ""
echo "================================================================"

exit $FAIL
