#!/usr/bin/env bash
# Harness: Aider (aider-chat)
# Install: pip install aider-chat
# Docs: https://aider.chat/docs/scripting.html
#
# NOTE: Aider works differently from the other harnesses. It's file-targeted
# rather than general-purpose. For the evolution loop, we pass the roster file
# as the target and the prompt as --message. Aider will edit the file directly.

HARNESS_NAME="aider"
HARNESS_DISPLAY="Aider"
HARNESS_CMD="aider"
HARNESS_MAX_TURNS=0  # aider auto-completes in one pass

harness_check() {
  command -v aider >/dev/null 2>&1
}

harness_install_hint() {
  echo "pip install aider-chat"
}

# Run a prompt file through the harness.
# Args: $1=prompt_file $2=output_log $3=working_dir
#
# IMPORTANT: Aider doesn't have a general "agent" mode like Claude/Codex.
# It edits files based on instructions but can't run arbitrary shell commands
# on its own. For the evolution loop, we use it to edit the bot config,
# then the outer loop runs benchmarks separately.
# See evolve.sh for the HARNESS_NEEDS_EXTERNAL_BENCH flag.
harness_exec() {
  local prompt_file="$1"
  local output_log="$2"
  local working_dir="$3"
  local prompt_text
  prompt_text="$(cat "$prompt_file")"

  cd "$working_dir"
  aider \
    --message "$prompt_text" \
    --yes \
    --no-auto-commits \
    --no-stream \
    src/bots/roster.rs \
    > "$output_log" 2>&1
}

harness_env_check() {
  if [ -z "${OPENAI_API_KEY:-}" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "ERROR: Set OPENAI_API_KEY or ANTHROPIC_API_KEY for Aider."
    return 1
  fi
  return 0
}

# Aider can't run cargo/benchmarks — the outer loop must handle this
HARNESS_NEEDS_EXTERNAL_BENCH=true
