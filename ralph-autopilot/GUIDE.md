# Ralph Autopilot — Progressive Evolution Loop

## What This Is
A harness-agnostic automated learning loop that iteratively improves the `evolve-candidate` SearchBot for a deterministic Asteroids game. Each iteration, a fresh AI agent context analyzes previous results, makes ONE targeted improvement, benchmarks it, and records everything.

## File Layout
```
ralph-autopilot/
├── evolve.sh                    # Main loop (harness-agnostic)
├── harnesses/                   # Per-tool harness adapters
├── state.json                   # Iteration state, best scores, history
├── seeds.txt                    # Fixed 12-seed evaluation set
├── GUIDE.md                     # This file
├── journal/
│   ├── lessons.md               # Cumulative lessons (append-only insights)
│   └── iteration-NNN.md         # Per-iteration reports
├── designs/
│   └── iteration-NNN.rs         # SearchConfig snapshots (for revert)
└── scores/
    └── iteration-NNN/           # Benchmark output per iteration
```

## The Bot Under Evolution
The `evolve-candidate` bot is a SearchBot defined in `src/bots/roster.rs` at the end of `search_bot_configs()`. It evaluates all 16 possible actions (4 bits: left, right, thrust, fire) every frame and picks the highest-utility action.

## Current Game Contract (AST3)

Use these values as ground truth for analysis and tuning:

- Ruleset: `AST3` (`RULES_DIGEST = 0x41535433`)
- Max tournament horizon: `108000` frames (30 minutes at 60 FPS)
- Score events: large asteroid `20`, medium asteroid `50`, small asteroid `100`, large saucer `200`, small saucer `990`
- Input legality: every frame byte must be strict-legal under the verifier (autopilot runners already enforce this)

If any iteration analysis or tooling assumes a `1000` small-saucer value, treat that as stale and fix it before trusting shot-hit inference.

### SearchConfig Parameters — EXPANDED RANGES

These ranges are based on cross-analysis of codex-autopilot (67K avg, 226K max) and claude-autopilot (35K avg, 137K max). The ranges are deliberately wide to allow exploration.

| Parameter | Description | Range | Sweet Spot | Effect |
|-----------|-------------|-------|------------|--------|
| `lookahead_frames` | Threat prediction horizon | 14-26 | 20-24 | Higher = sees threats earlier, plans better |
| `risk_weight_asteroid` | Asteroid avoidance weight | 0.8-3.0 | 1.5-2.5 | Higher = dodges asteroids more |
| `risk_weight_saucer` | Saucer body avoidance | 1.0-3.5 | 2.0-3.0 | Higher = stays away from saucers |
| `risk_weight_bullet` | Saucer bullet avoidance | 1.5-5.5 | 3.5-5.0 | **CRITICAL**: bullets are #1 killer |
| `survival_weight` | Overall survival multiplier | 1.0-4.0 | 2.5-3.5 | Higher = prioritizes not dying |
| `aggression_weight` | Attack value multiplier | 0.3-1.5 | 0.5-0.8 | Higher = values killing targets |
| `fire_reward` | Reward for well-aimed shots | 0.4-2.0 | 1.2-1.7 | **KEY**: higher = shoots more eagerly = clears threats faster |
| `shot_penalty` | Base cost of any shot | 0.3-1.5 | 0.5-0.9 | Higher = conserves ammo |
| `miss_fire_penalty` | Extra penalty for poor shots | 0.4-2.5 | 0.8-1.3 | Higher = refuses bad shots |
| `action_penalty` | Cost of any non-idle input | 0.005-0.03 | 0.007-0.012 | Higher = prefers stillness |
| `turn_penalty` | Cost of turning | 0.005-0.05 | 0.008-0.015 | Higher = turns less |
| `thrust_penalty` | Cost of thrusting | 0.005-0.04 | 0.008-0.012 | **Lower is better** — mobility saves lives |
| `center_weight` | Preference for screen center | 0.1-1.2 | 0.6-1.0 | **HIGH = GOOD**: center is safest position |
| `edge_penalty` | Avoidance of screen edges | 0.05-1.0 | 0.5-0.85 | **HIGH = GOOD**: edge deaths are common |
| `speed_soft_cap` | Speed above which penalties apply | 2.5-5.5 | 3.5-4.5 | Lower = slower but more controllable |
| `fire_tolerance_bam` | Aim angle tolerance (BAM units) | 5-14 | 7-10 | Higher = fires with less precise aim |
| `fire_distance_px` | Distance bonus range | 150-450 | 250-360 | Higher = bonus for distant shots |
| `lurk_trigger_frames` | Frames without kills → lurk response | 180-350 | 230-280 | **Lower = reacts sooner to saucer spawns** |
| `lurk_aggression_boost` | Aggression multiplier during lurk | 1.0-2.5 | 1.5-2.0 | Higher = much more aggressive when idle |

### Parameter Interaction Rules (IMPORTANT)

These parameters are **coupled** and should be changed together or with awareness:

1. **Survival-Offense Balance** (THE key tradeoff):
   - `survival_weight × risk_weights` = defense force
   - `aggression_weight × fire_reward` = offense force
   - Both must be HIGH simultaneously for best scores. Pure defense = low score. Pure offense = early death.
   - Proven sweet spot: survival_weight ~3.2 + fire_reward ~1.5

2. **Lurk Pair** (always change together):
   - `lurk_trigger_frames` + `lurk_aggression_boost` are tightly coupled
   - Lower trigger + higher boost = faster reaction to saucer spawns
   - If trigger is too low (<200), bot overreacts. If too high (>320), saucers accumulate.

3. **Position Pair** (change together):
   - `center_weight` + `edge_penalty` control positioning
   - Both should be HIGH. Extreme edge penalty (0.7+) is one of the biggest single improvements.
   - codex-autopilot uses 2x edge penalty as its top tuning win.

4. **Fire Discipline Trio** (balanced):
   - `fire_reward` vs `shot_penalty` + `miss_fire_penalty`
   - High fire_reward + low penalties = spam shots (bad). High fire_reward + moderate penalties = eager but disciplined (good).
   - The fire_reward should be ~2x the shot_penalty.

5. **Control Penalties** (not critical):
   - `action_penalty`, `turn_penalty`, `thrust_penalty` — diminishing returns on tuning
   - Keep them low (0.008-0.013) to allow fluid movement. Thrust should be lowest (mobility saves lives).

6. **Incompatible Changes** (DO NOT combine):
   - Don't increase `aggression_weight` AND decrease `survival_weight` in the same iteration
   - Don't increase `fire_tolerance_bam` AND decrease `shot_penalty` simultaneously (double-loosening)
   - Don't lower `speed_soft_cap` AND increase `thrust_penalty` (freezes the bot)

### Reference Scores from Other Bots

These are SearchBots in the same codebase for reference:

| Bot | avg_score | max_score | Strategy |
|-----|-----------|-----------|----------|
| omega-marathon | 10,418 | 19,920 | Defensive, cautious |
| omega-lurk-breaker | ~12K | ~25K | Anti-lurk, moderate aggression |
| omega-ace | ~15K | ~30K | Shot discipline striker |
| omega-alltime-hunter | ~20K | ~50K | Peak-score hunter, very aggressive |
| omega-supernova | ~22K | ~60K | Extreme aggression, saucer farming |

External bots (different codebase, different evaluation function, but same game):
| Bot | avg_score | max_score | Key Insight |
|-----|-----------|-----------|-------------|
| claude-autopilot marathon | 35K | 137K | survival=3.5, fire_reward=1.69, bullet=5.0 |
| codex-autopilot champion | 67K | 226K | extreme edge penalty, conservative fire, high mobility |

## Evaluation
- 12 fixed seeds (seeds.txt) for consistent comparison
- Max 108,000 frames (30 min at 60 fps) per run
- Score objective: `final_score * 1.0 + frame_count * 0.08 + final_lives * 120.0`
- omega-marathon runs alongside as reference baseline
- Improvement = higher avg_score across all 12 seeds
- **NEUTRAL** = within 2% of best_avg_score (don't revert, but don't celebrate)

## Benchmark Commands
```bash
# Standard evaluation benchmark
cargo run --release -- benchmark \
  --bots evolve-candidate,omega-marathon \
  --seed-file ralph-autopilot/seeds.txt \
  --max-frames 108000 \
  --objective score \
  --save-top 3 \
  --jobs 8 \
  --out-dir ralph-autopilot/scores/iteration-NNN

# Deep death analysis on a specific seed (USE THIS on worst seeds!)
cargo run --release -- codex-intel-run \
  --bot evolve-candidate \
  --seed 0xDEADBEEF \
  --max-frames 108000 \
  --output ralph-autopilot/scores/iteration-NNN/intel-DEADBEEF.json
```

## Strategy Tips
- Start with small changes (5-15% parameter adjustments)
- If 3+ consecutive regressions, try a different direction entirely
- **Run death analysis** (`codex-intel-run`) on the 2 worst seeds every iteration
- Don't overfit to one seed — check all 12 in the per-seed table
- Record EVERYTHING — future iterations depend on your notes
- When stuck at a local optimum, try **coordinated changes** (2-3 related params together)
- The biggest gains come from: bullet avoidance, edge penalty, fire reward, lurk timing
- **Always include a per-seed score table** in your iteration report

## Convergence Rules
- If 3+ consecutive regressions → try a fundamentally different parameter or approach
- If 5+ consecutive regressions/neutrals → pivot to coordinated multi-param changes
- If avg_score plateaus within 3% for 5 iterations → the config is at a local optimum; try moving 2-3 params simultaneously by 15-25%
- If a single seed regresses >30% while others improve → that seed has different dynamics; don't chase it at the expense of others
