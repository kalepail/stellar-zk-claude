# _helpers.sh — Shared helper library for stellar-asteroids-contract scripts.
# Source this file — do not execute directly.
#
# Provides:
#   - Logging:     info, ok, err, warn
#   - Paths:       SCRIPT_DIR, CONTRACT_DIR, ROOT_DIR, FIXTURES_DIR, WASM
#   - Constants:   RISC0_ROUTER, RISC0_MOCK, NETWORK, HORIZON_URL
#   - Crypto:      sha256_of_hex
#   - Keys:        ensure_funded_key
#   - State:       load_state, save_state
#   - Fixtures:    read_image_id
#   - Mock prover: mock_seal
#
# Callers must set SCRIPT_DIR before sourcing (for path resolution):
#   SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
#   source "$SCRIPT_DIR/_helpers.sh"

# ---------------------------------------------------------------------------
# Paths (derived from caller's SCRIPT_DIR)
# ---------------------------------------------------------------------------
CONTRACT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$CONTRACT_DIR/.." && pwd)"
FIXTURES_DIR="$ROOT_DIR/test-fixtures"
WASM="$CONTRACT_DIR/target/wasm32v1-none/release/asteroids_score.wasm"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
RISC0_ROUTER="CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD"
RISC0_MOCK="CCKXGODVBNCGZZIKTU2DIPTXPVSLIG5Z67VYPAL4X5HVSED7VI4OD6A3"
NETWORK="${NETWORK:-testnet}"
HORIZON_URL="https://horizon-testnet.stellar.org"

# CPU instruction limit per Stellar transaction
CPU_LIMIT=100000000

# ---------------------------------------------------------------------------
# Logging — all output goes to stderr so callers can capture stdout for data
# ---------------------------------------------------------------------------
info()  { echo -e "\033[1;34m==>\033[0m $*" >&2; }
ok()    { echo -e "\033[1;32m OK\033[0m $*" >&2; }
err()   { echo -e "\033[1;31mERR\033[0m $*" >&2; }
warn()  { echo -e "\033[1;33mWRN\033[0m $*" >&2; }

# ---------------------------------------------------------------------------
# Prerequisite check
# ---------------------------------------------------------------------------
require_cmds() {
  local missing=0
  for cmd in "$@"; do
    if ! command -v "$cmd" &>/dev/null; then
      err "Missing required command: $cmd"
      missing=1
    fi
  done
  if [[ "$missing" -eq 1 ]]; then
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Crypto
# ---------------------------------------------------------------------------

# Compute SHA-256 of raw bytes from hex string
sha256_of_hex() {
  echo -n "$1" | xxd -r -p | shasum -a 256 | cut -d' ' -f1
}

# Validate that a journal hex payload encodes AST3 rules_digest at bytes 20..24.
# Expects little-endian u32 "AST3" = 0x41535433 => hex 33545341 at byte offset 20.
assert_ast3_rules_digest_in_journal_hex() {
  local journal_hex="${1:-}"
  local context="${2:-journal}"
  local expected_le_hex="33545341"

  if [[ ${#journal_hex} -lt 48 ]]; then
    err "$context: journal too short (${#journal_hex} hex chars, need at least 48)"
    return 1
  fi

  local rules_digest_le_hex="${journal_hex:40:8}"
  rules_digest_le_hex=$(printf '%s' "$rules_digest_le_hex" | tr '[:upper:]' '[:lower:]')
  if [[ "$rules_digest_le_hex" != "$expected_le_hex" ]]; then
    err "$context: rules_digest is not AST3 (found LE hex ${rules_digest_le_hex}, expected ${expected_le_hex})"
    err "$context: regenerate proof fixtures against the AST3 prover before running this script"
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Stellar key management
# ---------------------------------------------------------------------------

# Ensure a named key exists and is funded on the configured network.
# Args: $1 = key name
ensure_funded_key() {
  local key_name="$1"
  if ! stellar keys address "$key_name" &>/dev/null; then
    info "Generating key: $key_name"
    stellar keys generate "$key_name" --network "$NETWORK" --fund
    ok "Funded key: $key_name"
  else
    info "Using existing key: $key_name"
  fi
}

# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------

# Read image_id from first available fixture (all fixtures share the same id)
# Sets global: IMAGE_ID_HEX
read_image_id() {
  local id_file="$FIXTURES_DIR/proof-medium-groth16.image_id"
  if [[ ! -f "$id_file" ]]; then
    err "Image ID fixture not found: $id_file"
    err "Run: bun run scripts/generate-proof.ts first"
    exit 1
  fi
  IMAGE_ID_HEX=$(tr -d '[:space:]' < "$id_file")
}

# ---------------------------------------------------------------------------
# State file persistence
# ---------------------------------------------------------------------------

# Load state from a file into current shell.
# Args: $1 = state file path
load_state() {
  local state_file="$1"
  if [[ -f "$state_file" ]]; then
    # shellcheck disable=SC1090
    source "$state_file"
  fi
}

# ---------------------------------------------------------------------------
# Mock prover
# ---------------------------------------------------------------------------

# Generate a mock seal via the testnet mock verifier contract.
# Args: $1 = image_id_hex, $2 = journal_digest_hex
# Requires: DEPLOYER_NAME, RISC0_MOCK, NETWORK
mock_seal() {
  local img_id="$1" jd="$2"
  local result
  result=$(stellar contract invoke -q \
    --id "$RISC0_MOCK" \
    --source "$DEPLOYER_NAME" \
    --network "$NETWORK" \
    -- \
    mock_prove \
    --image_id "$img_id" \
    --journal_digest "$jd" 2>&1)
  echo "$result" | python3 -c "import sys,json; print(json.load(sys.stdin)['seal'])"
}

# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

# Format a number with thousand-separators (locale-dependent)
fmt_num() {
  printf "%'d" "$1" 2>/dev/null || printf "%d" "$1"
}

# Convert stroops (integer) to XLM string with 6 decimal places
stroops_to_xlm() {
  awk "BEGIN { printf \"%.6f\", ${1:-0} / 10000000 }"
}
