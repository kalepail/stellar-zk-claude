# Evolution Loop Guide

## What This Is
An automated progressive learning loop that iteratively improves the `evolve-candidate` SearchBot autopilot for a deterministic Asteroids game. Each iteration, a fresh Claude Code context analyzes previous results, makes ONE targeted improvement, benchmarks it, and records everything.

## File Layout
```
evolve/
├── evolve.sh                    # Outer bash loop (calls claude -p per iteration)
├── state.json                   # Iteration state, best scores, history
├── seeds.txt                    # Fixed 12-seed evaluation set
├── GUIDE.md                     # This file — reference for each iteration
├── journal/
│   ├── lessons.md               # Cumulative lessons (append-only insights)
│   ├── iteration-001.md         # Per-iteration reports
│   └── ...
├── designs/
│   ├── iteration-001.rs         # Snapshot of SearchConfig at each iteration
│   └── ...
└── scores/
    ├── iteration-001/           # Benchmark output (summary.json, runs.csv, rankings.csv)
    └── ...
```

## The Bot Under Evolution
The `evolve-candidate` bot is a SearchBot defined in `src/bots/roster.rs` at the end of `search_bot_configs()`. It started as a clone of `omega-marathon`.

### SearchConfig Parameters

| Parameter | Description | Range | Effect |
|-----------|-------------|-------|--------|
| `lookahead_frames` | Threat prediction horizon | 10-25 | Higher = more cautious, sees threats earlier |
| `risk_weight_asteroid` | Asteroid avoidance weight | 0.8-2.5 | Higher = dodges asteroids more aggressively |
| `risk_weight_saucer` | Saucer body avoidance | 1.0-3.0 | Higher = stays away from saucers |
| `risk_weight_bullet` | Saucer bullet avoidance | 1.5-4.0 | CRITICAL: bullets are #1 killer |
| `survival_weight` | Overall survival multiplier on risk | 1.0-3.0 | Higher = prioritizes not dying over scoring |
| `aggression_weight` | Attack value multiplier | 0.3-1.5 | Higher = values killing targets more |
| `fire_reward` | Reward for well-aimed shots | 0.4-2.0 | Higher = shoots more eagerly |
| `shot_penalty` | Base cost of any shot | 0.3-1.5 | Higher = conserves ammo more |
| `miss_fire_penalty` | Extra penalty for poor shots | 0.4-2.5 | Higher = refuses bad shots |
| `action_penalty` | Cost of any non-idle input | 0.005-0.03 | Higher = prefers stillness |
| `turn_penalty` | Cost of turning | 0.005-0.05 | Higher = turns less |
| `thrust_penalty` | Cost of thrusting | 0.005-0.04 | Higher = thrusts less |
| `center_weight` | Preference for screen center | 0.1-0.8 | Higher = gravitates to center |
| `edge_penalty` | Avoidance of screen edges | 0.05-0.6 | Higher = avoids edges more |
| `speed_soft_cap` | Speed above which penalties apply | 2.5-6.0 | Lower = slower/more controllable |
| `fire_tolerance_bam` | Aim angle tolerance (BAM units) | 5-14 | Higher = fires with less precise aim |
| `fire_distance_px` | Distance bonus range | 150-450 | Higher = bonus for distant shots |
| `lurk_trigger_frames` | Frames without kills → lurk response | 180-400 | Lower = reacts to lurk sooner |
| `lurk_aggression_boost` | Aggression multiplier during lurk | 1.0-2.5 | Higher = much more aggressive when lurking |

### Parameter Interaction Guidelines
- **Survival vs Score tradeoff**: `survival_weight` * risk weights vs `aggression_weight` * `fire_reward`
- **Shot discipline**: `shot_penalty` + `miss_fire_penalty` control waste. `fire_reward` controls eagerness
- **Movement efficiency**: `action_penalty` + `turn_penalty` + `thrust_penalty` control jitter
- **Positioning**: `center_weight` + `edge_penalty` control where the ship prefers to be
- **Lurk response**: `lurk_trigger_frames` + `lurk_aggression_boost` control anti-lurk mechanic

## Evaluation Methodology
- 12 fixed seeds (evolve/seeds.txt) for consistent comparison
- Max 108,000 frames (30 minutes game time) per run
- Score objective: `final_score * 1.0 + frame_count * 0.08 + final_lives * 120.0`
- omega-marathon runs alongside as reference baseline
- Improvement = higher avg_score across all 12 seeds

## Benchmark Commands
```bash
# Standard evaluation benchmark
cargo run --release -- benchmark \
  --bots evolve-candidate,omega-marathon \
  --seed-file evolve/seeds.txt \
  --max-frames 108000 \
  --objective score \
  --save-top 3 \
  --jobs 8 \
  --out-dir evolve/scores/iteration-NNN

# Deep death analysis on a specific seed
cargo run --release -- codex-intel-run \
  --bot evolve-candidate \
  --seed 0xDEADBEEF \
  --max-frames 108000 \
  --output evolve/scores/iteration-NNN/intel-DEADBEEF.json
```

## Strategy Tips
- Start with small changes (5-15% parameter adjustments)
- If 3+ consecutive regressions, try a different direction entirely
- The biggest gains often come from the survival/aggression balance
- Death analysis (codex-intel-run) is expensive but reveals WHY the ship dies
- Don't overfit to one seed — check performance across all 12
- If you plateau on parameters, consider whether the algorithm itself needs changing
- Record EVERYTHING — future iterations depend on your notes
