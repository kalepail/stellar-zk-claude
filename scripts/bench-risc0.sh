#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERIFIER_DIR="$ROOT_DIR/risc0-asteroids-verifier"
SHORT_TAPE="$ROOT_DIR/test-fixtures/test-short.tape"
MEDIUM_TAPE="$ROOT_DIR/test-fixtures/test-medium.tape"
DEFAULT_THRESHOLDS_FILE="$VERIFIER_DIR/benchmarks/thresholds.env"

RUN_COVERAGE=1
RUN_DEV=1
RUN_SECURE_SHORT=1
RUN_SECURE_MEDIUM=0
CHECK_THRESHOLDS=0
OUT_DIR=""
THRESHOLDS_FILE="$DEFAULT_THRESHOLDS_FILE"

usage() {
  cat <<'USAGE_EOF'
Usage: scripts/bench-risc0.sh [options]

Run coverage + performance benchmarks for the RISC0 Asteroids verifier.

Options:
  --out-dir <path>         Write artifacts to this directory.
                           Default: risc0-asteroids-verifier/benchmarks/runs/<utc-timestamp>
  --check                  Enforce regression thresholds from thresholds file.
  --thresholds <path>      Use custom thresholds file (env format).
  --full                   Include secure medium-fixture proving (slow, expensive).
  --dev-only               Skip secure proving runs.
  --skip-coverage          Skip cargo-llvm-cov coverage run.
  -h, --help               Show this help.

Examples:
  bash scripts/bench-risc0.sh
  bash scripts/bench-risc0.sh --check
  bash scripts/bench-risc0.sh --full --check
  bash scripts/bench-risc0.sh --dev-only --out-dir /tmp/risc0-bench
USAGE_EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --check)
      CHECK_THRESHOLDS=1
      shift
      ;;
    --thresholds)
      THRESHOLDS_FILE="${2:-}"
      shift 2
      ;;
    --full)
      RUN_SECURE_MEDIUM=1
      shift
      ;;
    --dev-only)
      RUN_SECURE_SHORT=0
      RUN_SECURE_MEDIUM=0
      shift
      ;;
    --skip-coverage)
      RUN_COVERAGE=0
      shift
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

compare_float_ge() {
  local actual="$1"
  local expected_min="$2"
  awk -v a="$actual" -v b="$expected_min" 'BEGIN { exit !(a >= b) }'
}

now_utc() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

timestamp_utc() {
  date -u +"%Y%m%d-%H%M%S"
}

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$VERIFIER_DIR/benchmarks/runs/$(timestamp_utc)"
fi
mkdir -p "$OUT_DIR"

ensure_file "$SHORT_TAPE"
ensure_file "$MEDIUM_TAPE"
require_cmd cargo
require_cmd /usr/bin/time

if [[ "$RUN_COVERAGE" -eq 1 ]]; then
  if ! cargo llvm-cov --version >/dev/null 2>&1; then
    echo "cargo llvm-cov is not installed. Install with: cargo install cargo-llvm-cov --locked" >&2
    exit 1
  fi
fi

COVERAGE_REGIONS=""
COVERAGE_LINES=""
COVERAGE_FUNCTIONS=""

METRICS_CSV="$OUT_DIR/metrics.csv"
cat > "$METRICS_CSV" <<'CSV_EOF'
case,mode,frames,real_s,max_rss_bytes,segments,total_cycles,user_cycles,paging_cycles,reserved_cycles,log_path,pprof_path,pprof_top_path
CSV_EOF

echo "==> Benchmark output directory: $OUT_DIR"

append_metric_row() {
  local case_name="$1"
  local mode_label="$2"
  local frames="$3"
  local real_s="$4"
  local max_rss="$5"
  local segments="$6"
  local total_cycles="$7"
  local user_cycles="$8"
  local paging_cycles="$9"
  local reserved_cycles="${10}"
  local log_path="${11}"
  local pprof_path="${12}"
  local pprof_top_path="${13}"

  printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n" \
    "$case_name" \
    "$mode_label" \
    "$frames" \
    "$real_s" \
    "$max_rss" \
    "$segments" \
    "$total_cycles" \
    "$user_cycles" \
    "$paging_cycles" \
    "$reserved_cycles" \
    "$log_path" \
    "$pprof_path" \
    "$pprof_top_path" \
    >> "$METRICS_CSV"
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

run_coverage() {
  local log_file="$OUT_DIR/coverage.log"
  echo "==> Running coverage for asteroids-verifier-core"
  (
    cd "$VERIFIER_DIR"
    cargo llvm-cov -p asteroids-verifier-core --all-targets --summary-only
  ) > "$log_file" 2>&1

  COVERAGE_REGIONS="$(awk '/^TOTAL/ { gsub("%", "", $4); print $4; exit }' "$log_file")"
  COVERAGE_FUNCTIONS="$(awk '/^TOTAL/ { gsub("%", "", $7); print $7; exit }' "$log_file")"
  COVERAGE_LINES="$(awk '/^TOTAL/ { gsub("%", "", $10); print $10; exit }' "$log_file")"

  if [[ -z "$COVERAGE_REGIONS" || -z "$COVERAGE_LINES" || -z "$COVERAGE_FUNCTIONS" ]]; then
    echo "Failed to parse coverage totals from $log_file" >&2
    exit 1
  fi

  echo "    coverage regions:   ${COVERAGE_REGIONS}%"
  echo "    coverage lines:     ${COVERAGE_LINES}%"
  echo "    coverage functions: ${COVERAGE_FUNCTIONS}%"
}

run_case() {
  local case_name="$1"
  local mode="$2"
  local tape_path="$3"
  local with_pprof="$4"

  local log_file="$OUT_DIR/${case_name}.log"
  local pprof_file="$OUT_DIR/${case_name}.pprof"
  local pprof_top_file="$OUT_DIR/${case_name}.pprof.top.txt"

  echo "==> Running case: $case_name"

  local mode_label="secure"
  if [[ "$mode" == "1" ]]; then
    mode_label="dev"
  fi

  if [[ "$with_pprof" -eq 1 ]]; then
    (
      cd "$VERIFIER_DIR"
      /usr/bin/time -l env RISC0_INFO=1 RUST_LOG=info RISC0_DEV_MODE="$mode" RISC0_PPROF_OUT="$pprof_file" target/release/host --tape "$tape_path"
    ) > "$log_file" 2>&1
  else
    (
      cd "$VERIFIER_DIR"
      /usr/bin/time -l env RISC0_INFO=1 RUST_LOG=info RISC0_DEV_MODE="$mode" target/release/host --tape "$tape_path"
    ) > "$log_file" 2>&1
  fi

  local real_s
  local max_rss
  local frames
  local segments
  local total_cycles
  local user_cycles
  local paging_cycles
  local reserved_cycles
  local pprof_path=""
  local pprof_top_path=""

  real_s="$(awk '/ real / { print $1; exit }' "$log_file")"
  max_rss="$(awk '/maximum resident set size/ { print $1; exit }' "$log_file")"
  frames="$(sed -n 's/^  Frames:[[:space:]]*\([0-9][0-9]*\)$/\1/p' "$log_file" | head -n 1)"
  segments="$(sed -n 's/.*number of segments: \([0-9][0-9]*\)$/\1/p' "$log_file" | head -n 1)"
  total_cycles="$(sed -n 's/.*: \([0-9][0-9]*\) total cycles$/\1/p' "$log_file" | head -n 1)"
  user_cycles="$(sed -n 's/.*: \([0-9][0-9]*\) user cycles.*/\1/p' "$log_file" | head -n 1)"
  paging_cycles="$(sed -n 's/.*: \([0-9][0-9]*\) paging cycles.*/\1/p' "$log_file" | head -n 1)"
  reserved_cycles="$(sed -n 's/.*: \([0-9][0-9]*\) reserved cycles.*/\1/p' "$log_file" | head -n 1)"

  if [[ -z "$real_s" || -z "$max_rss" || -z "$frames" ]]; then
    echo "Failed to parse benchmark metrics for $case_name. See $log_file" >&2
    exit 1
  fi

  if [[ -z "$segments" ]]; then
    segments="NA"
  fi
  if [[ -z "$total_cycles" ]]; then
    total_cycles="NA"
  fi
  if [[ -z "$user_cycles" ]]; then
    user_cycles="NA"
  fi
  if [[ -z "$paging_cycles" ]]; then
    paging_cycles="NA"
  fi
  if [[ -z "$reserved_cycles" ]]; then
    reserved_cycles="NA"
  fi

  if [[ "$with_pprof" -eq 1 ]]; then
    pprof_path="$pprof_file"
    if command -v go >/dev/null 2>&1; then
      go tool pprof -top "$pprof_file" > "$pprof_top_file" 2>&1 || true
      pprof_top_path="$pprof_top_file"
    fi
  fi

  append_metric_row \
    "$case_name" \
    "$mode_label" \
    "$frames" \
    "$real_s" \
    "$max_rss" \
    "$segments" \
    "$total_cycles" \
    "$user_cycles" \
    "$paging_cycles" \
    "$reserved_cycles" \
    "$log_file" \
    "$pprof_path" \
    "$pprof_top_path"

  echo "    real_s=$real_s rss_bytes=$max_rss frames=$frames segments=$segments total_cycles=$total_cycles"
}

check_thresholds() {
  ensure_file "$THRESHOLDS_FILE"
  # shellcheck disable=SC1090
  source "$THRESHOLDS_FILE"

  local failures=0

  check_case_float_le() {
    local case_name="$1"
    local label="$2"
    local actual="$3"
    local max_allowed="$4"
    if ! compare_float_le "$actual" "$max_allowed"; then
      echo "FAIL: $case_name $label actual=$actual exceeds max=$max_allowed" >&2
      failures=$((failures + 1))
    fi
  }

  check_float_min() {
    local label="$1"
    local actual="$2"
    local min_allowed="$3"
    if ! compare_float_ge "$actual" "$min_allowed"; then
      echo "FAIL: $label actual=$actual is below min=$min_allowed" >&2
      failures=$((failures + 1))
    fi
  }

  if [[ "$RUN_COVERAGE" -eq 1 ]]; then
    check_float_min "coverage regions" "$COVERAGE_REGIONS" "$COVERAGE_MIN_REGIONS"
    check_float_min "coverage lines" "$COVERAGE_LINES" "$COVERAGE_MIN_LINES"
    check_float_min "coverage functions" "$COVERAGE_FUNCTIONS" "$COVERAGE_MIN_FUNCTIONS"
  fi

  local value

  value="$(metric_for_case dev_short real_s || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "dev_short" "real_s" "$value" "$DEV_SHORT_MAX_REAL_S"
  fi

  value="$(metric_for_case dev_medium real_s || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "dev_medium" "real_s" "$value" "$DEV_MEDIUM_MAX_REAL_S"
  fi

  value="$(metric_for_case secure_short real_s || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "secure_short" "real_s" "$value" "$SECURE_SHORT_MAX_REAL_S"
  fi

  value="$(metric_for_case secure_short max_rss_bytes || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "secure_short" "max_rss_bytes" "$value" "$SECURE_SHORT_MAX_RSS_BYTES"
  fi

  value="$(metric_for_case secure_medium real_s || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "secure_medium" "real_s" "$value" "$SECURE_MEDIUM_MAX_REAL_S"
  fi

  value="$(metric_for_case secure_medium max_rss_bytes || true)"
  if [[ -n "$value" ]]; then
    check_case_float_le "secure_medium" "max_rss_bytes" "$value" "$SECURE_MEDIUM_MAX_RSS_BYTES"
  fi

  value="$(metric_for_case dev_short total_cycles || true)"
  if [[ -n "$value" && "$value" != "NA" ]]; then
    check_case_float_le "dev_short" "total_cycles" "$value" "$DEV_SHORT_MAX_TOTAL_CYCLES"
  fi

  value="$(metric_for_case dev_medium total_cycles || true)"
  if [[ -n "$value" && "$value" != "NA" ]]; then
    check_case_float_le "dev_medium" "total_cycles" "$value" "$DEV_MEDIUM_MAX_TOTAL_CYCLES"
  fi

  if [[ "$failures" -gt 0 ]]; then
    echo "Threshold checks failed ($failures). See $OUT_DIR/summary.md and logs in $OUT_DIR" >&2
    exit 1
  fi

  echo "==> Threshold checks passed"
}

write_summary() {
  local summary_file="$OUT_DIR/summary.md"
  {
    echo "# RISC0 Asteroids Bench Summary"
    echo
    echo "- Generated (UTC): $(now_utc)"
    echo "- Output directory: \`$OUT_DIR\`"
    echo "- Coverage run: $([[ "$RUN_COVERAGE" -eq 1 ]] && echo "yes" || echo "no")"
    echo "- Dev runs: $([[ "$RUN_DEV" -eq 1 ]] && echo "yes" || echo "no")"
    echo "- Secure short run: $([[ "$RUN_SECURE_SHORT" -eq 1 ]] && echo "yes" || echo "no")"
    echo "- Secure medium run: $([[ "$RUN_SECURE_MEDIUM" -eq 1 ]] && echo "yes" || echo "no")"
    echo "- Threshold checks: $([[ "$CHECK_THRESHOLDS" -eq 1 ]] && echo "enabled" || echo "disabled")"
    echo

    if [[ "$RUN_COVERAGE" -eq 1 ]]; then
      echo "## Coverage"
      echo
      echo "- Regions: ${COVERAGE_REGIONS}%"
      echo "- Lines: ${COVERAGE_LINES}%"
      echo "- Functions: ${COVERAGE_FUNCTIONS}%"
      echo "- Raw log: \`$OUT_DIR/coverage.log\`"
      echo
      echo "> Note: workspace-wide guest coverage is not included because \`instrument-coverage\` is not supported by the RISC0 guest toolchain."
      echo
    fi

    echo "## Performance Runs"
    echo
    echo "| Case | Mode | Frames | Real (s) | Max RSS (bytes) | Segments | Total cycles | User cycles | Paging cycles | Reserved cycles |"
    echo "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"

    tail -n +2 "$METRICS_CSV" | while IFS=, read -r case_name mode_label frames real_s max_rss segments total_cycles user_cycles paging_cycles reserved_cycles _log _pprof _pprof_top; do
      echo "| $case_name | $mode_label | $frames | $real_s | $max_rss | $segments | $total_cycles | $user_cycles | $paging_cycles | $reserved_cycles |"
    done

    echo
    echo "## Artifacts"
    echo
    echo "- Metrics CSV: \`$METRICS_CSV\`"

    while IFS=, read -r case_name _mode _frames _real _rss _segments _total _user _paging _reserved log_path pprof_path pprof_top_path; do
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

  echo "==> Wrote summary: $summary_file"
}

echo "==> Building host release binary"
(
  cd "$VERIFIER_DIR"
  cargo build -p host --release >/dev/null
)

if [[ "$RUN_COVERAGE" -eq 1 ]]; then
  run_coverage
fi

if [[ "$RUN_DEV" -eq 1 ]]; then
  run_case "dev_short" "1" "$SHORT_TAPE" "1"
  run_case "dev_medium" "1" "$MEDIUM_TAPE" "1"
fi

if [[ "$RUN_SECURE_SHORT" -eq 1 ]]; then
  run_case "secure_short" "0" "$SHORT_TAPE" "0"
fi

if [[ "$RUN_SECURE_MEDIUM" -eq 1 ]]; then
  run_case "secure_medium" "0" "$MEDIUM_TAPE" "0"
fi

write_summary

if [[ "$CHECK_THRESHOLDS" -eq 1 ]]; then
  check_thresholds
fi

echo "==> Done"
