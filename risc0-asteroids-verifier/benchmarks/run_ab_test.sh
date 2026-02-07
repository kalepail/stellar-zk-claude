#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

TAPE="${1:-../test-fixtures/test-short.tape}"
RESULTS="benchmarks/benchmark_results.txt"
CARGO_TOML="Cargo.toml"
CARGO_TOML_BAK="Cargo.toml.bak"

# Save original
cp "$CARGO_TOML" "$CARGO_TOML_BAK"

# Cleanup on exit
trap 'cp "$CARGO_TOML_BAK" "$CARGO_TOML"; rm -f "$CARGO_TOML_BAK" Cargo.toml.experiment' EXIT

run_experiment() {
  local name="$1" lto="$2" opt="$3" cgu="$4"

  echo ""
  echo "=========================================="
  echo "  Experiment: $name"
  echo "  lto=$lto  opt-level=$opt  codegen-units=$cgu"
  echo "=========================================="

  # Write patched Cargo.toml
  cat > Cargo.toml.experiment <<TOML
[workspace]
resolver = "2"
members = ["api-server", "asteroids-core", "host", "methods"]

[profile.dev]
opt-level = 3

[profile.release]
debug = 1
lto = ${lto}
opt-level = ${opt}
codegen-units = ${cgu}
TOML
  cp Cargo.toml.experiment "$CARGO_TOML"

  # Build (guest gets rebuilt with new profile)
  echo "  Building..."
  cargo build --release -p host --no-default-features 2>&1 | tail -3

  # Run with dev mode for cycle counts
  echo "  Running..."
  output=$(RISC0_DEV_MODE=1 RISC0_INFO=1 cargo run --release -p host --no-default-features -- \
    --tape "$TAPE" --allow-dev-mode 2>&1)

  total=$(echo "$output" | grep "Total cycles:" | awk '{print $NF}')
  user=$(echo "$output" | grep "User cycles:" | awk '{print $NF}')
  paging=$(echo "$output" | grep "Paging cycles:" | awk '{print $NF}')
  segments=$(echo "$output" | grep "Segments:" | awk '{print $NF}')

  echo "  => total=$total  user=$user  paging=$paging  segments=$segments"
  printf "%-18s | total=%-10s user=%-10s paging=%-10s segments=%s\n" \
    "$name" "$total" "$user" "$paging" "$segments" >> "$RESULTS"
}

echo "=== A/B Test: Compiler Profile Optimization ===" | tee "$RESULTS"
echo "Tape: $TAPE" | tee -a "$RESULTS"
echo "Date: $(date)" | tee -a "$RESULTS"
echo "" | tee -a "$RESULTS"

run_experiment "0-baseline"    true      3      1
run_experiment "1-thin-lto"    '"thin"'  3      1
run_experiment "2-fat-opt2"    true      2      1
run_experiment "3-fat-opts"    true      '"s"'  1
run_experiment "4-fat-optz"    true      '"z"'  1
run_experiment "5-thin-opts"   '"thin"'  '"s"'  1

echo ""
echo "=========================================="
echo "  All experiments complete!"
echo "=========================================="
echo ""
cat "$RESULTS"
