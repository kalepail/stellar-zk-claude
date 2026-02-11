#!/usr/bin/env bash
# Harness: OpenAI Codex CLI (@openai/codex)
# Install: npm install -g @openai/codex

HARNESS_NAME="codex"
HARNESS_DISPLAY="OpenAI Codex CLI"
HARNESS_CMD="codex"
HARNESS_MAX_TURNS=0  # codex has no --max-turns; bounded by token budget

harness_check() {
  command -v codex >/dev/null 2>&1
}

harness_install_hint() {
  echo "npm install -g @openai/codex"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"

  cd "$working_dir"
  # codex exec reads from stdin with "-", --full-auto permits edits
  codex exec \
    --full-auto \
    -C "$working_dir" \
    - \
    < "$prompt_file" \
    > "$output_log" 2>&1
}

harness_env_check() {
  if [ -z "${OPENAI_API_KEY:-}" ] && [ -z "${CODEX_API_KEY:-}" ]; then
    echo "NOTE: OPENAI_API_KEY not set. Codex CLI may use its own session auth from 'codex login'."
  fi
  return 0
}
