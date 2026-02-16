#!/usr/bin/env bash
#
# evolve.sh — Harness-agnostic progressive learning loop for bot evolution
#
# Usage:
#   ./autopilot/evolve/evolve.sh --harness claude              # Run 100 iterations with Claude Code
#   ./autopilot/evolve/evolve.sh --harness codex --iterations 50
#   ./autopilot/evolve/evolve.sh --harness goose               # Resume from current state (default behavior)
#   ./autopilot/evolve/evolve.sh --harness aider --iterations 20
#   ./autopilot/evolve/evolve.sh --harness codex --build-mode skip
#
# Supported harnesses: claude, codex, opencode, gemini, aider, cline, goose
#
set -euo pipefail

# ── Parse arguments ────────────────────────────────────────────────

HARNESS=""
MAX_ITERATIONS=100
BUILD_MODE="full"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --harness|-h)   HARNESS="$2"; shift 2 ;;
    --iterations|-n) MAX_ITERATIONS="$2"; shift 2 ;;
    --build-mode)   BUILD_MODE="$2"; shift 2 ;;
    --help)
      echo "Usage: evolve.sh --harness <name> [--iterations N] [--build-mode full|skip]"
      echo ""
      echo "Harnesses: claude, codex, opencode, gemini, aider, cline, goose"
      echo ""
      echo "Options:"
      echo "  --harness, -h     AI coding tool to use (required)"
      echo "  --iterations, -n  Number of iterations to run (default: 100)"
      echo "  --build-mode      full|skip (default: full)"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

if [ -z "$HARNESS" ]; then
  echo "ERROR: --harness is required. Use --help for usage."
  exit 1
fi

if ! [[ "$MAX_ITERATIONS" =~ ^[0-9]+$ ]] || [ "$MAX_ITERATIONS" -lt 1 ]; then
  echo "ERROR: --iterations must be an integer >= 1"
  exit 1
fi

if [ "$BUILD_MODE" != "full" ] && [ "$BUILD_MODE" != "skip" ]; then
  echo "ERROR: --build-mode must be full or skip"
  exit 1
fi

# ── Paths ──────────────────────────────────────────────────────────

EVOLVE_DIR="$(cd "$(dirname "$0")" && pwd)"
AUTOPILOT_DIR="$(dirname "$EVOLVE_DIR")"

STATE_FILE="$EVOLVE_DIR/state.json"
LESSONS_FILE="$EVOLVE_DIR/journal/lessons.md"
GUIDE_FILE="$EVOLVE_DIR/GUIDE.md"
SEEDS_FILE="$EVOLVE_DIR/seeds.txt"
ROSTER_FILE="$AUTOPILOT_DIR/src/bots/roster.rs"
HARNESS_FILE="$EVOLVE_DIR/harnesses/${HARNESS}.sh"

# ── Helpers ────────────────────────────────────────────────────────

log() { printf '[evolve %s] %s\n' "$(date +%H:%M:%S)" "$*"; }

read_state() {
  python3 -c "import json; print(json.load(open('$STATE_FILE'))['$1'])"
}

update_state_field() {
  local field="$1" value="$2"
  python3 -c "
import json
s = json.load(open('$STATE_FILE'))
s['$field'] = $value
json.dump(s, open('$STATE_FILE', 'w'), indent=2)
print('  $field =', s['$field'])
"
}

# Extract per-seed analysis from a runs.csv file
# Outputs a markdown table comparing evolve-candidate vs omega-marathon per seed
extract_per_seed_analysis() {
  local csv_file="$1"
  if [ ! -f "$csv_file" ]; then
    echo "(no per-seed data available)"
    return
  fi
  python3 -c "
import csv, sys
rows = list(csv.DictReader(open('$csv_file')))
evolve = {r['seed_hex']: r for r in rows if r.get('bot_id') == 'evolve-candidate'}
omega = {r['seed_hex']: r for r in rows if r.get('bot_id') == 'omega-marathon'}
if not evolve:
    print('(no evolve-candidate data)')
    sys.exit(0)
print('| Seed | evolve score | omega score | delta | frames | wave |')
print('|------|-------------|-------------|-------|--------|------|')
worst_seeds = []
for seed in sorted(evolve.keys()):
    e = evolve[seed]
    o = omega.get(seed, {})
    es = float(e.get('final_score', 0))
    os = float(o.get('final_score', 0)) if o else 0
    delta = es - os
    pct = f'+{delta/os*100:.0f}%' if os > 0 and delta >= 0 else f'{delta/os*100:.0f}%' if os > 0 else 'n/a'
    frames = e.get('frame_count', '?')
    wave = e.get('final_wave', '?')
    marker = ' **WORST**' if es < os * 0.7 else ''
    print(f'| {seed} | {es:.0f} | {os:.0f} | {pct} | {frames} | {wave} |{marker}')
    worst_seeds.append((es, seed))
worst_seeds.sort()
if worst_seeds:
    print()
    print(f'**Worst 2 seeds**: {worst_seeds[0][1]} ({worst_seeds[0][0]:.0f}), {worst_seeds[1][1]} ({worst_seeds[1][0]:.0f})')
    print(f'**Best 2 seeds**: {worst_seeds[-1][1]} ({worst_seeds[-1][0]:.0f}), {worst_seeds[-2][1]} ({worst_seeds[-2][0]:.0f})')
" 2>/dev/null || echo "(error parsing per-seed data)"
}

# Extract known failures from state history
extract_known_failures() {
  python3 -c "
import json
state = json.load(open('$STATE_FILE'))
failures = [h for h in state.get('history', []) if not h.get('improved', True)]
if not failures:
    print('(no failures recorded yet — this is the first run)')
else:
    for f in failures[-8:]:
        score = f.get('avg_score', 0)
        change = f.get('change', '?')
        print(f'  - iter {f[\"iteration\"]}: {change} (avg_score={score:.0f})')
" 2>/dev/null || echo "  (error reading history)"
}

# ── Load harness ───────────────────────────────────────────────────

if [ ! -f "$HARNESS_FILE" ]; then
  echo "ERROR: Harness '$HARNESS' not found at $HARNESS_FILE"
  echo "Available harnesses:"
  ls "$EVOLVE_DIR/harnesses/"*.sh 2>/dev/null | xargs -I{} basename {} .sh | sed 's/^/  /'
  exit 1
fi

# shellcheck source=/dev/null
source "$HARNESS_FILE"

log "Loaded harness: $HARNESS_DISPLAY ($HARNESS_NAME)"

# ── Pre-flight checks ─────────────────────────────────────────────

if ! command -v python3 >/dev/null 2>&1; then
  log "ERROR: python3 is required for JSON state management"
  exit 1
fi

if ! harness_check; then
  log "ERROR: '$HARNESS_CMD' not found in PATH"
  log "Install: $(harness_install_hint)"
  exit 1
fi

harness_env_check || exit 1

if [ ! -f "$STATE_FILE" ]; then
  log "ERROR: $STATE_FILE not found"
  exit 1
fi

if [ ! -f "$ROSTER_FILE" ]; then
  log "ERROR: $ROSTER_FILE not found. Is the autopilot crate checked out?"
  exit 1
fi

# Record which harness is being used
update_state_field harness "\"$HARNESS\""

# ── Initial build ──────────────────────────────────────────────────

if [ "$BUILD_MODE" = "full" ]; then
  log "Building release binary..."
  (cd "$AUTOPILOT_DIR" && cargo build --release 2>&1 | tail -5)
  log "Build OK"
fi

# ── Determine starting iteration ───────────────────────────────────

CURRENT_ITER=$(read_state iteration)
START_ITER=$((CURRENT_ITER + 1))
END_ITER=$((START_ITER + MAX_ITERATIONS - 1))

log "Will run iterations $START_ITER through $END_ITER using $HARNESS_DISPLAY"

# ── Main evolution loop ────────────────────────────────────────────

for ITER in $(seq "$START_ITER" "$END_ITER"); do
  ITER_PAD=$(printf '%03d' "$ITER")
  PREV_ITER=$((ITER - 1))
  PREV_PAD=$(printf '%03d' "$PREV_ITER")

  log "═══════════════════════════════════════════════════"
  log "ITERATION $ITER / $END_ITER  [$HARNESS_DISPLAY]"
  log "═══════════════════════════════════════════════════"

  # ── Gather context for prompt ──────────────────────────────────

  # Recent iteration reports (last 3 — focused, not overwhelming)
  RECENT_REPORTS=""
  for LOOKBACK in $(seq $((ITER - 3 < 1 ? 1 : ITER - 3)) $((ITER - 1))); do
    LB_PAD=$(printf '%03d' "$LOOKBACK")
    REPORT="$EVOLVE_DIR/journal/iteration-${LB_PAD}.md"
    if [ -f "$REPORT" ]; then
      RECENT_REPORTS="$RECENT_REPORTS
---
$(cat "$REPORT")
"
    fi
  done

  # Recent history from state.json (last 10)
  HISTORY_JSON=$(python3 -c "
import json
state = json.load(open('$STATE_FILE'))
history = state.get('history', [])[-10:]
for h in history:
    print(f\"  iter={h['iteration']} avg_score={h.get('avg_score',0):.0f} improved={h.get('improved','?')} change=\\\"{h.get('change','?')}\\\"\")" 2>/dev/null || echo "  (no history yet)")

  # Read current evolve-candidate config from roster.rs
  CURRENT_CONFIG=$(python3 -c "
import re
text = open('$ROSTER_FILE').read()
m = re.search(r'// ── evolve-candidate.*?},', text, re.DOTALL)
if m: print(m.group())
else: print('(could not extract config)')
")

  # Per-seed analysis from previous iteration
  PREV_RUNS_CSV="$EVOLVE_DIR/scores/iteration-${PREV_PAD}/runs.csv"
  PER_SEED_DATA=$(extract_per_seed_analysis "$PREV_RUNS_CSV")

  # Known failures from history
  KNOWN_FAILURES=$(extract_known_failures)

  # ── Build the prompt ───────────────────────────────────────────

  PROMPT_FILE=$(mktemp /tmp/evolve-prompt-XXXXXX.md)

  # Determine if this harness can run shell commands
  NEEDS_EXTERNAL="${HARNESS_NEEDS_EXTERNAL_BENCH:-false}"

  # Construct different prompts based on harness capability
  if [ "$NEEDS_EXTERNAL" = "true" ]; then
    # Edit-only harness (e.g., Aider) — focuses on analysis + code changes
    cat > "$PROMPT_FILE" << EDIT_PROMPT
# AUTOPILOT EVOLUTION — ITERATION $ITER (Edit-Only Mode)

You are improving an Asteroids autopilot bot. This is iteration **$ITER**.
Your job: analyze the context below and make ONE targeted improvement to the \`evolve-candidate\` SearchConfig.

## CRITICAL RULES
1. Make **ONE** focused change per iteration. Small delta = clear signal.
2. If consecutive regressions >= 3, try a fundamentally different direction.
3. Think about **WHY** the ship dies, not just how to tweak numbers.

## CURRENT STATE
\`\`\`
iteration: $ITER (prev: $PREV_ITER)
best_iteration: $(read_state best_iteration)
best_avg_score: $(read_state best_avg_score)
best_max_score: $(read_state best_max_score)
consecutive_regressions: $(read_state consecutive_regressions)
total_improvements: $(read_state total_improvements)
total_regressions: $(read_state total_regressions)
\`\`\`

### Recent History
$HISTORY_JSON

### Known Failures (DO NOT REPEAT)
$KNOWN_FAILURES

## CURRENT CONFIG
\`\`\`rust
$CURRENT_CONFIG
\`\`\`

$(if [ "$PREV_ITER" -gt 0 ] && [ "$PER_SEED_DATA" != "(no per-seed data available)" ]; then echo "## PREVIOUS ITERATION PER-SEED BREAKDOWN
$PER_SEED_DATA"; fi)

## LESSONS LEARNED
$(cat "$LESSONS_FILE" 2>/dev/null || echo "(none yet)")

$(if [ -n "$RECENT_REPORTS" ]; then echo "## RECENT ITERATION REPORTS
$RECENT_REPORTS"; fi)

  ## YOUR TASK
  Edit the \`evolve-candidate\` SearchConfig in \`src/bots/roster.rs\`.
  Change ONE parameter (or a small related group). Explain your reasoning as a comment next to the change.
  Read \`evolve/GUIDE.md\` for parameter ranges, interaction rules, and strategy tips.

  IMPORTANT: The working directory is the autopilot crate root. Use \`src/\` for code and \`evolve/\` for evolution state/artifacts.
EDIT_PROMPT
  else
    # Full-agent harness — all steps (analyze + edit + build + benchmark + report)
    cat > "$PROMPT_FILE" << AGENT_PROMPT
# AUTOPILOT EVOLUTION — ITERATION $ITER

You are an AI bot designer iteratively improving an Asteroids autopilot bot. This is iteration **$ITER** of an automated progressive learning loop. You are running headlessly inside \`$HARNESS_DISPLAY\` in the autopilot crate root. The Rust code is in \`src/\` and evolution state/artifacts are in \`evolve/\`.

## YOUR MISSION
Analyze previous performance, make ONE targeted improvement to the \`evolve-candidate\` bot, benchmark it, and record everything for the next iteration.

## CRITICAL RULES
1. Make **ONE** focused change per iteration (or a small COORDINATED group of 2-3 related params). Small delta = clear signal.
2. **ALWAYS** run the benchmark after code changes. Never skip.
3. **ALWAYS** write the iteration report with a **per-seed score table**. Future you depends on it.
4. If you regress, **REVERT** to the best design and record what failed.
5. Think about **WHY** the ship dies, not just how to tweak numbers. Use \`codex-intel-run\` on worst seeds.
6. If 3+ consecutive regressions, try a fundamentally different direction.
7. **ALWAYS** update state.json and lessons.md before finishing.

## CURRENT STATE
\`\`\`
iteration: $ITER (prev: $PREV_ITER)
best_iteration: $(read_state best_iteration)
best_avg_score: $(read_state best_avg_score)
best_max_score: $(read_state best_max_score)
consecutive_regressions: $(read_state consecutive_regressions)
total_improvements: $(read_state total_improvements)
total_regressions: $(read_state total_regressions)
\`\`\`

### Recent History
$HISTORY_JSON

### Known Failures (DO NOT REPEAT these changes)
$KNOWN_FAILURES

$(if [ "$PREV_ITER" -gt 0 ] && [ "$PER_SEED_DATA" != "(no per-seed data available)" ]; then echo "## PREVIOUS ITERATION PER-SEED BREAKDOWN
$PER_SEED_DATA
"; fi)

## CURRENT CONFIG
\`\`\`rust
$CURRENT_CONFIG
\`\`\`

## STEP-BY-STEP PROCESS

### Step 1: Gather Context
Read these files:
- \`evolve/GUIDE.md\` — parameter ranges, interaction rules, reference scores, strategy
- \`evolve/journal/lessons.md\` — cumulative learnings from all prior iterations
- \`src/bots/roster.rs\` — find the \`evolve-candidate\` SearchConfig at the bottom of \`search_bot_configs()\`

$(if [ "$PREV_ITER" -gt 0 ]; then echo "Since this is not the first iteration, also read:
- \`evolve/scores/iteration-${PREV_PAD}/summary.json\` — last benchmark results
- \`evolve/scores/iteration-${PREV_PAD}/runs.csv\` — per-seed breakdown
- \`evolve/journal/iteration-${PREV_PAD}.md\` — last iteration's report"; fi)

### Step 2: Analyze & Decide
Based on your analysis:
- What was the primary weakness in the last run? (Look at worst seeds)
- What parameter change would address that?
- What's your hypothesis for improvement?
- Does your proposed change conflict with any known failures?

**IMPORTANT**: Run death analysis on the 1-2 worst seeds to understand WHY the bot died:
\`\`\`bash
cargo run --release -- codex-intel-run --bot evolve-candidate --seed <WORST_SEED_HEX> --max-frames 108000
\`\`\`
This shows death causes (asteroid/saucer/bullet), edge distances, and recent actions before each death.

### Step 3: Edit the Bot
Edit the \`evolve-candidate\` SearchConfig in \`src/bots/roster.rs\`.
The config is at the end of \`search_bot_configs()\`, marked with a comment.
Change ONE parameter (or a small COORDINATED group — see GUIDE.md for interaction rules).

### Step 4: Build & Benchmark
\`\`\`bash
cargo build --release 2>&1
\`\`\`
If build fails, fix and retry.

\`\`\`bash
cargo run --release -- benchmark \\
  --bots evolve-candidate,omega-marathon \\
  --seed-file evolve/seeds.txt \\
  --max-frames 108000 \\
  --objective score \\
  --save-top 3 \\
  --jobs 8 \\
  --out-dir evolve/scores/iteration-${ITER_PAD}
\`\`\`

### Step 5: Evaluate Results
Read \`evolve/scores/iteration-${ITER_PAD}/summary.json\` and \`runs.csv\`.
Compare evolve-candidate avg_score to:
- Previous iteration's score
- Best-ever score ($(read_state best_avg_score))
- omega-marathon's score (reference baseline)

### Step 6: Write Iteration Report (MANDATORY FORMAT)
Write to \`evolve/journal/iteration-${ITER_PAD}.md\` — this MUST include:

\`\`\`markdown
# Iteration $ITER

## Change
What: [what you changed and the specific values]
Why: [your hypothesis — what problem were you solving?]
Parameters: [param] [old_value] -> [new_value]

## Results Summary
| Metric | evolve-candidate | omega-marathon | best-ever |
|--------|-----------------|----------------|-----------|
| avg_score | X | Y | Z |
| max_score | ... | ... | ... |
| avg_frames | ... | ... | ... |

## Per-Seed Breakdown (REQUIRED — paste from runs.csv)
| Seed | evolve score | omega score | delta % | frames | wave |
|------|-------------|-------------|---------|--------|------|
| 0xDEADBEEF | ... | ... | ... | ... | ... |
(all 12 seeds)

**Worst seeds**: [list the 2-3 worst with scores]
**Best seeds**: [list the 2-3 best with scores]

## Death Analysis (if run)
[Summary of codex-intel-run findings on worst seeds — what killed the bot?]

## Assessment
[IMPROVED/REGRESSED/NEUTRAL] — [explanation of why]
Delta from previous: [+/-X%]
Delta from best-ever: [+/-X%]

## Key Observations
- [specific insight about per-seed patterns]
- [specific insight about death causes]
- [anything surprising or counterintuitive]

## Lessons for Future Iterations
- [specific, actionable insight — not vague]

## Next Steps
- [what should the next iteration try and why]
\`\`\`

### Step 7: Update State
Read \`evolve/state.json\`, then write it back with these changes:
- Set \`iteration\` to $ITER
- Set \`last_avg_score\`, \`last_max_score\`, \`last_avg_frames\` from results
- If **IMPROVED** (avg_score > best_avg_score):
  - Update \`best_avg_score\`, \`best_max_score\`, \`best_avg_frames\`, \`best_avg_lives\`
  - Set \`best_iteration\` to $ITER
  - Reset \`consecutive_regressions\` to 0
  - Increment \`total_improvements\`
  - Snapshot config: copy the evolve-candidate SearchConfig block to \`evolve/designs/iteration-${ITER_PAD}.rs\`
- If **REGRESSED** (avg_score < best_avg_score by more than 2%):
  - Increment \`consecutive_regressions\` and \`total_regressions\`
  - **REVERT**: read \`evolve/designs/iteration-$(printf '%03d' $(read_state best_iteration)).rs\` and restore that config into roster.rs
  - Then run \`cargo build --release\` to verify the revert compiles
- If **NEUTRAL** (within 2% of best):
  - Increment \`total_neutral\`
  - Keep the change (don't revert) — it might combine well with future changes
- Append to \`history\` array: \`{ "iteration": $ITER, "avg_score": X, "improved": bool, "change": "brief but specific description" }\`

### Step 8: Update Lessons
**ALWAYS** append to \`evolve/journal/lessons.md\` under "## Iteration History". Be SPECIFIC:
- BAD: "changed risk weight" (too vague, useless)
- GOOD: "iter006: risk_weight_saucer 2.8→3.2 = +8% avg_score. Saucer avoidance was weak on seeds 0xCAFE/0xBAAD where saucers spawn early. Bullet deaths also dropped because bot stays further from saucer bullet origin."
Include: what changed, exact values, result percentage, WHY it worked or failed, which seeds were affected.

$(if [ -n "$RECENT_REPORTS" ]; then echo "## RECENT ITERATION REPORTS (for context)
$RECENT_REPORTS"; fi)

## LESSONS LEARNED (read evolve/journal/lessons.md for full version)
$(head -40 "$LESSONS_FILE" 2>/dev/null || echo "(none yet)")
AGENT_PROMPT
  fi

  PROMPT_SIZE=$(wc -c < "$PROMPT_FILE")
  log "Prompt built: ${PROMPT_SIZE} bytes"

  # ── Run the harness ──────────────────────────────────────────────

  OUTPUT_LOG="$EVOLVE_DIR/journal/${HARNESS}-output-${ITER_PAD}.log"
  SCORES_DIR="$EVOLVE_DIR/scores/iteration-${ITER_PAD}"

  log "Launching $HARNESS_DISPLAY for iteration $ITER..."
  HARNESS_START=$(date +%s)

  set +e
  harness_exec "$PROMPT_FILE" "$OUTPUT_LOG" "$AUTOPILOT_DIR"
  HARNESS_EXIT=$?
  set -e

  HARNESS_END=$(date +%s)
  HARNESS_DURATION=$((HARNESS_END - HARNESS_START))
  log "$HARNESS_DISPLAY finished in ${HARNESS_DURATION}s (exit=$HARNESS_EXIT)"

  # ── External benchmarking for edit-only harnesses ────────────────

  if [ "${HARNESS_NEEDS_EXTERNAL_BENCH:-false}" = "true" ]; then
    log "Edit-only harness — running external build + benchmark..."

    cd "$AUTOPILOT_DIR"

    # Build
    set +e
    cargo build --release 2>&1 | tail -5
    BUILD_EXIT=$?
    set -e

    if [ "$BUILD_EXIT" -ne 0 ]; then
      log "ERROR: Build failed after harness edits. Reverting..."
      BEST_ITER_PAD=$(printf '%03d' "$(read_state best_iteration)")
      BEST_DESIGN="$EVOLVE_DIR/designs/iteration-${BEST_ITER_PAD}.rs"
      if [ -f "$BEST_DESIGN" ]; then
        python3 -c "
import re
roster = open('$ROSTER_FILE').read()
design = open('$BEST_DESIGN').read()
config_lines = [l for l in design.split('\n') if not l.strip().startswith('//')]
config_block = '\n'.join(config_lines).strip()
pattern = r'(// ── evolve-candidate.*?)(SearchConfig \{.*?id: \"evolve-candidate\".*?\},)'
roster_new = re.sub(pattern, r'\1' + config_block, roster, flags=re.DOTALL)
open('$ROSTER_FILE', 'w').write(roster_new)
"
        cargo build --release 2>&1 | tail -3
        log "Reverted to best design (iteration $(read_state best_iteration))"
      fi

      python3 -c "
import json
s = json.load(open('$STATE_FILE'))
s['iteration'] = $ITER
s['consecutive_regressions'] = s.get('consecutive_regressions', 0) + 1
s['total_regressions'] = s.get('total_regressions', 0) + 1
s['history'].append({'iteration': $ITER, 'avg_score': 0, 'improved': False, 'change': 'FAILED: build error after harness edits'})
json.dump(s, open('$STATE_FILE', 'w'), indent=2)
"
      rm -f "$PROMPT_FILE"
      sleep 2
      continue
    fi

    # Benchmark
    mkdir -p "$SCORES_DIR"
    log "Running benchmark..."
    cargo run --release -- benchmark \
      --bots evolve-candidate,omega-marathon \
      --seed-file "$SEEDS_FILE" \
      --max-frames 108000 \
      --objective score \
      --save-top 3 \
      --jobs 8 \
	      --out-dir "$SCORES_DIR" \
	      2>&1 | tee "$EVOLVE_DIR/journal/bench-output-${ITER_PAD}.log"

    # Evaluate results
    if [ -f "$SCORES_DIR/summary.json" ]; then
      EVAL_RESULT=$(python3 -c "
import json, csv

scores_dir = '$SCORES_DIR'
state_file = '$STATE_FILE'
iter_num = $ITER
iter_pad = '${ITER_PAD}'
	evolve_dir = '$EVOLVE_DIR'
harness_name = '$HARNESS'

summary = json.load(open(f'{scores_dir}/summary.json'))
state = json.load(open(state_file))

evolve = omega = None
for bot in summary.get('bots', summary.get('rankings', [])):
    name = bot.get('bot', bot.get('id', ''))
    if name == 'evolve-candidate': evolve = bot
    elif name == 'omega-marathon': omega = bot

if not evolve:
    print('ERROR: evolve-candidate not found')
    import sys; sys.exit(1)

avg_score = evolve.get('avg_score', 0)
max_score = evolve.get('max_score', 0)
avg_frames = evolve.get('avg_frames', 0)
best_avg = state.get('best_avg_score', 0)

improved = avg_score > best_avg
neutral = not improved and best_avg > 0 and abs(avg_score - best_avg) / best_avg < 0.02

state['iteration'] = iter_num

if improved:
    state['best_avg_score'] = avg_score
    state['best_max_score'] = max_score
    state['best_avg_frames'] = avg_frames
    state['best_iteration'] = iter_num
    state['consecutive_regressions'] = 0
    state['total_improvements'] = state.get('total_improvements', 0) + 1
    verdict = 'IMPROVED'
elif neutral:
    state['total_neutral'] = state.get('total_neutral', 0) + 1
    verdict = 'NEUTRAL'
else:
    state['consecutive_regressions'] = state.get('consecutive_regressions', 0) + 1
    state['total_regressions'] = state.get('total_regressions', 0) + 1
    verdict = 'REGRESSED'

state['history'].append({'iteration': iter_num, 'avg_score': avg_score, 'improved': improved, 'change': f'(edit-only via {harness_name})'})
json.dump(state, open(state_file, 'w'), indent=2)

# Build per-seed report
omega_avg = omega.get('avg_score', 0) if omega else 0
omega_max = omega.get('max_score', 0) if omega else 0
seed_table = ''
runs_csv = f'{scores_dir}/runs.csv'
try:
    rows = list(csv.DictReader(open(runs_csv)))
    ev = {r['seed_hex']: r for r in rows if r.get('bot_id') == 'evolve-candidate'}
    om = {r['seed_hex']: r for r in rows if r.get('bot_id') == 'omega-marathon'}
    seed_table = '## Per-Seed Breakdown\n| Seed | evolve | omega | delta | frames | wave |\n|------|--------|-------|-------|--------|------|\n'
    for s in sorted(ev.keys()):
        es = float(ev[s].get('final_score', 0))
        oss = float(om[s].get('final_score', 0)) if s in om else 0
        d = f'+{(es-oss)/oss*100:.0f}%' if oss > 0 and es >= oss else f'{(es-oss)/oss*100:.0f}%' if oss > 0 else 'n/a'
        seed_table += f'| {s} | {es:.0f} | {oss:.0f} | {d} | {ev[s].get(\"frame_count\",\"?\")} | {ev[s].get(\"final_wave\",\"?\")} |\n'
except: pass

report = f'''# Iteration {iter_num}

## Change
What: (edited by {harness_name} harness - see {harness_name}-output-{iter_pad}.log)

## Results
| Metric | evolve-candidate | omega-marathon | best-ever |
|--------|-----------------|----------------|-----------|
| avg_score | {avg_score:.0f} | {omega_avg:.0f} | {state[\"best_avg_score\"]:.0f} |
| max_score | {max_score:.0f} | {omega_max:.0f} | {state[\"best_max_score\"]:.0f} |
| avg_frames | {avg_frames:.0f} | {omega.get(\"avg_frames\", 0) if omega else 0:.0f} | {state.get(\"best_avg_frames\", 0):.0f} |

{seed_table}

## Assessment
{verdict}
'''
	with open(f'{evolve_dir}/journal/iteration-{iter_pad}.md', 'w') as f:
    f.write(report)

print(f'{verdict} avg_score={avg_score:.0f} best={state[\"best_avg_score\"]:.0f}')
")

      log "Evaluation: $EVAL_RESULT"

      # Handle revert if regressed
	      if echo "$EVAL_RESULT" | grep -q "REGRESSED"; then
	        log "Regressed — reverting to best design..."
	        BEST_ITER_PAD=$(printf '%03d' "$(read_state best_iteration)")
	        BEST_DESIGN="$EVOLVE_DIR/designs/iteration-${BEST_ITER_PAD}.rs"
        if [ -f "$BEST_DESIGN" ]; then
          python3 -c "
import re
roster = open('$ROSTER_FILE').read()
design = open('$BEST_DESIGN').read()
config_lines = [l for l in design.split('\n') if not l.strip().startswith('//')]
config_block = '\n'.join(config_lines).strip()
pattern = r'(// ── evolve-candidate.*?)(SearchConfig \{.*?id: \"evolve-candidate\".*?\},)'
roster_new = re.sub(pattern, r'\1' + config_block, roster, flags=re.DOTALL)
open('$ROSTER_FILE', 'w').write(roster_new)
"
          cargo build --release 2>&1 | tail -3
          log "Reverted to iteration $(read_state best_iteration)"
        fi
      fi

      # Snapshot config if improved
	      if echo "$EVAL_RESULT" | grep -q "IMPROVED"; then
	        python3 -c "
	import re
	text = open('$ROSTER_FILE').read()
	m = re.search(r'// ── evolve-candidate.*?},', text, re.DOTALL)
	if m:
	    with open('$EVOLVE_DIR/designs/iteration-${ITER_PAD}.rs', 'w') as f:
	        f.write(f'// Iteration ${ITER_PAD}\n')
	        f.write(m.group())
	"
        log "Saved design snapshot: designs/iteration-${ITER_PAD}.rs"
      fi
    else
      log "WARNING: No summary.json found — benchmark may have failed"
    fi
  fi

  # ── Verify iteration advanced (full-agent harnesses) ────────────

  if [ "${HARNESS_NEEDS_EXTERNAL_BENCH:-false}" != "true" ]; then
    NEW_ITER=$(read_state iteration)
    if [ "$NEW_ITER" -lt "$ITER" ]; then
      log "WARNING: Harness did not advance iteration counter ($NEW_ITER < $ITER)"
      log "Force-advancing state..."

      python3 -c "
import json
s = json.load(open('$STATE_FILE'))
s['iteration'] = $ITER
s['history'].append({'iteration': $ITER, 'avg_score': 0, 'improved': False, 'change': 'FAILED: $HARNESS_DISPLAY did not complete iteration'})
json.dump(s, open('$STATE_FILE', 'w'), indent=2)
"
      log "State force-advanced to iteration $ITER"
    fi
  fi

  # ── Check stop conditions ──────────────────────────────────────

  CONSEC_REG=$(read_state consecutive_regressions)
  if [ "$CONSEC_REG" -ge 8 ]; then
    log "WARNING: $CONSEC_REG consecutive regressions. Consider stopping."
  fi

  # ── Summary ────────────────────────────────────────────────────

  BEST_SCORE=$(read_state best_avg_score)
  BEST_ITER_NUM=$(read_state best_iteration)
  log "Best so far: avg_score=$BEST_SCORE (iteration $BEST_ITER_NUM)"
  log ""

  # Clean up temp file
  rm -f "$PROMPT_FILE"

  # Small delay between iterations
  sleep 2
done

log "═══════════════════════════════════════════════════"
log "EVOLUTION COMPLETE"
log "Ran iterations $START_ITER through $END_ITER using $HARNESS_DISPLAY"
log "Best: avg_score=$(read_state best_avg_score) at iteration $(read_state best_iteration)"
log "Improvements: $(read_state total_improvements) | Regressions: $(read_state total_regressions) | Neutral: $(read_state total_neutral)"
log "═══════════════════════════════════════════════════"
