# Shared helper library for prover test scripts.
# Source this file — do not execute directly.
#
# Expects callers to set:
#   PROVER_URL   — base URL of the prover API
#   POLL_INTERVAL — seconds between poll iterations (default: 5)

POLL_INTERVAL="${POLL_INTERVAL:-5}"

# ── JSON helpers ─────────────────────────────────────────────────────

# Extract a top-level field from JSON on stdin.
#   echo '{"foo":1}' | json_field foo  →  1
json_field() {
  python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get(sys.argv[1],''))" "$1" 2>/dev/null
}

# Extract a nested field via dot-path from JSON on stdin.
#   echo '{"a":{"b":2}}' | json_field_nested a.b  →  2
json_field_nested() {
  python3 -c "
import sys, json
d = json.load(sys.stdin)
for k in sys.argv[1].split('.'):
    if isinstance(d, dict):
        d = d.get(k, '')
    else:
        d = ''
        break
print(d)
" "$1" 2>/dev/null
}

# ── HTTP helpers ─────────────────────────────────────────────────────

# Curl wrapper that returns body + HTTP status code on separate lines.
# Last line of output is the HTTP status code; everything before is the body.
#   result=$(http_status_and_body -X POST "$url" ...)
#   http_code=$(echo "$result" | tail -1)
#   body=$(echo "$result" | sed '$d')
http_status_and_body() {
  curl -s -w '\n%{http_code}' "$@"
}

# ── Prover helpers ───────────────────────────────────────────────────

# Poll /health until running_jobs==0 && queued_jobs==0.
# Uses globals: PROVER_URL, POLL_INTERVAL
wait_for_idle() {
  while true; do
    local h
    h=$(curl -sf --connect-timeout 5 "$PROVER_URL/health" 2>/dev/null) || { sleep "$POLL_INTERVAL"; continue; }
    local running queued
    running=$(echo "$h" | json_field running_jobs)
    queued=$(echo "$h"  | json_field queued_jobs)
    if [[ "$running" == "0" && "$queued" == "0" ]]; then
      return 0
    fi
    echo "  waiting (running=$running, queued=$queued)..."
    sleep "$POLL_INTERVAL"
  done
}

# Parse a ZKTP binary tape file header.
# Args: $1 = path to .tape file
# Outputs: "frames score seed size_bytes" on stdout
read_tape_header() {
  local tape_path="$1"
  python3 - "$tape_path" << 'PYEOF'
import struct, sys
path = sys.argv[1]
with open(path, 'rb') as f:
    data = f.read()
magic = struct.unpack_from('<I', data, 0)[0]
if magic != 0x5A4B5450:
    print('ERROR: not a ZKTP tape', file=sys.stderr)
    sys.exit(1)
seed    = struct.unpack_from('<I', data, 8)[0]
frames  = struct.unpack_from('<I', data, 12)[0]
score   = struct.unpack_from('<I', data, 16 + frames)[0]
print(f'{frames} {score} {seed} {len(data)}')
PYEOF
}
