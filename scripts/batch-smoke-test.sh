#!/usr/bin/env bash
set -euo pipefail

# Batch smoke test: run composite + groth16 on multiple tape files.
# Produces a CSV results file and a summary table.
#
# Usage:
#   bash scripts/batch-smoke-test.sh [options] <tape-paths-or-dirs...>
#
# Options:
#   --url <prover-url>     Prover URL (default: http://127.0.0.1:8080)
#   --receipts <csv>       composite,groth16 (default: composite,groth16)
#   --poll <seconds>       Poll interval (default: 5)
#   --out <dir>            Output directory (default: auto-timestamped in batch-results/)
#   -h, --help             Show this help
#
# Examples:
#   bash scripts/batch-smoke-test.sh test-fixtures/
#   bash scripts/batch-smoke-test.sh --receipts composite test-fixtures/test-short.tape test-fixtures/test-medium.tape
#   bash scripts/batch-smoke-test.sh --url http://localhost:8080 --out results/ test-fixtures/
#   bash scripts/batch-smoke-test.sh --url https://<vast-host>:<port> --out results/ test-fixtures/

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

PROVER_URL="http://127.0.0.1:8080"
RECEIPTS_CSV="composite,groth16"
POLL_INTERVAL=5
OUT_DIR=""
TAPE_ARGS=()
declare -a RECEIPTS=()

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/batch-smoke-test.sh [options] <tape-paths-or-dirs...>

Options:
  --url <prover-url>     Prover URL (default: http://127.0.0.1:8080)
  --receipts <csv>       Comma-separated list of receipt kinds to run.
                         Allowed values: composite|groth16
                         Default: composite,groth16
  --poll <seconds>       Poll interval (default: 5)
  --out <dir>            Output directory (default: auto-timestamped in batch-results/)
  -h, --help             Show this help
USAGE_EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --url) PROVER_URL="${2%/}"; shift 2 ;;
    --receipts) RECEIPTS_CSV="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"; shift 2 ;;
    --poll) POLL_INTERVAL="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --*) echo "Unknown option: $1" >&2; exit 1 ;;
    *) TAPE_ARGS+=("$1"); shift ;;
  esac
done

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
  case "$receipt" in
    composite|groth16)
      NORMALIZED_RECEIPTS+=("$receipt")
      ;;
    *)
      echo "ERROR: unsupported receipt kind in --receipts: $receipt (allowed: composite|groth16)" >&2
      exit 1
      ;;
  esac
done
RECEIPTS=("${NORMALIZED_RECEIPTS[@]}")
if [[ "${#RECEIPTS[@]}" -eq 0 ]]; then
  echo "ERROR: --receipts must include at least one valid receipt kind" >&2
  exit 1
fi

if [[ ${#TAPE_ARGS[@]} -eq 0 ]]; then
  usage >&2
  exit 1
fi

# shellcheck source=_prover-helpers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_prover-helpers.sh"

# ── Discover tape files ──────────────────────────────────────────────
TAPE_FILES=()
for arg in "${TAPE_ARGS[@]}"; do
  if [[ -d "$arg" ]]; then
    while IFS= read -r f; do
      TAPE_FILES+=("$f")
    done < <(find "$arg" -name '*.tape' -type f | sort)
  elif [[ -f "$arg" ]]; then
    TAPE_FILES+=("$arg")
  else
    echo "WARNING: skipping '$arg' (not a file or directory)" >&2
  fi
done

if [[ ${#TAPE_FILES[@]} -eq 0 ]]; then
  echo "ERROR: no .tape files found" >&2
  exit 1
fi

# ── Parse headers into a temp file, sort by frame count ──────────────
# Each line: frames<TAB>score<TAB>seed<TAB>size<TAB>path
TAPE_INDEX_FILE=$(mktemp)
trap 'rm -f "$TAPE_INDEX_FILE"' EXIT

for tape in "${TAPE_FILES[@]}"; do
  info=$(read_tape_header "$tape" 2>/dev/null) || { echo "WARNING: skipping $tape (invalid header)" >&2; continue; }
  frames=$(echo "$info" | awk '{print $1}')
  score=$(echo "$info"  | awk '{print $2}')
  seed=$(echo "$info"   | awk '{print $3}')
  size=$(echo "$info"   | awk '{print $4}')
  printf '%s\t%s\t%s\t%s\t%s\n' "$frames" "$score" "$seed" "$size" "$tape" >> "$TAPE_INDEX_FILE"
done

TAPE_COUNT=$(wc -l < "$TAPE_INDEX_FILE" | tr -d ' ')
if [[ "$TAPE_COUNT" -eq 0 ]]; then
  echo "ERROR: no valid .tape files found" >&2
  exit 1
fi

# Sort by frame count (first field, numeric)
sort -t$'\t' -k1 -n "$TAPE_INDEX_FILE" -o "$TAPE_INDEX_FILE"

# ── Output directory ─────────────────────────────────────────────────
if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$ROOT_DIR/batch-results/$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUT_DIR"

CSV_FILE="$OUT_DIR/results.csv"
LOG_FILE="$OUT_DIR/batch.log"
RESULT_FILE="$OUT_DIR/.results.tmp"

# Write CSV header
echo "tape,frames,score,seed,size_bytes,stage,status,job_id,prover_ms,wall_s,segments,total_cycles,error,error_code" > "$CSV_FILE"
: > "$RESULT_FILE"

# ── CSV writer ───────────────────────────────────────────────────────
write_csv_row() {
  python3 -c "
import csv, sys
w = csv.writer(sys.stdout)
w.writerow(sys.argv[1:])
" "$@" >> "$CSV_FILE"
}

# ── Banner ───────────────────────────────────────────────────────────
{
echo "================================================================"
echo "Batch Smoke Test"
echo "$(date)"
echo "================================================================"
echo ""
echo "Prover:     $PROVER_URL"
echo "Tapes:      $TAPE_COUNT"
echo "Receipts:   ${RECEIPTS[*]}"
echo "Poll:       ${POLL_INTERVAL}s"
echo "Output:     $OUT_DIR"
echo ""
} | tee "$LOG_FILE"

# ── Health check ─────────────────────────────────────────────────────
health=$(curl -sf --connect-timeout 10 "$PROVER_URL/health" 2>/dev/null) || {
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}

accel=$(echo "$health" | json_field accelerator)
image=$(echo "$health" | json_field image_id | cut -c1-16)
max_frames=$(echo "$health" | json_field max_frames)

{
echo "Accel:      $accel"
echo "ImageID:    ${image}..."
echo "Max frames: $max_frames"
echo ""
} | tee -a "$LOG_FILE"

# ── Run a single proof stage ─────────────────────────────────────────
# Args: $1=tape_path $2=receipt_kind $3=frames $4=score $5=seed $6=size
# Sets: LAST_STATUS
run_stage() {
  local tape="$1" receipt="$2" frames="$3" score="$4" seed="$5" size="$6"
  local tape_name
  tape_name=$(basename "$tape")

  LAST_STATUS=""
  local last_job_id="" last_prover_ms="" last_wall_s=""
  local last_segments="" last_total_cycles="" last_error="" last_error_code=""

  # Check frame limit
  if [[ -n "$max_frames" && "$max_frames" != "0" && "$max_frames" != "" && "$frames" -gt "$max_frames" ]]; then
    echo "  SKIP: $frames frames exceeds max_frames=$max_frames"
    LAST_STATUS="skip"
    last_error="exceeds max_frames ($max_frames)"
    write_csv_row "$tape_name" "$frames" "$score" "$seed" "$size" "$receipt" "skip" "" "" "" "" "" "$last_error" ""
    return 0
  fi

  wait_for_idle

  # Submit
  local query
  query=$(with_claimant_query "receipt_kind=${receipt}&verify_mode=policy")
  local resp_raw http_code body
  resp_raw=$(http_status_and_body -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
    --data-binary "@${tape}" -H "content-type: application/octet-stream")
  http_code=$(echo "$resp_raw" | tail -1)
  body=$(echo "$resp_raw" | sed '$d')

  if [[ "$http_code" != "202" ]]; then
    LAST_STATUS="fail"
    last_error=$(echo "$body" | json_field error)
    last_error_code=$(echo "$body" | json_field error_code)
    echo "  SUBMIT FAILED (HTTP $http_code)"
    [[ -n "$last_error_code" ]] && echo "  error_code: $last_error_code"
    [[ -n "$last_error" ]]      && echo "  error: $last_error"
    write_csv_row "$tape_name" "$frames" "$score" "$seed" "$size" "$receipt" "submit_failed" "" "" "" "" "" "$last_error" "$last_error_code"
    return 1
  fi

  last_job_id=$(echo "$body" | json_field job_id)
  if [[ -z "$last_job_id" ]]; then
    LAST_STATUS="fail"
    last_error="no job_id in response"
    echo "  REJECTED: $body"
    write_csv_row "$tape_name" "$frames" "$score" "$seed" "$size" "$receipt" "rejected" "" "" "" "" "" "$last_error" ""
    return 1
  fi
  echo "  job: $last_job_id"

  local wall_start
  wall_start=$(date +%s)

  # Poll
  while true; do
    sleep "$POLL_INTERVAL"
    local jr status
    jr=$(curl -sf "${PROVER_URL}/api/jobs/${last_job_id}" 2>/dev/null) || { echo "  (poll error)"; continue; }
    status=$(echo "$jr" | json_field status)

    if [[ "$status" == "succeeded" ]]; then
      LAST_STATUS="pass"
      last_wall_s=$(( $(date +%s) - wall_start ))
      last_prover_ms=$(echo "$jr" | json_field_nested result.elapsed_ms)
      last_segments=$(echo "$jr" | json_field_nested result.proof.stats.segments)
      last_total_cycles=$(echo "$jr" | json_field_nested result.proof.stats.total_cycles)

      local prover_s
      prover_s=$(python3 -c "print(f'{${last_prover_ms:-0}/1000:.1f}')")
      echo "  PASS (prover: ${prover_s}s, wall: ${last_wall_s}s, segments: ${last_segments}, cycles: ${last_total_cycles})"

      write_csv_row "$tape_name" "$frames" "$score" "$seed" "$size" "$receipt" "succeeded" \
        "$last_job_id" "$last_prover_ms" "$last_wall_s" "$last_segments" "$last_total_cycles" "" ""

      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${last_job_id}" > /dev/null 2>&1 || true
      return 0

    elif [[ "$status" == "failed" ]]; then
      LAST_STATUS="fail"
      last_wall_s=$(( $(date +%s) - wall_start ))
      last_error=$(echo "$jr" | json_field error)
      last_error_code=$(echo "$jr" | json_field error_code)

      if [[ -n "$last_error_code" ]]; then
        echo "  FAILED [$last_error_code]: $last_error"
      else
        echo "  FAILED: $last_error"
      fi

      write_csv_row "$tape_name" "$frames" "$score" "$seed" "$size" "$receipt" "failed" \
        "$last_job_id" "" "$last_wall_s" "" "" "$last_error" "$last_error_code"

      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${last_job_id}" > /dev/null 2>&1 || true
      return 1
    fi
  done
}

# ── Main loop ────────────────────────────────────────────────────────
total_start=$(date +%s)
total_pass=0
total_fail=0
total_skip=0
tape_index=0

while IFS=$'\t' read -r frames score seed size tape; do
  tape_name=$(basename "$tape")
  tape_index=$((tape_index + 1))

  {
  echo "────────────────────────────────────────────────────────────────"
  echo "[$tape_index/$TAPE_COUNT] $tape_name  (${frames} frames)"
  echo "────────────────────────────────────────────────────────────────"
  } | tee -a "$LOG_FILE"

  for receipt in "${RECEIPTS[@]}"; do
    echo "  ${receipt}:" | tee -a "$LOG_FILE"
    stage_rc=0
    run_stage "$tape" "$receipt" "$frames" "$score" "$seed" "$size" > >(tee -a "$LOG_FILE") 2>&1 || stage_rc=$?
    if [[ $stage_rc -eq 0 ]]; then
      if [[ "$LAST_STATUS" == "skip" ]]; then
        total_skip=$((total_skip + 1))
        printf '%s\t%s\tskip\n' "$tape_name" "$receipt" >> "$RESULT_FILE"
      else
        total_pass=$((total_pass + 1))
        printf '%s\t%s\tpass\n' "$tape_name" "$receipt" >> "$RESULT_FILE"
      fi
    else
      total_fail=$((total_fail + 1))
      printf '%s\t%s\tfail\n' "$tape_name" "$receipt" >> "$RESULT_FILE"
    fi
  done

  echo "" | tee -a "$LOG_FILE"
done < "$TAPE_INDEX_FILE"

total_end=$(date +%s)
total_secs=$((total_end - total_start))

# ── Summary table ────────────────────────────────────────────────────
{
echo "================================================================"
echo "BATCH SUMMARY"
echo "$(date)"
echo "================================================================"
echo ""
printf "%-55s %-12s %-12s\n" "TAPE" "COMPOSITE" "GROTH16"
printf "%-55s %-12s %-12s\n" "────" "─────────" "───────"

while IFS=$'\t' read -r frames score seed size tape; do
  tape_name=$(basename "$tape")
  comp_result="—"
  g16_result="—"
  while IFS=$'\t' read -r rname rstage rstatus; do
    if [[ "$rname" == "$tape_name" && "$rstage" == "composite" ]]; then
      comp_result="$rstatus"
    fi
    if [[ "$rname" == "$tape_name" && "$rstage" == "groth16" ]]; then
      g16_result="$rstatus"
    fi
  done < "$RESULT_FILE"
  printf "%-55s %-12s %-12s\n" "$tape_name" "$comp_result" "$g16_result"
done < "$TAPE_INDEX_FILE"

echo ""
echo "Total:   $((total_pass + total_fail + total_skip)) stages"
echo "Passed:  $total_pass"
echo "Failed:  $total_fail"
echo "Skipped: $total_skip"
echo "Time:    ${total_secs}s"
echo ""
echo "CSV:     $CSV_FILE"
echo "Log:     $LOG_FILE"
echo "================================================================"
} | tee -a "$LOG_FILE"

rm -f "$RESULT_FILE"

if [[ $total_fail -gt 0 ]]; then
  exit 1
fi
