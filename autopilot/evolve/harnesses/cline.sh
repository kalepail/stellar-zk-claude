#!/usr/bin/env bash
# Harness: Cline CLI
# Install: npm i -g cline
# Docs: https://docs.cline.bot/cline-cli/overview

HARNESS_NAME="cline"
HARNESS_DISPLAY="Cline CLI"
HARNESS_CMD="cline"
HARNESS_MAX_TURNS=0  # no explicit turn limit; use --timeout instead

harness_check() {
  command -v cline >/dev/null 2>&1
}

harness_install_hint() {
  echo "npm i -g cline"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"

  cd "$working_dir"
  # -y = YOLO mode (auto-approve all), --timeout in seconds
  cline -y \
    --timeout 600 \
    < "$prompt_file" \
    > "$output_log" 2>&1
}

harness_env_check() {
  return 0
}
