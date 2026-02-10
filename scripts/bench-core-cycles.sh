#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERIFIER_DIR="$ROOT_DIR/risc0-asteroids-verifier"
SHORT_TAPE="$ROOT_DIR/test-fixtures/test-short.tape"
MEDIUM_TAPE="$ROOT_DIR/test-fixtures/test-medium.tape"
REAL_TAPE="$ROOT_DIR/test-fixtures/test-real-game.tape"

OUT_DIR=""
THRESHOLD_MODE="off" # off|check
THRESHOLDS_FILE="$VERIFIER_DIR/benchmarks/core-cycle-thresholds.env"
MAX_FRAMES=""
PPROF_CASE="none" # none|short|medium|real

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/bench-core-cycles.sh [options]

Run deterministic guest-cycle benchmarks against fixed tapes using the
dev-mode benchmark binary (RISC0_DEV_MODE=1; no secure proving, no CUDA).

Options:
  --out-dir <path>         Output directory for logs/csv/summary
                           Default: risc0-asteroids-verifier/benchmarks/runs/core-cycles-<timestamp>
  --threshold-mode <mode>  off|check (default: off)
  --thresholds <path>      Threshold env file
                           Default: risc0-asteroids-verifier/benchmarks/core-cycle-thresholds.env
  --max-frames <n>         Optional max-frames override passed to benchmark binary
  --pprof-case <case>      none|short|medium|real (default: none)
                           Captures one RISC0 pprof profile and writes a top report if Go is installed.
  -h, --help               Show this help

Examples:
  bash scripts/bench-core-cycles.sh
  bash scripts/bench-core-cycles.sh --threshold-mode check
  bash scripts/bench-core-cycles.sh --pprof-case medium --threshold-mode check
USAGE_EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --threshold-mode)
      THRESHOLD_MODE="${2:-}"
      shift 2
      ;;
    --thresholds)
      THRESHOLDS_FILE="${2:-}"
      shift 2
      ;;
    --max-frames)
      MAX_FRAMES="${2:-}"
      shift 2
      ;;
    --pprof-case)
      PPROF_CASE="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ "$THRESHOLD_MODE" != "off" && "$THRESHOLD_MODE" != "check" ]]; then
  echo "Invalid --threshold-mode value: $THRESHOLD_MODE (expected off|check)" >&2
  exit 1
fi
if [[ -n "$MAX_FRAMES" && ! "$MAX_FRAMES" =~ ^[0-9]+$ ]]; then
  echo "Invalid --max-frames value: $MAX_FRAMES (expected integer)" >&2
  exit 1
fi
if [[ "$PPROF_CASE" != "none" && "$PPROF_CASE" != "short" && "$PPROF_CASE" != "medium" && "$PPROF_CASE" != "real" ]]; then
  echo "Invalid --pprof-case value: $PPROF_CASE (expected none|short|medium|real)" >&2
  exit 1
fi

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$VERIFIER_DIR/benchmarks/runs/core-cycles-$(date -u +%Y%m%d-%H%M%S)"
fi
mkdir -p "$OUT_DIR"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

ensure_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Required file not found: $path" >&2
    exit 1
  fi
}

compare_float_le() {
  local actual="$1"
  local expected_max="$2"
  awk -v a="$actual" -v b="$expected_max" 'BEGIN { exit !(a <= b) }'
}

metric_for_case() {
  local case_name="$1"
  local column_name="$2"
  awk -F',' -v c="$case_name" -v target="$column_name" '
    NR == 1 {
      idx = 0
      for (i = 1; i <= NF; i++) {
        if ($i == target) {
          idx = i
          break
        }
      }
      next
    }
    $1 == c && idx > 0 {
      print $idx
      exit
    }
  ' "$METRICS_CSV"
}

require_cmd cargo
require_cmd python3
ensure_file "$SHORT_TAPE"
ensure_file "$MEDIUM_TAPE"
ensure_file "$REAL_TAPE"

METRICS_CSV="$OUT_DIR/metrics.csv"
cat > "$METRICS_CSV" <<'CSV_EOF'
case,frames,total_cycles,cycles_per_frame,segments,real_s,max_rss_bytes,log_path,pprof_path,pprof_top_path
CSV_EOF

run_case() {
  local case_name="$1"
  local tape_path="$2"
  local json_file="$OUT_DIR/${case_name}.json"
  local run_log="$OUT_DIR/${case_name}.log"
  local pprof_path=""
  local pprof_top_path=""
  local frames=""
  local total_cycles=""
  local cycles_per_frame=""
  local segments=""
  local real_s=""
  local max_rss_bytes=""
  local start_s end_s

  start_s=$(python3 -c 'import time; print(time.time())')
  local -a bench_cmd=(
    cargo run --quiet -p host --release --no-default-features --bin benchmark --
    --tape "$tape_path"
    --json-out "$json_file"
  )
  if [[ -n "$MAX_FRAMES" ]]; then
    bench_cmd+=(--max-frames "$MAX_FRAMES")
  fi

  local attempt=1
  local max_attempts=3
  while true; do
    : > "$run_log"
    if [[ "$PPROF_CASE" == "$case_name" ]]; then
      pprof_path="$OUT_DIR/${case_name}.pprof"
      if (
        cd "$VERIFIER_DIR"
        env RISC0_DEV_MODE=1 RISC0_PPROF_OUT="$pprof_path" "${bench_cmd[@]}"
      ) >"$run_log" 2>&1; then
        break
      fi
    else
      if (
        cd "$VERIFIER_DIR"
        env RISC0_DEV_MODE=1 "${bench_cmd[@]}"
      ) >"$run_log" 2>&1; then
        break
      fi
    fi

    if grep -q "Operation not permitted (os error 1)" "$run_log" && [[ "$attempt" -lt "$max_attempts" ]]; then
      echo "  WARN: transient guest execute failure for $case_name (attempt $attempt/$max_attempts), retrying..."
      attempt=$((attempt + 1))
      sleep 1
      continue
    fi

    echo "Benchmark execution failed for case=$case_name. See $run_log" >&2
    cat "$run_log" >&2
    exit 1
  done
  end_s=$(python3 -c 'import time; print(time.time())')

  if [[ ! -f "$json_file" ]]; then
    echo "Benchmark did not produce JSON output for case=$case_name. Expected $json_file" >&2
    exit 1
  fi
  read -r frames total_cycles cycles_per_frame segments <<<"$(python3 -c '
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    d = json.load(fh)
print(d["frame_count"], d["total_cycles"], d["cycles_per_frame"], d["segments"])
' "$json_file")"
  real_s=$(python3 -c "print(f'{float($end_s) - float($start_s):.2f}')")

  if [[ -z "$frames" || -z "$total_cycles" || -z "$cycles_per_frame" || -z "$segments" ]]; then
    echo "Failed to parse benchmark output for case=$case_name. See $json_file" >&2
    exit 1
  fi
  max_rss_bytes="NA"

  if [[ -n "$pprof_path" && -f "$pprof_path" && "$(command -v go || true)" != "" ]]; then
    pprof_top_path="$OUT_DIR/${case_name}.pprof.top.txt"
    go tool pprof -top "$pprof_path" > "$pprof_top_path" 2>&1 || true
  fi

  printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n" \
    "$case_name" \
    "$frames" \
    "$total_cycles" \
    "$cycles_per_frame" \
    "$segments" \
    "$real_s" \
    "$max_rss_bytes" \
    "$run_log" \
    "$pprof_path" \
    "$pprof_top_path" \
    >> "$METRICS_CSV"

  echo "  $case_name: frames=$frames cycles=$total_cycles cpf=$cycles_per_frame segments=$segments"
}

check_thresholds() {
  ensure_file "$THRESHOLDS_FILE"
  # shellcheck disable=SC1090
  source "$THRESHOLDS_FILE"

  local failures=0
  check_case_le() {
    local case_name="$1"
    local actual="$2"
    local max_allowed="$3"
    if ! compare_float_le "$actual" "$max_allowed"; then
      echo "FAIL: $case_name total_cycles actual=$actual exceeds max=$max_allowed" >&2
      failures=$((failures + 1))
    fi
  }

  local short_cycles medium_cycles real_cycles
  short_cycles="$(metric_for_case short total_cycles || true)"
  medium_cycles="$(metric_for_case medium total_cycles || true)"
  real_cycles="$(metric_for_case real total_cycles || true)"

  if [[ -z "${SHORT_MAX_TOTAL_CYCLES:-}" || -z "${MEDIUM_MAX_TOTAL_CYCLES:-}" || -z "${REAL_MAX_TOTAL_CYCLES:-}" ]]; then
    echo "Threshold file missing one of: SHORT_MAX_TOTAL_CYCLES, MEDIUM_MAX_TOTAL_CYCLES, REAL_MAX_TOTAL_CYCLES" >&2
    exit 1
  fi

  if [[ -n "$short_cycles" ]]; then
    check_case_le "short" "$short_cycles" "$SHORT_MAX_TOTAL_CYCLES"
  fi
  if [[ -n "$medium_cycles" ]]; then
    check_case_le "medium" "$medium_cycles" "$MEDIUM_MAX_TOTAL_CYCLES"
  fi
  if [[ -n "$real_cycles" ]]; then
    check_case_le "real" "$real_cycles" "$REAL_MAX_TOTAL_CYCLES"
  fi

  if [[ "$failures" -gt 0 ]]; then
    echo "Threshold checks failed ($failures). See $OUT_DIR" >&2
    exit 1
  fi

  echo "Threshold checks passed."
}

write_summary() {
  local summary_file="$OUT_DIR/summary.md"
  {
    echo "# Core Cycle Benchmark Summary"
    echo
    echo "- Generated (UTC): $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo "- Output directory: \`$OUT_DIR\`"
    echo "- Threshold mode: \`$THRESHOLD_MODE\`"
    echo "- Threshold file: \`$THRESHOLDS_FILE\`"
    echo "- Pprof case: \`$PPROF_CASE\`"
    if [[ -n "$MAX_FRAMES" ]]; then
      echo "- Max frames override: \`$MAX_FRAMES\`"
    fi
    echo
    echo "## Metrics"
    echo
    echo "| Case | Frames | Total Cycles | Cycles/Frame | Segments | Real (s) | Max RSS (bytes) |"
    echo "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"
    tail -n +2 "$METRICS_CSV" | while IFS=, read -r case_name frames total_cycles cpf segments real_s max_rss _log _pprof _top; do
      echo "| $case_name | $frames | $total_cycles | $cpf | $segments | $real_s | $max_rss |"
    done
    echo
    echo "## Artifacts"
    echo
    echo "- CSV: \`$METRICS_CSV\`"
    while IFS=, read -r case_name _frames _total _cpf _segments _real _rss log_path pprof_path pprof_top_path; do
      if [[ "$case_name" == "case" ]]; then
        continue
      fi
      echo "- ${case_name} log: \`$log_path\`"
      if [[ -n "$pprof_path" ]]; then
        echo "- ${case_name} pprof: \`$pprof_path\`"
      fi
      if [[ -n "$pprof_top_path" ]]; then
        echo "- ${case_name} pprof top: \`$pprof_top_path\`"
      fi
    done < "$METRICS_CSV"
  } > "$summary_file"
  echo "Wrote summary: $summary_file"
}

echo "Building benchmark binary (CPU mode, no default host features)..."
(
  cd "$VERIFIER_DIR"
  cargo build -p host --release --no-default-features --bin benchmark >/dev/null
)

echo "Running cycle cases..."
run_case "short" "$SHORT_TAPE"
run_case "medium" "$MEDIUM_TAPE"
run_case "real" "$REAL_TAPE"

if [[ "$THRESHOLD_MODE" == "check" ]]; then
  check_thresholds
fi

write_summary
echo "Done. Metrics: $METRICS_CSV"
