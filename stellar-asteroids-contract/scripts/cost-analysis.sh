#!/usr/bin/env bash
# cost-analysis.sh
#
# Measure CPU instructions, read/write bytes, and resource fees for each
# operation on the Asteroids Score contract.  Deploys a throwaway instance
# on testnet (or reuses an existing one) and simulates operations via
# `stellar contract invoke --send=no` with RUST_LOG=trace to capture
# SorobanTransactionData from the simulation response.
#
# Prerequisites:
#   - `stellar` CLI v25+
#   - Contract built: `stellar contract build` in workspace root
#   - Proof fixtures in test-fixtures/
#   - `jq` installed
#
# Usage:
#   ./scripts/cost-analysis.sh                    # full deploy + measure
#   ./scripts/cost-analysis.sh --deploy-mode reuse # reuse existing deployment
#   ./scripts/cost-analysis.sh --deployer <name>  # custom deployer key name

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/_helpers.sh"

require_cmds stellar jq curl python3 xxd

DEPLOY_MODE="fresh" # fresh|reuse
RUN_ID=$(date +%s | tail -c 7)
DEPLOYER_NAME="ast-cost-${RUN_ID}"
PLAYER_NAME="ast-costp-${RUN_ID}"

STATE_FILE="$CONTRACT_DIR/.cost-analysis-state.env"

usage() {
  cat <<'USAGE_EOF'
Usage: stellar-asteroids-contract/scripts/cost-analysis.sh [options]

Options:
  --deploy-mode <mode>  fresh|reuse (default: fresh)
  --deployer <name>     Custom deployer key name
  -h, --help            Show this help
USAGE_EOF
}

# ---------------------------------------------------------------------------
# Parse args
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --deploy-mode)
      DEPLOY_MODE="$(echo "${2:-}" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --deployer)
      DEPLOYER_NAME="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$DEPLOY_MODE" != "fresh" && "$DEPLOY_MODE" != "reuse" ]]; then
  err "--deploy-mode must be fresh or reuse"
  exit 1
fi

# ---------------------------------------------------------------------------
# State persistence (cost-analysis-specific fields)
# ---------------------------------------------------------------------------
save_cost_state() {
  cat > "$STATE_FILE" << EOF
SCORE_CONTRACT_ID=$SCORE_CONTRACT_ID
TOKEN_ID=$TOKEN_ID
DEPLOYER_NAME=$DEPLOYER_NAME
PLAYER_NAME=$PLAYER_NAME
IMAGE_ID_HEX=$IMAGE_ID_HEX
DEPLOY_TX_HASH=${DEPLOY_TX_HASH:-}
SAC_TX_HASH=${SAC_TX_HASH:-}
EOF
}

# ---------------------------------------------------------------------------
# simulate_operation  --  run --send=no with trace, extract resources
#
# Stdout: tab-separated line: cpu_instructions read_bytes write_bytes resource_fee
# All diagnostics go to stderr via info/ok/warn/err.
# ---------------------------------------------------------------------------
simulate_operation() {
  local label="$1"
  shift
  # remaining args are the stellar contract invoke arguments

  info "Simulating: $label"

  local trace_output
  trace_output=$(RUST_LOG=trace stellar contract invoke \
    --send=no \
    "$@" 2>&1) || {
    err "Simulation failed for $label"
    echo "$trace_output" | tail -5 >&2
    echo "0	0	0	0"
    return
  }

  # Extract base64 transactionData from trace output
  local tx_data_b64
  tx_data_b64=$(echo "$trace_output" \
    | grep -oE '"transactionData"\s*:\s*"[A-Za-z0-9+/=]+"' \
    | head -1 \
    | sed 's/.*:.*"\([A-Za-z0-9+/=]*\)".*/\1/') || true

  if [[ -z "$tx_data_b64" ]]; then
    warn "Could not extract transactionData for $label"
    echo "0	0	0	0"
    return
  fi

  # Decode SorobanTransactionData
  local decoded
  decoded=$(echo "$tx_data_b64" \
    | stellar xdr decode --type SorobanTransactionData --output json 2>/dev/null) || {
    err "XDR decode failed for $label"
    echo "0	0	0	0"
    return
  }

  local cpu read_bytes write_bytes resource_fee
  cpu=$(echo "$decoded" | jq -r '.resources.instructions // 0')
  read_bytes=$(echo "$decoded" | jq -r '.resources.disk_read_bytes // .resources.read_bytes // 0')
  write_bytes=$(echo "$decoded" | jq -r '.resources.write_bytes // 0')
  resource_fee=$(echo "$decoded" | jq -r '.resource_fee // 0')

  ok "$label: $(fmt_num "$cpu") CPU, ${read_bytes}r/${write_bytes}w bytes, fee=$(fmt_num "$resource_fee")"

  echo "${cpu}	${read_bytes}	${write_bytes}	${resource_fee}"
}

# ---------------------------------------------------------------------------
# analyze_horizon_tx  --  fetch a submitted tx from Horizon
#
# Stdout: fee_charged (stroops, integer)
# ---------------------------------------------------------------------------
analyze_horizon_tx() {
  local label="$1" tx_hash="$2"

  if [[ -z "$tx_hash" ]]; then
    warn "No tx hash for $label"
    echo "0"
    return
  fi

  info "Fetching Horizon tx: $label ($tx_hash)"

  local resp
  resp=$(curl -sf "${HORIZON_URL}/transactions/${tx_hash}" 2>/dev/null) || {
    err "Horizon fetch failed for $tx_hash"
    echo "0"
    return
  }

  local fee_charged
  fee_charged=$(echo "$resp" | jq -r '.fee_charged // "0"')

  ok "$label: fee_charged = $(fmt_num "$fee_charged") stroops ($(stroops_to_xlm "$fee_charged") XLM)"

  echo "$fee_charged"
}

# ---------------------------------------------------------------------------
# Deploy throwaway contract instance for cost measurement
# ---------------------------------------------------------------------------
deploy_cost_instance() {
  info "Building contract..."
  (cd "$CONTRACT_DIR" && stellar contract build)
  ok "WASM built: $(wc -c < "$WASM" | tr -d ' ') bytes"

  ensure_funded_key "$DEPLOYER_NAME"
  ensure_funded_key "$PLAYER_NAME"
  DEPLOYER_ADDR=$(stellar keys address "$DEPLOYER_NAME")
  PLAYER_ADDR=$(stellar keys address "$PLAYER_NAME")
  info "Deployer address: $DEPLOYER_ADDR"
  info "Player address:   $PLAYER_ADDR"

  read_image_id
  info "Image ID: $IMAGE_ID_HEX"

  # Deploy SAC token
  info "Deploying COST token (SAC)..."
  local token_output
  token_output=$(stellar contract asset deploy \
    --asset "COST:$DEPLOYER_ADDR" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" 2>&1) || {
    token_output=$(stellar contract id asset \
      --asset "COST:$DEPLOYER_ADDR" \
      --network "$NETWORK" 2>&1)
  }
  TOKEN_ID=$(echo "$token_output" | grep -oE '^C[A-Z0-9]{55}$' | tail -1)
  ok "Token ID: $TOKEN_ID"

  # Try to capture the SAC deploy tx hash from output
  SAC_TX_HASH=$(echo "$token_output" | grep -oE '^[a-f0-9]{64}$' | head -1) || true

  # Deploy score contract
  info "Deploying score contract..."
  local deploy_output
  deploy_output=$(stellar contract deploy \
    --wasm "$WASM" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    --admin "$DEPLOYER_ADDR" \
    --router_id "$RISC0_ROUTER" \
    --image_id "${IMAGE_ID_HEX}" \
    --token_id "$TOKEN_ID" \
    2>&1)
  SCORE_CONTRACT_ID=$(echo "$deploy_output" | grep -oE '^C[A-Z0-9]{55}$' | tail -1)
  ok "Score contract ID: $SCORE_CONTRACT_ID"

  DEPLOY_TX_HASH=$(echo "$deploy_output" | grep -oE '^[a-f0-9]{64}$' | head -1) || true

  # Transfer token admin to score contract
  info "Transferring token admin to score contract..."
  stellar contract invoke -q \
    --id "$TOKEN_ID" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    set_admin \
    --new_admin "$SCORE_CONTRACT_ID" >/dev/null 2>&1
  ok "Token admin transferred"

  # Player needs a trustline to hold the COST token (SAC requires trustline for non-issuer)
  info "Creating COST trustline for player..."
  stellar tx new change-trust \
    --source "$PLAYER_NAME" \
    --line "COST:$DEPLOYER_ADDR" \
    --network "$NETWORK" >/dev/null 2>&1
  ok "Player trustline created"

  save_cost_state

  info "Cost analysis deployment:"
  echo "  Score contract: $SCORE_CONTRACT_ID" >&2
  echo "  Token:          $TOKEN_ID" >&2
  echo "  Deployer:       $DEPLOYER_ADDR" >&2
  echo "  Player:         $PLAYER_ADDR" >&2
}

# ---------------------------------------------------------------------------
# Measurement functions
# ---------------------------------------------------------------------------

# Results arrays (parallel indexed)
declare -a OP_NAMES=()
declare -a OP_CPU=()
declare -a OP_READ=()
declare -a OP_WRITE=()
declare -a OP_FEE=()

record_result() {
  local name="$1" cpu="$2" read_b="$3" write_b="$4" fee="$5"
  OP_NAMES+=("$name")
  OP_CPU+=("$cpu")
  OP_READ+=("$read_b")
  OP_WRITE+=("$write_b")
  OP_FEE+=("$fee")
}

# Generic: simulate an operation and record results.
# Args: label, then all stellar contract invoke args
measure_operation() {
  local label="$1"
  shift
  local result
  result=$(simulate_operation "$label" "$@")
  IFS=$'\t' read -r cpu read_b write_b fee <<< "$result"
  record_result "$label" "$cpu" "$read_b" "$write_b" "$fee"
}

measure_submit_score_groth16() {
  local fixture_prefix="proof-medium-groth16"
  local seal_file="$FIXTURES_DIR/${fixture_prefix}.seal"
  local journal_file="$FIXTURES_DIR/${fixture_prefix}.journal_raw"

  if [[ ! -f "$seal_file" || ! -f "$journal_file" ]]; then
    warn "SKIP: Groth16 fixture files not found"
    return
  fi

  local seal_hex journal_hex
  seal_hex=$(tr -d '[:space:]' < "$seal_file")
  journal_hex=$(tr -d '[:space:]' < "$journal_file")

  PLAYER_ADDR=$(stellar keys address "$PLAYER_NAME")

  measure_operation "submit_score (groth16)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$PLAYER_NAME" \
    --network "$NETWORK" \
    -- \
    submit_score \
    --seal "$seal_hex" \
    --journal_raw "$journal_hex" \
    --claimant "$PLAYER_ADDR"
}

measure_submit_score_mock() {
  local fixture_prefix="proof-medium-groth16"
  local journal_file="$FIXTURES_DIR/${fixture_prefix}.journal_raw"

  if [[ ! -f "$journal_file" ]]; then
    warn "SKIP: Mock fixture files not found"
    return
  fi

  local journal_hex
  journal_hex=$(tr -d '[:space:]' < "$journal_file")

  PLAYER_ADDR=$(stellar keys address "$PLAYER_NAME")

  local journal_digest_hex
  journal_digest_hex=$(sha256_of_hex "$journal_hex")

  info "Generating mock seal for cost measurement..."
  local seal_hex
  seal_hex=$(mock_seal "$IMAGE_ID_HEX" "$journal_digest_hex")

  measure_operation "submit_score (mock)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$PLAYER_NAME" \
    --network "$NETWORK" \
    -- \
    submit_score \
    --seal "$seal_hex" \
    --journal_raw "$journal_hex" \
    --claimant "$PLAYER_ADDR"
}

measure_set_image_id() {
  measure_operation "set_image_id (admin)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    set_image_id \
    --new_image_id "$IMAGE_ID_HEX"
}

measure_is_claimed() {
  local fake_digest="0000000000000000000000000000000000000000000000000000000000000001"

  measure_operation "is_claimed (read-only)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    is_claimed \
    --journal_digest "$fake_digest"
}

measure_image_id() {
  measure_operation "image_id (read-only)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    image_id
}

measure_router_id() {
  measure_operation "router_id (read-only)" \
    --id "$SCORE_CONTRACT_ID" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    router_id
}

# ---------------------------------------------------------------------------
# Print results table
# ---------------------------------------------------------------------------
print_results_table() {
  echo ""
  echo "================================================================"
  echo "Asteroids Score Contract -- Cost Analysis Results"
  echo "$(date)"
  echo "================================================================"
  echo ""

  # Header
  printf "%-26s  %12s  %6s  %7s  %8s  %12s  %10s\n" \
    "Operation" "CPU Instr" "%Limit" "Read B" "Write B" "Res Fee" "XLM"
  printf "%-26s  %12s  %6s  %7s  %8s  %12s  %10s\n" \
    "--------------------------" "------------" "------" "-------" "--------" "------------" "----------"

  local i
  for i in "${!OP_NAMES[@]}"; do
    local name="${OP_NAMES[$i]}"
    local cpu="${OP_CPU[$i]}"
    local read_b="${OP_READ[$i]}"
    local write_b="${OP_WRITE[$i]}"
    local fee="${OP_FEE[$i]}"

    local pct
    if [[ "$cpu" -gt 0 ]]; then
      pct=$(awk "BEGIN { printf \"%.1f%%\", ($cpu / $CPU_LIMIT) * 100 }")
    else
      pct="0.0%"
    fi

    local xlm
    xlm=$(stroops_to_xlm "$fee")

    printf "%-26s  %'12d  %6s  %'7d  %'8d  %'12d  %10s\n" \
      "$name" "$cpu" "$pct" "$read_b" "$write_b" "$fee" "$xlm"
  done

  # Deployment costs from Horizon
  echo ""
  echo "Deployment Costs (from Horizon):"

  local deploy_fee sac_fee
  deploy_fee=$(analyze_horizon_tx "Score contract deploy" "${DEPLOY_TX_HASH:-}")
  sac_fee=$(analyze_horizon_tx "SAC token deploy" "${SAC_TX_HASH:-}")

  if [[ "$deploy_fee" -gt 0 ]]; then
    printf "  Score contract:  %'d stroops (%s XLM)\n" "$deploy_fee" "$(stroops_to_xlm "$deploy_fee")"
  else
    echo "  Score contract:  (no tx hash captured)"
  fi

  if [[ "$sac_fee" -gt 0 ]]; then
    printf "  SAC token:       %'d stroops (%s XLM)\n" "$sac_fee" "$(stroops_to_xlm "$sac_fee")"
  else
    echo "  SAC token:       (no tx hash captured)"
  fi

  # Groth16 vs Mock comparison (if both were measured)
  local groth16_cpu=0 mock_cpu=0 groth16_fee=0 mock_fee=0
  for i in "${!OP_NAMES[@]}"; do
    case "${OP_NAMES[$i]}" in
      "submit_score (groth16)")
        groth16_cpu="${OP_CPU[$i]}"
        groth16_fee="${OP_FEE[$i]}"
        ;;
      "submit_score (mock)")
        mock_cpu="${OP_CPU[$i]}"
        mock_fee="${OP_FEE[$i]}"
        ;;
    esac
  done

  if [[ "$groth16_cpu" -gt 0 && "$mock_cpu" -gt 0 ]]; then
    echo ""
    local cpu_ratio fee_ratio groth16_pct
    cpu_ratio=$(awk "BEGIN { printf \"%.1f\", $groth16_cpu / $mock_cpu }")
    fee_ratio=$(awk "BEGIN { printf \"%.1f\", $groth16_fee / $mock_fee }")
    groth16_pct=$(awk "BEGIN { printf \"%.1f\", ($groth16_cpu / $CPU_LIMIT) * 100 }")
    echo "Groth16 vs Mock: ${cpu_ratio}x more CPU, ${fee_ratio}x more fee"
    echo "Groth16 % of tx CPU limit (100M): ${groth16_pct}%"
  fi

  echo ""
  echo "================================================================"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "================================================================" >&2
echo "Asteroids Score Contract -- Cost Analysis" >&2
echo "$(date)" >&2
echo "================================================================" >&2
echo "" >&2

if [[ "$DEPLOY_MODE" == "fresh" ]]; then
  deploy_cost_instance
else
  load_state "$STATE_FILE"
  read_image_id
  if [[ -z "${SCORE_CONTRACT_ID:-}" || -z "${TOKEN_ID:-}" || -z "${PLAYER_NAME:-}" ]]; then
    err "No deployment state found (or missing PLAYER_NAME). Run with --deploy-mode fresh first."
    exit 1
  fi
  info "Reusing deployment from $STATE_FILE"
  echo "  Score contract: $SCORE_CONTRACT_ID" >&2
  echo "  Token:          $TOKEN_ID" >&2
  echo "  Player:         $PLAYER_NAME" >&2
fi

echo "" >&2
info "Measuring operation costs..."
echo "" >&2

# Measure each operation
measure_submit_score_groth16
measure_submit_score_mock
measure_set_image_id
measure_is_claimed
measure_image_id
measure_router_id

# Print formatted results
print_results_table
