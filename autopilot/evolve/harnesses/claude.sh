#!/usr/bin/env bash
# Harness: Claude Code (@anthropic-ai/claude-code)
# Install: npm install -g @anthropic-ai/claude-code

HARNESS_NAME="claude"
HARNESS_DISPLAY="Claude Code"
HARNESS_CMD="claude"
HARNESS_MAX_TURNS=40

harness_check() {
  command -v claude >/dev/null 2>&1
}

harness_install_hint() {
  echo "npm install -g @anthropic-ai/claude-code"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"

  cd "$working_dir"
  claude -p \
    --dangerously-skip-permissions \
    --verbose \
    --output-format text \
    --max-turns "$HARNESS_MAX_TURNS" \
    < "$prompt_file" \
    > "$output_log" 2>&1
}

# Env vars this harness needs
harness_env_check() {
  if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    # Claude Code may use its own auth — only warn, don't fail
    echo "NOTE: ANTHROPIC_API_KEY not set. Claude Code may use its own session auth."
  fi
  return 0
}
