#!/usr/bin/env bash
set -euo pipefail

# Benchmark a remote RISC Zero prover across a segment_limit_po2 sweep with
# configurable floors/ceilings and receipt kinds.
#
# Usage:
#   bash scripts/bench-segment-sweep.sh <prover-url> [options]
#
# Example:
#   bash scripts/bench-segment-sweep.sh https://your-prover.example.com \
#     --seg-floor 19 --seg-ceiling 22 \
#     --receipt composite --receipt succinct \
#     --repeat 2 --include-real

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHORT_TAPE="$ROOT_DIR/test-fixtures/test-short.tape"
MEDIUM_TAPE="$ROOT_DIR/test-fixtures/test-medium.tape"
REAL_TAPE="$ROOT_DIR/test-fixtures/test-real-game.tape"

POLL_INTERVAL=5
SEG_FLOOR=19
SEG_CEILING=22
VERIFY_RECEIPT="false"
REPEAT=1
INCLUDE_REAL=0
MAX_FRAMES=""
STRICT_BOUNDS=0
CSV_OUT=""
RECEIPT_FLAGS_SET=0
declare -a RECEIPTS=("composite" "succinct")

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/bench-segment-sweep.sh <prover-url> [options]

Options:
  --seg-floor <n>            Requested segment_limit_po2 floor (default: 19)
  --seg-ceiling <n>          Requested segment_limit_po2 ceiling (default: 22)
  --strict-bounds            Fail if requested floor/ceiling exceed prover policy.
                             Default behavior is to clamp to prover health bounds.
  --receipt <kind>           Receipt kind to benchmark (repeatable).
                             Values: composite|succinct|groth16
                             Default: composite + succinct
  --verify-receipt <bool>    true|false (default: false)
  --repeat <n>               Repetitions per config (default: 1)
  --poll <seconds>           Poll interval while waiting for completion (default: 5)
  --max-frames <n>           Optional max_frames query parameter override
  --include-real             Include test-real-game.tape in matrix when present
  --short-tape <path>        Override short tape path
  --medium-tape <path>       Override medium tape path
  --real-tape <path>         Override real tape path
  --csv-out <path>           Output CSV path
  -h, --help                 Show this help

Notes:
  - Runs sequentially (single-flight friendly).
  - Waits for prover idle state before each submission.
  - Records both wall-clock and prover elapsed_ms.
USAGE_EOF
}

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

if [[ "$1" == "-h" || "$1" == "--help" ]]; then
  usage
  exit 0
fi

PROVER_URL="${1%/}"
shift

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seg-floor)
      SEG_FLOOR="${2:-}"
      shift 2
      ;;
    --seg-ceiling)
      SEG_CEILING="${2:-}"
      shift 2
      ;;
    --strict-bounds)
      STRICT_BOUNDS=1
      shift
      ;;
    --receipt)
      if [[ "$RECEIPT_FLAGS_SET" -eq 0 ]]; then
        RECEIPTS=()
        RECEIPT_FLAGS_SET=1
      fi
      RECEIPTS+=("${2:-}")
      shift 2
      ;;
    --verify-receipt)
      VERIFY_RECEIPT="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --repeat)
      REPEAT="${2:-}"
      shift 2
      ;;
    --poll)
      POLL_INTERVAL="${2:-}"
      shift 2
      ;;
    --max-frames)
      MAX_FRAMES="${2:-}"
      shift 2
      ;;
    --include-real)
      INCLUDE_REAL=1
      shift
      ;;
    --short-tape)
      SHORT_TAPE="${2:-}"
      shift 2
      ;;
    --medium-tape)
      MEDIUM_TAPE="${2:-}"
      shift 2
      ;;
    --real-tape)
      REAL_TAPE="${2:-}"
      shift 2
      ;;
    --csv-out)
      CSV_OUT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ ! "$SEG_FLOOR" =~ ^[0-9]+$ || ! "$SEG_CEILING" =~ ^[0-9]+$ ]]; then
  echo "ERROR: --seg-floor and --seg-ceiling must be integers" >&2
  exit 1
fi
if [[ ! "$REPEAT" =~ ^[0-9]+$ || "$REPEAT" -lt 1 ]]; then
  echo "ERROR: --repeat must be an integer >= 1" >&2
  exit 1
fi
if [[ ! "$POLL_INTERVAL" =~ ^[0-9]+$ || "$POLL_INTERVAL" -lt 1 ]]; then
  echo "ERROR: --poll must be an integer >= 1" >&2
  exit 1
fi
if [[ -n "$MAX_FRAMES" && ! "$MAX_FRAMES" =~ ^[0-9]+$ ]]; then
  echo "ERROR: --max-frames must be an integer" >&2
  exit 1
fi
if [[ "$VERIFY_RECEIPT" != "true" && "$VERIFY_RECEIPT" != "false" ]]; then
  echo "ERROR: --verify-receipt must be true or false" >&2
  exit 1
fi
for receipt in "${RECEIPTS[@]}"; do
  case "$receipt" in
    composite|succinct|groth16) ;;
    *)
      echo "ERROR: unsupported receipt kind: $receipt" >&2
      exit 1
      ;;
  esac
done

# shellcheck source=_prover-helpers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_prover-helpers.sh"

health_json="$(curl -sf --connect-timeout 10 "$PROVER_URL/health")" || {
  echo "ERROR: prover unreachable at $PROVER_URL" >&2
  exit 1
}

read -r SERVER_MIN_SEG SERVER_MAX_SEG ACCELERATOR DEV_MODE <<<"$(
  echo "$health_json" | python3 -c '
import json,sys
d=json.load(sys.stdin)
print(d.get("min_segment_limit_po2",""), d.get("max_segment_limit_po2",""),
      d.get("accelerator","unknown"), d.get("dev_mode","unknown"))'
)"

if [[ -z "$SERVER_MIN_SEG" || -z "$SERVER_MAX_SEG" ]]; then
  echo "ERROR: failed to parse segment bounds from /health" >&2
  exit 1
fi

EFFECTIVE_FLOOR="$SEG_FLOOR"
EFFECTIVE_CEILING="$SEG_CEILING"

if [[ "$STRICT_BOUNDS" -eq 1 ]]; then
  if [[ "$SEG_FLOOR" -lt "$SERVER_MIN_SEG" || "$SEG_CEILING" -gt "$SERVER_MAX_SEG" ]]; then
    echo "ERROR: requested segment range [$SEG_FLOOR..$SEG_CEILING] exceeds prover policy [$SERVER_MIN_SEG..$SERVER_MAX_SEG]" >&2
    exit 1
  fi
else
  if [[ "$EFFECTIVE_FLOOR" -lt "$SERVER_MIN_SEG" ]]; then
    EFFECTIVE_FLOOR="$SERVER_MIN_SEG"
  fi
  if [[ "$EFFECTIVE_CEILING" -gt "$SERVER_MAX_SEG" ]]; then
    EFFECTIVE_CEILING="$SERVER_MAX_SEG"
  fi
fi

if [[ "$EFFECTIVE_FLOOR" -gt "$EFFECTIVE_CEILING" ]]; then
  echo "ERROR: effective segment range is empty after bounds check" >&2
  echo "  requested: [$SEG_FLOOR..$SEG_CEILING]" >&2
  echo "  policy:    [$SERVER_MIN_SEG..$SERVER_MAX_SEG]" >&2
  exit 1
fi

if [[ -z "$CSV_OUT" ]]; then
  mkdir -p "$ROOT_DIR/batch-results"
  CSV_OUT="$ROOT_DIR/batch-results/segment-sweep-$(date -u +%Y%m%d-%H%M%S).csv"
fi
mkdir -p "$(dirname "$CSV_OUT")"

declare -a SEGMENTS=()
for ((seg = EFFECTIVE_FLOOR; seg <= EFFECTIVE_CEILING; seg++)); do
  SEGMENTS+=("$seg")
done

declare -a TAPES=()
if [[ -f "$SHORT_TAPE" ]]; then
  TAPES+=("short|$SHORT_TAPE")
else
  echo "WARN: short tape not found: $SHORT_TAPE (skipping)" >&2
fi
if [[ -f "$MEDIUM_TAPE" ]]; then
  TAPES+=("medium|$MEDIUM_TAPE")
else
  echo "WARN: medium tape not found: $MEDIUM_TAPE (skipping)" >&2
fi
if [[ "$INCLUDE_REAL" -eq 1 ]]; then
  if [[ -f "$REAL_TAPE" ]]; then
    TAPES+=("real|$REAL_TAPE")
  else
    echo "WARN: real tape not found: $REAL_TAPE (skipping)" >&2
  fi
fi

if [[ "${#TAPES[@]}" -eq 0 ]]; then
  echo "ERROR: no tapes available to benchmark" >&2
  exit 1
fi

printf "timestamp_utc,label,tape_path,tape_bytes,frame_count,final_score,seed,receipt_kind,segment_limit_po2,verify_receipt,max_frames,run,status,submit_http,job_id,wall_s,elapsed_ms,proof_segments,total_cycles,user_cycles,paging_cycles,reserved_cycles,produced_receipt_kind,error_code,error\n" > "$CSV_OUT"

sanitize_csv_field() {
  local value="${1:-}"
  value="${value//$'\n'/ }"
  value="${value//$'\r'/ }"
  value="${value//,/;}"
  printf "%s" "$value"
}

submit_and_wait() {
  local tape_label="$1"
  local tape_path="$2"
  local tape_bytes="$3"
  local tape_frames="$4"
  local tape_score="$5"
  local tape_seed="$6"
  local receipt="$7"
  local seg="$8"
  local run_idx="$9"

  local query="receipt_kind=${receipt}&segment_limit_po2=${seg}&verify_receipt=${VERIFY_RECEIPT}"
  if [[ -n "$MAX_FRAMES" ]]; then
    query="${query}&max_frames=${MAX_FRAMES}"
  fi

  wait_for_idle

  local resp_raw http_code body
  resp_raw=$(http_status_and_body -X POST "${PROVER_URL}/api/jobs/prove-tape/raw?${query}" \
    --data-binary "@${tape_path}" -H "content-type: application/octet-stream")
  http_code=$(echo "$resp_raw" | tail -1)
  body=$(echo "$resp_raw" | sed '$d')

  local now
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  local label="${tape_label}-${receipt}-seg${seg}-run${run_idx}"

  if [[ "$http_code" != "202" ]]; then
    local err err_code
    err="$(echo "$body" | json_field error || true)"
    err_code="$(echo "$body" | json_field error_code || true)"
    err="$(sanitize_csv_field "$err")"
    err_code="$(sanitize_csv_field "$err_code")"
    printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,submit_failed,%s,,,,,,,,,%s,%s\n" \
      "$now" "$label" "$tape_path" "$tape_bytes" "$tape_frames" "$tape_score" "$tape_seed" \
      "$receipt" "$seg" "$VERIFY_RECEIPT" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$err_code" "$err" \
      >> "$CSV_OUT"
    echo "  FAIL submit: http=$http_code error_code=${err_code:-none} error=${err:-unknown}"
    return 1
  fi

  local job_id
  job_id="$(echo "$body" | json_field job_id)"
  if [[ -z "$job_id" ]]; then
    local err err_code
    err="$(sanitize_csv_field "$body")"
    err_code=""
    printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,submit_failed,%s,,,,,,,,,%s,%s\n" \
      "$now" "$label" "$tape_path" "$tape_bytes" "$tape_frames" "$tape_score" "$tape_seed" \
      "$receipt" "$seg" "$VERIFY_RECEIPT" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$err_code" "$err" \
      >> "$CSV_OUT"
    echo "  FAIL submit: missing job_id"
    return 1
  fi

  local wall_start wall_end wall_s
  wall_start="$(date +%s)"

  while true; do
    sleep "$POLL_INTERVAL"
    local jr status
    jr=$(curl -sf "${PROVER_URL}/api/jobs/${job_id}" 2>/dev/null) || continue
    status="$(echo "$jr" | json_field status)"

    if [[ "$status" == "succeeded" ]]; then
      wall_end="$(date +%s)"
      wall_s="$((wall_end - wall_start))"
      read -r elapsed_ms proof_segments total_cycles user_cycles paging_cycles reserved_cycles produced_receipt <<<"$(
        echo "$jr" | python3 -c '
import json,sys
d=json.load(sys.stdin)
r=d.get("result",{})
p=r.get("proof",{})
s=p.get("stats",{})
print(r.get("elapsed_ms",""),
      s.get("segments",""),
      s.get("total_cycles",""),
      s.get("user_cycles",""),
      s.get("paging_cycles",""),
      s.get("reserved_cycles",""),
      p.get("produced_receipt_kind",""))'
      )"

      printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,succeeded,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,,\n" \
        "$now" "$label" "$tape_path" "$tape_bytes" "$tape_frames" "$tape_score" "$tape_seed" \
        "$receipt" "$seg" "$VERIFY_RECEIPT" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$job_id" \
        "$wall_s" "$elapsed_ms" "$proof_segments" "$total_cycles" "$user_cycles" "$paging_cycles" "$reserved_cycles" "$produced_receipt" \
        >> "$CSV_OUT"

      echo "  OK  wall=${wall_s}s elapsed_ms=${elapsed_ms} segments=${proof_segments} total_cycles=${total_cycles}"
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" >/dev/null 2>&1 || true
      return 0
    fi

    if [[ "$status" == "failed" ]]; then
      wall_end="$(date +%s)"
      wall_s="$((wall_end - wall_start))"
      local err err_code
      err="$(sanitize_csv_field "$(echo "$jr" | json_field error || true)")"
      err_code="$(sanitize_csv_field "$(echo "$jr" | json_field error_code || true)")"
      printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,failed,%s,%s,%s,,,,,,,%s,%s\n" \
        "$now" "$label" "$tape_path" "$tape_bytes" "$tape_frames" "$tape_score" "$tape_seed" \
        "$receipt" "$seg" "$VERIFY_RECEIPT" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$job_id" "$wall_s" \
        "$err_code" "$err" \
        >> "$CSV_OUT"
      echo "  FAIL proof: wall=${wall_s}s error_code=${err_code:-none} error=${err:-unknown}"
      curl -sf -X DELETE "${PROVER_URL}/api/jobs/${job_id}" >/dev/null 2>&1 || true
      return 1
    fi
  done
}

echo "================================================"
echo "Segment Sweep Benchmark"
echo "$(date -u)"
echo "================================================"
echo "Prover URL:            $PROVER_URL"
echo "Accelerator:           $ACCELERATOR"
echo "Dev mode:              $DEV_MODE"
echo "Policy segment range:  [$SERVER_MIN_SEG..$SERVER_MAX_SEG]"
echo "Requested range:       [$SEG_FLOOR..$SEG_CEILING]"
echo "Effective range:       [$EFFECTIVE_FLOOR..$EFFECTIVE_CEILING]"
echo "Receipt kinds:         ${RECEIPTS[*]}"
echo "Verify receipt:        $VERIFY_RECEIPT"
echo "Repeat count:          $REPEAT"
echo "Poll interval (s):     $POLL_INTERVAL"
if [[ -n "$MAX_FRAMES" ]]; then
  echo "Max frames override:   $MAX_FRAMES"
else
  echo "Max frames override:   (none)"
fi
echo "CSV output:            $CSV_OUT"
echo ""

PASS=0
FAIL=0

for tape_entry in "${TAPES[@]}"; do
  IFS='|' read -r tape_label tape_path <<< "$tape_entry"
  read -r tape_frames tape_score tape_seed tape_bytes <<< "$(read_tape_header "$tape_path")"
  echo "== Tape: $tape_label ($(basename "$tape_path"), ${tape_bytes} bytes, frames=${tape_frames}) =="

  for receipt in "${RECEIPTS[@]}"; do
    for seg in "${SEGMENTS[@]}"; do
      for ((run_idx = 1; run_idx <= REPEAT; run_idx++)); do
        echo "-- case receipt=${receipt} seg=${seg} run=${run_idx}"
        if submit_and_wait "$tape_label" "$tape_path" "$tape_bytes" "$tape_frames" "$tape_score" "$tape_seed" "$receipt" "$seg" "$run_idx"; then
          PASS=$((PASS + 1))
        else
          FAIL=$((FAIL + 1))
        fi
      done
    done
  done
done

echo ""
echo "================================================"
echo "Done: ${PASS} succeeded, ${FAIL} failed"
echo "CSV:  $CSV_OUT"
echo "================================================"

if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
