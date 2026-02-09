#!/usr/bin/env bash
#
# evolve.sh — Progressive learning loop for autopilot bot evolution
#
# Usage:
#   ./evolve/evolve.sh              # Run 100 iterations (default)
#   ./evolve/evolve.sh 50           # Run 50 iterations
#   ./evolve/evolve.sh 10 --resume  # Resume from current state, run 10 more
#
set -euo pipefail

EVOLVE_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$EVOLVE_DIR")"
MAX_ITERATIONS="${1:-100}"
RESUME_FLAG="${2:-}"

STATE_FILE="$EVOLVE_DIR/state.json"
LESSONS_FILE="$EVOLVE_DIR/journal/lessons.md"
GUIDE_FILE="$EVOLVE_DIR/GUIDE.md"
SEEDS_FILE="$EVOLVE_DIR/seeds.txt"
ROSTER_FILE="$PROJECT_DIR/src/bots/roster.rs"

# ── Helpers ──────────────────────────────────────────────────────────

log() { printf '[evolve %s] %s\n' "$(date +%H:%M:%S)" "$*"; }

read_state_field() {
  python3 -c "import json,sys; print(json.load(open('$STATE_FILE'))['$1'])"
}

# ── Pre-flight checks ───────────────────────────────────────────────

if ! command -v claude >/dev/null 2>&1; then
  log "ERROR: 'claude' CLI not found in PATH"
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  # Fall back to python3 for JSON parsing
  if ! command -v python3 >/dev/null 2>&1; then
    log "ERROR: need 'jq' or 'python3' for JSON parsing"
    exit 1
  fi
fi

if [ ! -f "$STATE_FILE" ]; then
  log "ERROR: $STATE_FILE not found. Run from repo root."
  exit 1
fi

# ── Initial build ────────────────────────────────────────────────────

log "Building release binary..."
cd "$PROJECT_DIR"
cargo build --release 2>&1 | tail -3
log "Build OK"

# ── Determine starting iteration ─────────────────────────────────────

CURRENT_ITER=$(read_state_field iteration)
if [ "$RESUME_FLAG" = "--resume" ]; then
  START_ITER=$((CURRENT_ITER + 1))
  log "Resuming from iteration $START_ITER"
else
  START_ITER=$((CURRENT_ITER + 1))
fi

END_ITER=$((START_ITER + MAX_ITERATIONS - 1))
log "Will run iterations $START_ITER through $END_ITER"

# ── Main evolution loop ──────────────────────────────────────────────

for ITER in $(seq "$START_ITER" "$END_ITER"); do
  ITER_PAD=$(printf '%03d' "$ITER")
  PREV_ITER=$((ITER - 1))
  PREV_PAD=$(printf '%03d' "$PREV_ITER")

  log "═══════════════════════════════════════════════════"
  log "ITERATION $ITER / $END_ITER"
  log "═══════════════════════════════════════════════════"

  # ── Build the prompt ───────────────────────────────────────────────

  PROMPT_FILE=$(mktemp /tmp/evolve-prompt-XXXXXX.md)
  trap "rm -f '$PROMPT_FILE'" EXIT

  # Gather recent iteration reports (last 5)
  RECENT_REPORTS=""
  for LOOKBACK in $(seq $((ITER - 5 < 1 ? 1 : ITER - 5)) $((ITER - 1))); do
    LB_PAD=$(printf '%03d' "$LOOKBACK")
    REPORT="$EVOLVE_DIR/journal/iteration-${LB_PAD}.md"
    if [ -f "$REPORT" ]; then
      RECENT_REPORTS="$RECENT_REPORTS
---
$(cat "$REPORT")
"
    fi
  done

  # Gather recent history entries from state.json (last 10)
  HISTORY_JSON=$(python3 -c "
import json, sys
state = json.load(open('$STATE_FILE'))
history = state.get('history', [])[-10:]
for h in history:
    print(f\"  iter={h['iteration']} avg_score={h.get('avg_score',0):.0f} improved={h.get('improved','?')} change=\\\"{h.get('change','?')}\\\"\")" 2>/dev/null || echo "  (no history yet)")

  cat > "$PROMPT_FILE" << EVOLVE_PROMPT
# AUTOPILOT EVOLUTION — ITERATION $ITER

You are an AI bot designer iteratively improving an Asteroids autopilot bot. This is iteration **$ITER** of an automated progressive learning loop. You are running headlessly inside \`claude -p\` in the \`rust-autopilot/\` directory.

## YOUR MISSION
Analyze previous performance, make ONE targeted improvement to the \`evolve-candidate\` bot, benchmark it, and record everything for the next iteration.

## CRITICAL RULES
1. Make **ONE** focused change per iteration. Small delta = clear signal.
2. **ALWAYS** run the benchmark after code changes. Never skip.
3. **ALWAYS** write the iteration report. Future you depends on it.
4. If you regress, **REVERT** to the best design and record what failed.
5. Think about **WHY** the ship dies, not just how to tweak numbers.
6. If 3+ consecutive regressions, try a fundamentally different direction.

## CURRENT STATE
\`\`\`
iteration: $ITER (prev: $PREV_ITER)
best_iteration: $(read_state_field best_iteration)
best_avg_score: $(read_state_field best_avg_score)
best_max_score: $(read_state_field best_max_score)
consecutive_regressions: $(read_state_field consecutive_regressions)
total_improvements: $(read_state_field total_improvements)
total_regressions: $(read_state_field total_regressions)
\`\`\`

### Recent History
$HISTORY_JSON

## STEP-BY-STEP PROCESS

### Step 1: Gather Context
Read these files:
- \`evolve/GUIDE.md\` — parameter reference and strategy tips
- \`evolve/journal/lessons.md\` — cumulative learnings from all prior iterations
- \`src/bots/roster.rs\` — find the \`evolve-candidate\` SearchConfig at the bottom of \`search_bot_configs()\`

$(if [ "$PREV_ITER" -gt 0 ]; then echo "Since this is not the first iteration, also read:
- \`evolve/scores/iteration-${PREV_PAD}/summary.json\` — last benchmark results
- \`evolve/scores/iteration-${PREV_PAD}/runs.csv\` — per-seed breakdown
- \`evolve/journal/iteration-${PREV_PAD}.md\` — last iteration's report"
fi)

### Step 2: Analyze & Decide
Based on your analysis:
- What was the primary weakness in the last run?
- What parameter change would address that?
- What's your hypothesis for improvement?

If you want deeper death analysis on a specific seed, run:
\`\`\`
cargo run --release -- codex-intel-run --bot evolve-candidate --seed <HEX_SEED> --max-frames 108000
\`\`\`

### Step 3: Edit the Bot
Edit the \`evolve-candidate\` SearchConfig in \`src/bots/roster.rs\`.
The config is at the end of \`search_bot_configs()\`, marked with a comment.
Change ONE parameter (or a small related group).

### Step 4: Build & Benchmark
\`\`\`
cargo build --release 2>&1
\`\`\`
If build fails, fix and retry.

\`\`\`
cargo run --release -- benchmark --bots evolve-candidate,omega-marathon --seed-file evolve/seeds.txt --max-frames 108000 --objective score --save-top 3 --jobs 8 --out-dir evolve/scores/iteration-${ITER_PAD}
\`\`\`

### Step 5: Evaluate Results
Read \`evolve/scores/iteration-${ITER_PAD}/summary.json\`.
Compare evolve-candidate avg_score to:
- Previous iteration's score
- Best-ever score ($(read_state_field best_avg_score))
- omega-marathon's score (reference)

### Step 6: Write Iteration Report
Write to \`evolve/journal/iteration-${ITER_PAD}.md\` with this structure:
\`\`\`
# Iteration $ITER

## Change
What: [what you changed]
Why: [your hypothesis]
Parameters: [param] [old] -> [new]

## Results
| Metric | evolve-candidate | omega-marathon | best-ever |
|--------|-----------------|----------------|-----------|
| avg_score | X | Y | Z |
| max_score | ... | ... | ... |
| avg_frames | ... | ... | ... |

## Assessment
[Improved/Regressed/Neutral] — [explanation]

## Key Observations
- ...

## Next Steps
- ...
\`\`\`

### Step 7: Update State
Read \`evolve/state.json\`, then write it back with these changes:
- Set \`iteration\` to $ITER
- Set \`last_avg_score\`, \`last_max_score\`, \`last_avg_frames\` from results
- If IMPROVED (avg_score > best_avg_score):
  - Update \`best_avg_score\`, \`best_max_score\`, \`best_avg_frames\`, \`best_avg_lives\`
  - Set \`best_iteration\` to $ITER
  - Reset \`consecutive_regressions\` to 0
  - Increment \`total_improvements\`
  - Snapshot config: copy the evolve-candidate SearchConfig block to \`evolve/designs/iteration-${ITER_PAD}.rs\`
- If REGRESSED:
  - Increment \`consecutive_regressions\` and \`total_regressions\`
  - REVERT: read \`evolve/designs/iteration-$(printf '%03d' $(read_state_field best_iteration)).rs\` and restore that config into roster.rs
  - Then run \`cargo build --release\` to verify the revert compiles
- Append to \`history\` array: { "iteration": $ITER, "avg_score": X, "improved": bool, "change": "brief description" }

### Step 8: Update Lessons
If you learned something useful, append to \`evolve/journal/lessons.md\`.
One line per insight. Be specific:
- BAD: "changed risk weight" (too vague)
- GOOD: "risk_weight_asteroid=1.6 (up from 1.45) reduced asteroid deaths 20% but cost 5% score"

$(if [ -n "$RECENT_REPORTS" ]; then echo "## RECENT ITERATION REPORTS (for context)
$RECENT_REPORTS"; fi)

EVOLVE_PROMPT

  PROMPT_SIZE=$(wc -c < "$PROMPT_FILE")
  log "Prompt built: ${PROMPT_SIZE} bytes"

  # ── Run Claude Code ────────────────────────────────────────────────

  log "Launching Claude Code for iteration $ITER..."
  CLAUDE_START=$(date +%s)

  set +e
  claude -p \
    --dangerously-skip-permissions \
    --verbose \
    --output-format text \
    --max-turns 40 \
    < "$PROMPT_FILE" \
    > "$EVOLVE_DIR/journal/claude-output-${ITER_PAD}.log" 2>&1
  CLAUDE_EXIT=$?
  set -e

  CLAUDE_END=$(date +%s)
  CLAUDE_DURATION=$((CLAUDE_END - CLAUDE_START))
  log "Claude Code finished in ${CLAUDE_DURATION}s (exit=$CLAUDE_EXIT)"

  # ── Verify iteration advanced ──────────────────────────────────────

  NEW_ITER=$(read_state_field iteration)
  if [ "$NEW_ITER" -lt "$ITER" ]; then
    log "WARNING: Claude did not advance iteration counter ($NEW_ITER < $ITER)"
    log "Attempting manual state advancement..."

    # Force advance the iteration counter so the loop can continue
    python3 -c "
import json
state = json.load(open('$STATE_FILE'))
state['iteration'] = $ITER
state['history'].append({
    'iteration': $ITER,
    'avg_score': 0,
    'improved': False,
    'change': 'FAILED: Claude did not complete iteration'
})
json.dump(state, open('$STATE_FILE', 'w'), indent=2)
"
    log "State force-advanced to iteration $ITER"
  fi

  # ── Check for stop conditions ──────────────────────────────────────

  CONSEC_REG=$(read_state_field consecutive_regressions)
  if [ "$CONSEC_REG" -ge 8 ]; then
    log "WARNING: $CONSEC_REG consecutive regressions. Consider stopping."
    log "(Continuing anyway — the prompt tells Claude to try a different direction)"
  fi

  # ── Summary ────────────────────────────────────────────────────────

  BEST_SCORE=$(read_state_field best_avg_score)
  BEST_ITER=$(read_state_field best_iteration)
  log "Best so far: avg_score=$BEST_SCORE (iteration $BEST_ITER)"
  log ""

  # Clean up temp file
  rm -f "$PROMPT_FILE"
  trap - EXIT

  # Small delay between iterations to avoid rate limits
  sleep 2
done

log "═══════════════════════════════════════════════════"
log "EVOLUTION COMPLETE"
log "Ran iterations $START_ITER through $END_ITER"
log "Best: avg_score=$(read_state_field best_avg_score) at iteration $(read_state_field best_iteration)"
log "Improvements: $(read_state_field total_improvements) | Regressions: $(read_state_field total_regressions)"
log "═══════════════════════════════════════════════════"
