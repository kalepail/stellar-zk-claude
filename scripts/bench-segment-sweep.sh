#!/usr/bin/env bash
set -euo pipefail

# Benchmark a remote RISC Zero prover across a segment_limit_po2 sweep with
# configurable floors/ceilings and receipt kinds.
#
# Usage:
#   bash scripts/bench-segment-sweep.sh [prover-url] [options]
#
# Example:
#   bash scripts/bench-segment-sweep.sh http://127.0.0.1:8080 \
#     --seg-floor 19 --seg-ceiling 22 \
#     --receipts composite,succinct \
#     --repeat 2 --tapes all
#   bash scripts/bench-segment-sweep.sh https://your-prover.example.com \
#     --seg-floor 19 --seg-ceiling 22 \
#     --receipts composite,succinct \
#     --repeat 2 --tapes all

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHORT_TAPE="$ROOT_DIR/test-fixtures/test-short.tape"
MEDIUM_TAPE="$ROOT_DIR/test-fixtures/test-medium.tape"
REAL_TAPE="$ROOT_DIR/test-fixtures/test-real-game.tape"
DEFAULT_PROVER_URL="http://127.0.0.1:8080"

POLL_INTERVAL=5
SEG_FLOOR=19
SEG_CEILING=22
VERIFY_MODE="policy"
REPEAT=1
TAPES_CSV="short,medium"
MAX_FRAMES=""
BOUNDS_MODE="clamp"
CSV_OUT=""
RECEIPTS_CSV="composite,succinct"
declare -a RECEIPTS=()

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/bench-segment-sweep.sh [prover-url] [options]

Options:
  --seg-floor <n>            Requested segment_limit_po2 floor (default: 19)
  --seg-ceiling <n>          Requested segment_limit_po2 ceiling (default: 22)
  --bounds-mode <mode>       clamp|strict (default: clamp)
                             clamp = clamp to prover health bounds
                             strict = fail when requested range exceeds policy
  --receipts <csv>           Comma-separated receipt kinds to benchmark.
                             Values: composite|succinct|groth16
                             Default: composite + succinct
  --tapes <csv>              Comma-separated tape labels to benchmark.
                             Values: short|medium|real|all
                             Default: short,medium
  --repeat <n>               Repetitions per config (default: 1)
  --poll <seconds>           Poll interval while waiting for completion (default: 5)
  --max-frames <n>           Optional max_frames query parameter override
  --short-tape <path>        Override short tape path
  --medium-tape <path>       Override medium tape path
  --real-tape <path>         Override real tape path
  --csv-out <path>           Output CSV path
  -h, --help                 Show this help

Notes:
  - Runs sequentially (single-flight friendly).
  - Waits for prover idle state before each submission.
  - Records both wall-clock and prover elapsed_ms.
  - Skips tapes with final_score=0 (these are rejected by prover policy).

Examples:
  bash scripts/bench-segment-sweep.sh
  bash scripts/bench-segment-sweep.sh http://127.0.0.1:8080 --receipts composite,succinct
  bash scripts/bench-segment-sweep.sh https://<vast-host>:<port> --seg-floor 19 --seg-ceiling 22
USAGE_EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

PROVER_URL="$DEFAULT_PROVER_URL"
if [[ $# -gt 0 && "$1" != --* ]]; then
  PROVER_URL="${1%/}"
  shift
fi

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
    --bounds-mode)
      BOUNDS_MODE="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --receipts)
      RECEIPTS_CSV="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --tapes)
      TAPES_CSV="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
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

if [[ "$BOUNDS_MODE" != "clamp" && "$BOUNDS_MODE" != "strict" ]]; then
  echo "ERROR: --bounds-mode must be clamp or strict" >&2
  exit 1
fi
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
    composite|succinct|groth16) ;;
    *)
      echo "ERROR: unsupported receipt kind: $receipt" >&2
      exit 1
      ;;
  esac
  NORMALIZED_RECEIPTS+=("$receipt")
done
RECEIPTS=("${NORMALIZED_RECEIPTS[@]}")
if [[ -z "$TAPES_CSV" ]]; then
  echo "ERROR: --tapes cannot be empty" >&2
  exit 1
fi

declare -a REQUESTED_TAPES=()
if [[ "$TAPES_CSV" == "all" ]]; then
  REQUESTED_TAPES=("short" "medium" "real")
else
  IFS=',' read -r -a REQUESTED_TAPES <<< "$TAPES_CSV"
fi
if [[ "${#REQUESTED_TAPES[@]}" -eq 0 ]]; then
  echo "ERROR: --tapes must include at least one value" >&2
  exit 1
fi
for tape_kind in "${REQUESTED_TAPES[@]}"; do
  tape_kind="$(echo "$tape_kind" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  case "$tape_kind" in
    short|medium|real) ;;
    *)
      echo "ERROR: unsupported tape label in --tapes: $tape_kind" >&2
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

if [[ "$BOUNDS_MODE" == "strict" ]]; then
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
for tape_kind in "${REQUESTED_TAPES[@]}"; do
  tape_kind="$(echo "$tape_kind" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  tape_path=""
  case "$tape_kind" in
    short) tape_path="$SHORT_TAPE" ;;
    medium) tape_path="$MEDIUM_TAPE" ;;
    real) tape_path="$REAL_TAPE" ;;
  esac

  if [[ -f "$tape_path" ]]; then
    TAPES+=("${tape_kind}|${tape_path}")
  else
    echo "WARN: $tape_kind tape not found: $tape_path (skipping)" >&2
  fi
done

if [[ "${#TAPES[@]}" -eq 0 ]]; then
  echo "ERROR: no tapes available to benchmark" >&2
  exit 1
fi

printf "timestamp_utc,label,tape_path,tape_bytes,frame_count,final_score,seed,receipt_kind,segment_limit_po2,verify_mode,max_frames,run,status,submit_http,job_id,wall_s,elapsed_ms,proof_segments,total_cycles,user_cycles,paging_cycles,reserved_cycles,produced_receipt_kind,error_code,error\n" > "$CSV_OUT"

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

  local query="receipt_kind=${receipt}&segment_limit_po2=${seg}&verify_mode=${VERIFY_MODE}"
  if [[ -n "$MAX_FRAMES" ]]; then
    query="${query}&max_frames=${MAX_FRAMES}"
  fi
  query=$(with_claimant_query "$query")

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
      "$receipt" "$seg" "$VERIFY_MODE" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$err_code" "$err" \
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
      "$receipt" "$seg" "$VERIFY_MODE" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$err_code" "$err" \
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
        "$receipt" "$seg" "$VERIFY_MODE" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$job_id" \
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
        "$receipt" "$seg" "$VERIFY_MODE" "${MAX_FRAMES:-}" "$run_idx" "$http_code" "$job_id" "$wall_s" \
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
echo "Bounds mode:           $BOUNDS_MODE"
echo "Receipt kinds:         ${RECEIPTS[*]}"
echo "Tape set:              ${TAPES_CSV}"
echo "Verify mode:           $VERIFY_MODE"
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
  if [[ "$tape_score" == "0" ]]; then
    echo "WARN: skipping $(basename "$tape_path") because final_score=0 (prover rejects zero-score tapes)"
    continue
  fi
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
