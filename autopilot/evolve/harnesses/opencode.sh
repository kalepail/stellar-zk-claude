#!/usr/bin/env bash
# Harness: OpenCode (anomalyco/opencode)
# Install: npm i -g opencode-ai@latest
# Docs: https://opencode.ai/docs/cli/

HARNESS_NAME="opencode"
HARNESS_DISPLAY="OpenCode"
HARNESS_CMD="opencode"
HARNESS_MAX_TURNS=0  # no explicit turn limit

harness_check() {
  command -v opencode >/dev/null 2>&1
}

harness_install_hint() {
  echo "npm i -g opencode-ai@latest"
  echo "  or: curl -fsSL https://opencode.ai/install | bash"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"
  local prompt_text
  prompt_text="$(cat "$prompt_file")"

  cd "$working_dir"
  # opencode run takes the prompt as a positional argument
  # Permissions are auto-approved in non-interactive mode
  opencode run \
    --format json \
    "$prompt_text" \
    > "$output_log" 2>&1
}

harness_env_check() {
  # opencode uses its own config for provider/model selection
  return 0
}
