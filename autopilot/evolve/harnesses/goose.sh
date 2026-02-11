#!/usr/bin/env bash
# Harness: Goose (Block/Square)
# Install: curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash
# Docs: https://block.github.io/goose/docs/tutorials/headless-goose/

HARNESS_NAME="goose"
HARNESS_DISPLAY="Goose"
HARNESS_CMD="goose"
HARNESS_MAX_TURNS=0  # controlled via GOOSE_MAX_TURNS env var

harness_check() {
  command -v goose >/dev/null 2>&1
}

harness_install_hint() {
  echo "curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash"
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
  # GOOSE_MODE=auto auto-approves actions
  GOOSE_MODE=auto \
  GOOSE_MAX_TURNS="${GOOSE_MAX_TURNS:-50}" \
  goose run \
    --no-session \
    --with-builtin developer \
    -t "$prompt_text" \
    > "$output_log" 2>&1
}

harness_env_check() {
  if [ -z "${GOOSE_PROVIDER:-}" ]; then
    echo "NOTE: GOOSE_PROVIDER not set. Goose will use its configured default."
  fi
  return 0
}
