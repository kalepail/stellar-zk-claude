#!/usr/bin/env bash
# Harness: Gemini CLI (@google/gemini-cli)
# Install: npm install -g @google/gemini-cli

HARNESS_NAME="gemini"
HARNESS_DISPLAY="Gemini CLI"
HARNESS_CMD="gemini"
HARNESS_MAX_TURNS=0  # no explicit turn limit

harness_check() {
  command -v gemini >/dev/null 2>&1
}

harness_install_hint() {
  echo "npm install -g @google/gemini-cli"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"

  cd "$working_dir"
  # gemini -p reads prompt, -y auto-approves actions
  gemini -p "$(cat "$prompt_file")" \
    -y \
    --output-format text \
    > "$output_log" 2>&1
}

harness_env_check() {
  if [ -z "${GOOGLE_API_KEY:-}" ] && [ -z "${GEMINI_API_KEY:-}" ]; then
    echo "ERROR: Set GOOGLE_API_KEY or GEMINI_API_KEY for Gemini CLI."
    return 1
  fi
  return 0
}
