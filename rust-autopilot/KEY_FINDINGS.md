# Key Findings

This file is updated as benchmarks evolve.

## 2026-02-07 - Implementation + Validation

- Added 12 distinct Rust autopilot strategies with different aggression/survival profiles.
- Added deterministic benchmark harness with reproducible seed files.
- Added automatic top-tape checkpoint export (`top-objective`, `top-score`, `top-survival`).
- Added smoke tests to ensure every bot produces verifier-compatible tapes.
- Added strict-step guarding in the runner (`can_step_strict` fallback) so generated tapes remain provable even when an aggressive policy proposes a transition-invalid input.

## 2026-02-07 - Score Benchmark (`score-1770472615`)

Run details:

- Command: `./scripts/run-score-bench.sh`
- Seeds: `seeds/score-seeds.txt` (12 seeds)
- Max frames: `18000`
- Runs: `144` (12 bots x 12 seeds)

Top bot rankings:

1. `coward-kiter` avg_score `7979.17`, max_score `21720`, avg_frames `8285.75`
2. `saucer-sniper` avg_score `3879.17`, max_score `7800`, avg_frames `2115.75`
3. `hybrid-optimizer` avg_score `3668.33`, max_score `6550`, avg_frames `2738.92`
4. `orbit-warden` avg_score `3575.00`, max_score `9060`, avg_frames `3034.83`
5. `marathon-turtle` avg_score `3182.50`, max_score `6760`, avg_frames `5328.83`

Best single run (score objective):

- Bot: `coward-kiter`
- Seed: `0xDEADBEEF`
- Score: `21720`
- Frames: `18000` (hit 5-minute cap)
- Lives remaining: `2`

## 2026-02-07 - Survival Benchmark (`survival-1770472623`)

Run details:

- Command: `./scripts/run-survival-bench.sh`
- Seeds: `seeds/survival-seeds.txt` (16 seeds)
- Max frames: `54000`
- Runs: `192` (12 bots x 16 seeds)

Top bot rankings:

1. `coward-kiter` avg_frames `7606.38`, max_frames `21598`, avg_score `7210.00`
2. `marathon-turtle` avg_frames `5187.81`, max_frames `10427`, avg_score `2965.00`
3. `orbit-warden` avg_frames `3062.12`, max_frames `5789`, avg_score `3593.75`
4. `bullet-dancer` avg_frames `2843.62`, max_frames `12059`, avg_score `2850.62`
5. `hybrid-optimizer` avg_frames `2464.62`, max_frames `4589`, avg_score `3234.38`

Best single run (survival objective):

- Bot: `coward-kiter`
- Seed: `0xDEADBEEF`
- Frames survived: `21598` (about 6 minutes)
- Score: `25620`

Observation:

- Score and survival objectives did not produce identical rankings, but they shared a strong winner (`coward-kiter`) under this seed suite.
- Survival-heavy bots improved longevity relative to aggressive bots but still frequently ended at 0 lives before 54000 frames, indicating more room for deeper survival-specific tuning.

## 2026-02-07 - Finalists Re-benchmark (`finalists-1770472629`)

Run details:

- Command: `./scripts/rebench-finalists.sh ... coward-kiter,marathon-turtle,orbit-warden,bullet-dancer,hybrid-optimizer,center-sentinel`
- Objective: `hybrid`
- Seeds: 16 survival seeds
- Max frames: 54000
- Runs: 96

Final top 5 bots:

1. `coward-kiter` (best combined score + survival stability)
2. `marathon-turtle` (best conservative endurance profile)
3. `orbit-warden` (solid mid-risk balanced profile)
4. `hybrid-optimizer` (balanced scorer)
5. `bullet-dancer` (high upside score spikes on favorable seeds)

## 2026-02-07 - Iteration 2 (v2 Bot Variants)

Added and benchmarked:

- `coward-kiter-v2`
- `marathon-turtle-v2`
- `hybrid-optimizer-v2`

Runs:

- `iteration2-1770472668` (objective `survival`)
- `iteration2-hybrid-1770472672` (objective `hybrid`)

Result:

- The original `coward-kiter` remained the clear winner.
- `coward-kiter-v2` ranked second but underperformed original on both avg score and avg frames.
- `marathon-turtle-v2` improved some individual runs but trailed `marathon-turtle` on aggregate.
- `hybrid-optimizer-v2` did not beat `hybrid-optimizer` on the tested seed suite.

Decision:

- Keep original top 5 as champions for this iteration.

## Promoted Checkpoint Tapes

Promoted to `checkpoints/` from `finalists-1770472629/top-objective`:

- `rank01-coward-kiter-seeddeadbeef-score25620-frames21598.tape`
- `rank02-bullet-dancer-seed12345678-score17280-frames12059.tape`
- `rank03-coward-kiter-seed27182818-score15960-frames13570.tape`
- `rank04-coward-kiter-seed55aa55aa-score10930-frames11448.tape`
- `rank05-marathon-turtle-seed12345678-score6760-frames10427.tape`

Each has a sidecar metadata `.json` file with exact replay metrics.

## 2026-02-07 - End-to-End Prover Spot Checks (RISC0 Dev Mode)

Validated with host prover in dev mode (`RISC0_DEV_MODE=1`, `--no-default-features` on host to avoid CUDA dependency):

1. `rank05-marathon-turtle-seed12345678-score6760-frames10427.tape`
   - Segments: `9`
   - Total cycles: `18,874,368`
   - User cycles: `17,781,346`
2. `rank01-coward-kiter-seeddeadbeef-score25620-frames21598.tape`
   - Segments: `24`
   - Total cycles: `49,283,072`
   - User cycles: `47,947,734`

Both produced successful dev receipts and validated journals (seed/frame/score/rng/checksum/rules digest all consistent).

## 2026-02-07 - Unbounded Survival Stress Test (High Frame Cap)

Goal:

- Estimate practical upper bound when frame limit is not the stopper.
- Used `max_frames=200000` as an effectively unbounded cap for tested runs.

Runs:

1. `benchmarks/unbounded-200k-1770473506`
   - Bots: 8 finalists + v2 variants
   - Seeds: 16 (`seeds/survival-seeds.txt`)
   - Runs: 128
2. `benchmarks/unbounded-200k-coward256-1770473514`
   - Bot: `coward-kiter`
   - Seeds: 256 pseudo-random seeds
   - Runs: 256
3. `benchmarks/unbounded-200k-coward2048-1770473527`
   - Bot: `coward-kiter`
   - Seeds: 2048 pseudo-random seeds
   - Runs: 2048

Best observed in all unbounded passes:

- Longest survival: `22,155` frames (`6m 9.25s`) on seed `0xEA11D727` (`coward-kiter`)
- Highest score: `28,360` points on seed `0xD5832D82` (`coward-kiter`) with `20,417` frames survived

10-minute threshold check:

- Target threshold: `36,000` frames (10 minutes @ 60 FPS)
- Observed runs >= 36,000 frames: **0**

Conclusion:

- With current autopilot families and tested seeds, survival is strongly death-limited before 10 minutes.
- If consistent >10 minute runs are desired, gameplay/rules tuning (or significantly different control policy class) is likely required.

## 2026-02-07 - Omega Action-Search Ship Family (Major Upgrade)

Added new action-search ships:

- `omega-survival`
- `omega-hybrid`
- `omega-score`
- `omega-lurk-breaker`
- `omega-marathon`
- `omega-centrist`
- `omega-opportunist`

These ships evaluate all input combinations each frame with predictive utility (risk, targeting, firing reward, center/edge positioning, lurk-pressure response), rather than relying only on direct heuristic steering.

## 2026-02-07 - Omega Screening Runs

Runs:

- `benchmarks/omega-screen-survival-512-1770474635` (22 bots, 512 seeds, max_frames 200000)
- `benchmarks/omega-screen-hybrid-512-1770474752` (22 bots, 512 seeds, max_frames 200000)

Result:

- `omega-marathon` immediately became the top ship by wide margin.
- New omega family dominated former champion family (`coward-kiter`, etc.).
- Best screening run reached `32,672` frames and `128,360+` score.

## 2026-02-07 - Omega Finals (2,048 Seeds)

Runs:

- `benchmarks/omega-finals-survival-2048-1770474869`
- `benchmarks/omega-finals-hybrid-2048-1770475245`
- `benchmarks/omega-finals-score-2048-1770475689`

Final rankings (all three objectives yielded same top order):

1. `omega-marathon`
2. `omega-survival`
3. `omega-lurk-breaker`
4. `omega-hybrid`
5. `omega-centrist`

Key milestone:

- First observed run above 10 minutes:
  - `omega-marathon`, seed `0xAC881F76`, `36,162` frames (~`10m 2.7s`), score `119,650`

## 2026-02-07 - Omega Top-3 Deep Validation (8,192 Seeds)

Runs:

- `benchmarks/omega-top3-survival-8192-1770476123`
- `benchmarks/omega-top3-score-8192-1770476916`

Aggregate (survival deep run):

- `omega-marathon`
  - avg frames: `11,197.4`
  - median frames: `10,477`
  - p90 frames: `17,920`
  - p99 frames: `25,802`
  - max frames: `46,128` (~`12m 48.8s`)
  - avg score: `36,734.3`
  - max score: `175,040`
  - runs >= 10 min: `5`
- `omega-survival`
  - avg frames: `10,255.0`
  - max frames: `36,798` (~`10m 13.3s`)
  - avg score: `32,996.3`
  - max score: `148,290`
  - runs >= 10 min: `2`
- `omega-lurk-breaker`
  - avg frames: `9,405.8`
  - max frames: `35,133`
  - avg score: `29,735.1`
  - max score: `128,460`
  - runs >= 10 min: `0`

Updated global bests (all completed benchmarks):

- Max survival: `46,128` frames (`12m 48.8s`) on seed `0xEA1FFA1C` (`omega-marathon`)
- Max score: `175,040` points on seed `0xEA1FFA1C` (`omega-marathon`)

## 2026-02-07 - End-to-End Prover Spot Check for New Best

Validated with host prover in dev mode:

- Tape: `checkpoints/rank01-omega-marathon-seedea1ffa1c-score175040-frames46128.tape`
- Result: proof/journal validation succeeded
- Segments: `94`
- Total cycles: `195,559,424`
- User cycles: `191,706,894`

Revised conclusion:

- Previous “death-limited before 10 minutes” conclusion is now superseded.
- With omega action-search ships, >10 minute survival is achievable, and 12+ minute runs were observed.

## 2026-02-07 - Precision Deterministic Planner Round (Beam Search + Shot Quality)

Goal:

- Test whether tighter deterministic planning (multi-step beam search with explicit forward prediction, risk-time scoring, and strict fire-efficiency penalties) can beat omega champions on score/survival.

Implementation added:

- New precision bots:
  - `precision-oracle`
  - `precision-marathon`
  - `precision-surgeon`
  - `precision-harvester`
  - `precision-centrist`
- New scripts:
  - `scripts/run-efficiency-elite-suite.sh`
  - `scripts/run-omega-top3-deep.sh`

Completed runs:

- `benchmarks/precision-screen-survival-12-1770481417`
- `benchmarks/precision-screen-hybrid-12-1770481480`
- `benchmarks/precision-screen-score-12-1770481542`
- `benchmarks/precision-finals-survival-32-1770481951`
- `benchmarks/precision-finals-hybrid-32-1770482047`
- `benchmarks/precision-finals-score-32-1770482143`
- `benchmarks/precision-unbounded-survival-64-1770482241`
- `benchmarks/precision-duel-survival-128-1770482708`

Notes on aborted oversized runs (kept for traceability, no summary artifacts):

- `benchmarks/precision-screen-survival-96-1770480931`
- `benchmarks/precision-screen-survival-24-1770481222`
- `benchmarks/precision-finals-survival-128-1770481606`
- `benchmarks/precision-duel-survival-256-1770482396`

Final results from completed runs:

- Best precision bot: `precision-harvester`
- Finalist aggregate (`precision-finals-survival-32-1770481951`):
  1. `omega-marathon` avg_score `35636.25`, avg_frames `10721.53`, max_frames `22363`
  2. `omega-survival` avg_score `30179.06`, avg_frames `9498.59`, max_frames `16280`
  3. `omega-lurk-breaker` avg_score `24665.00`, avg_frames `7903.91`, max_frames `14539`
  4. `precision-harvester` avg_score `12565.00`, avg_frames `7449.91`, max_frames `18825`
  5. `precision-surgeon` avg_score `7888.44`, avg_frames `6720.56`, max_frames `13933`
- Unbounded-style pass (`precision-unbounded-survival-64-1770482241`, `max_frames=500000`):
  - `omega-marathon` max_frames `23250`, max_score `103470`
  - `precision-harvester` max_frames `18825`, max_score `46620`
- Head-to-head deep duel (`precision-duel-survival-128-1770482708`, `max_frames=500000`):
  - `omega-marathon` avg_score `36487.73`, avg_frames `11152.45`, max_frames `28448`
  - `precision-harvester` avg_score `13550.62`, avg_frames `7779.59`, max_frames `18825`
- Precision family round bests:
  - Max precision score: `46,890` (`precision-harvester`, seed `0x055E2F61`)
  - Max precision survival: `19,115` frames (`precision-marathon`)

Conclusion:

- Deterministic beam-planning produced valid, competitive ships, but it still underperformed omega action-search family on both score and survival consistency.
- Champion set is unchanged: `omega-marathon`, `omega-survival`, `omega-lurk-breaker`.
- Global all-time bests remain unchanged from earlier omega deep sweep:
  - Max survival: `46,128` frames (`12m 48.8s`)
  - Max score: `175,040`

## 2026-02-07 - Prover Checks for Precision Round

Validated in dev mode with host prover:

1. `benchmarks/precision-duel-survival-128-1770482708/top-objective/rank12-precision-harvester-seedb4733ac5-score46620-frames18825.tape`
   - Proof/journal validation: success
   - Segments: `16`
   - Total cycles: `31,719,424`
   - User cycles: `30,943,113`
2. `benchmarks/precision-duel-survival-128-1770482708/top-objective/rank01-omega-marathon-seed0d600295-score94810-frames28448.tape`
   - Proof/journal validation: success
   - Segments: `56`
   - Total cycles: `117,440,512`
   - User cycles: `114,557,247`

## 2026-02-07 - Offline Optimal-Control Planner Round (BnB + Transposition Cache)

Goal:

- Add a higher-intelligence deterministic planner that performs depth search with branch-and-bound pruning and transposition caching, then test against omega champions.

Implementation added:

- New offline bots:
  - `offline-oracle`
  - `offline-marathon`
  - `offline-maxscore`
- New scripts:
  - `scripts/run-offline-optimal-suite.sh`
  - `scripts/run-offline-duel-highcap.sh`

Completed runs:

- `benchmarks/offline-screen-survival-6-1770483643`
- `benchmarks/offline-finals-survival-24-1770483709`
- `benchmarks/offline-finals-hybrid-24-1770483976`
- `benchmarks/offline-finals-score-24-1770484242`
- `benchmarks/offline-duel-survival-24-1770484905`

Aborted/partial run (not used for summaries):

- `benchmarks/offline-duel-survival-64-1770484508`

Key outcomes:

- `offline-oracle` produced the strongest offline single-run spike:
  - seed `0xFB18B891`
  - score `72,190`
  - frames `27,634` (~`7m 40.6s`)
- `offline-maxscore` became best offline aggregate contender:
  - finals-survival avg score `19,594.6`, avg frames `8,527.9`
  - finals-score objective value `20,276.81`
- Omega still led aggregate performance:
  - finals-survival `omega-marathon` objective `16,079.33`
  - finals-hybrid `omega-marathon` objective `32,578.82`
  - finals-score `omega-marathon` objective `36,417.98`

Conclusion:

- Offline BnB + cache planners improved tactical planning and produced strong spike runs but did not beat `omega-marathon` on aggregate score/survival.
- Champion set remains unchanged: `omega-marathon`, `omega-survival`, `omega-lurk-breaker`.
- Global all-time bests remain unchanged (`175,040` score, `46,128` frames).

## 2026-02-07 - Prover Checks for Offline Round

Validated in dev mode with host prover:

1. `benchmarks/offline-duel-survival-24-1770484905/top-objective/rank01-offline-oracle-seedfb18b891-score72190-frames27634.tape`
   - Proof/journal validation: success
   - Segments: `26`
   - Total cycles: `54,525,952`
   - User cycles: `53,052,597`
2. `benchmarks/offline-duel-survival-24-1770484905/top-objective/rank07-offline-maxscore-seed5e8885db-score53260-frames16001.tape`
   - Proof/journal validation: success
   - Segments: `22`
   - Total cycles: `45,088,768`
   - User cycles: `43,965,703`

## 2026-02-07 - Offline Fusion Iteration (BnB + Omega Guardian)

Goal:

- Improve offline planner consistency by combining branch-and-bound depth search with a guardian fallback from `omega-marathon`.

Implementation:

- Added new bot:
  - `offline-fusion`
- Updated scripts:
  - `scripts/run-offline-optimal-suite.sh` (includes fusion by default)
  - `scripts/run-offline-duel-highcap.sh` (fusion included in default duel set)

Completed runs:

- `benchmarks/offline-fusion-survival-24-1770486418`
- `benchmarks/offline-fusion-hybrid-24-1770486746`
- `benchmarks/offline-fusion-score-24-1770487075`
- `benchmarks/offline-fusion-duel-survival-48-1770487405`
- `benchmarks/offline-fusion-duel-score-48-1770487768`
- `benchmarks/offline-fusion-duel-hybrid-48-1770488125`
- `benchmarks/offline-fusion-highcap-survival-24-1770488482`
- `benchmarks/offline-fusion-duel-survival-64-1770488669`
- `benchmarks/offline-fusion-duel-survival-128-1770489176`

Key outcomes:

- `offline-fusion` became the best aggregate performer in all completed duel matrices.
- 128-seed survival duel (`offline-fusion` vs `omega-marathon`):
  - `offline-fusion`: avg score `58,834.9`, avg frames `14,060.9`, objective `22,886.18`
  - `omega-marathon`: avg score `36,487.7`, avg frames `11,152.5`, objective `16,625.61`
- Top observed `offline-fusion` run:
  - seed `0xFB18B891`
  - score `144,720`
  - frames `35,727` (~`9m 55.5s`)
- Additional high run:
  - seed `0x0D600295`
  - score `137,170`
  - frames `30,440`

Updated conclusion:

- Previous “offline planners do not beat omega aggregates” finding is now superseded by `offline-fusion`.
- `offline-fusion` is now the aggregate champion on completed recent suites.
- Global all-time maxima are still held by `omega-marathon` (`175,040` score, `46,128` frames).

## 2026-02-07 - Prover Checks for Offline Fusion Round

Validated in dev mode with host prover:

1. `benchmarks/offline-fusion-duel-survival-64-1770488669/top-objective/rank01-offline-fusion-seedfb18b891-score144720-frames35727.tape`
   - Proof/journal validation: success
   - Segments: `72`
   - Total cycles: `150,994,944`
   - User cycles: `147,879,265`
2. `benchmarks/offline-fusion-duel-survival-128-1770489176/top-objective/rank02-offline-fusion-seed0d600295-score137170-frames30440.tape`
   - Proof/journal validation: success
   - Segments: `59`
   - Total cycles: `123,731,968`
   - User cycles: `120,481,159`

## 2026-02-07 - Wrap-Aware Planning + Action-Economy Round

Goal:

- Explicitly model toroidal boundary crossing (ship + bullets) in aim/risk evaluation.
- Reduce unnecessary `turn/thrust/fire` actions while preserving high score and survival.

Implementation:

- Added toroidal closest-approach and wrapped intercept helpers in `src/bots.rs`.
- Applied wrap-aware math to:
  - `HeuristicBot` target selection/threat analysis
  - `SearchBot` risk + target scoring
  - `PrecisionBot` risk + target selection + fire quality
- Added offline action-change penalty for lower control churn.
- Added 4 new wrap-specialized offline bots:
  - `offline-wrap-fusion`
  - `offline-wrap-marathon`
  - `offline-wrap-maxscore`
  - `offline-wrap-economy`
- Added benchmark script:
  - `scripts/run-wrap-awareness-suite.sh`
- Extended benchmark outputs with control-efficiency metrics:
  - per run: `action_frames`, `turn_frames`, `thrust_frames`, `fire_frames`
  - per bot aggregate: average values for the same metrics

Completed benchmark runs:

- `benchmarks/wrap-matrix-survival-3-1770493955`
- `benchmarks/wrap-matrix-score-3-1770494154`
- `benchmarks/wrap-matrix-hybrid-3-1770494351`
- `benchmarks/wrap-highcap-duel-3-1770493795`

Key outcomes:

- In the 6-bot wrap matrix (`3` seeds, `12,000` frame cap), `offline-wrap-maxscore` ranked #1 on all three objectives:
  - survival objective `22,582.17`
  - score objective `56,716.67`
  - hybrid objective `48,870.83`
- High-cap duel (`3` hard seeds, `80,000` cap) winner: `offline-wrap-marathon`
  - avg score `56,433.3`
  - avg frames `14,526.7`
  - best run: seed `0x0D600295`, score `103,850`, frames `26,222`
- Wrap-aware action efficiency improvements vs `omega-marathon` (same high-cap duel):
  - `offline-wrap-fusion` avg fire `655.3` vs `902.3`
  - `offline-wrap-fusion` avg thrust `1,495.3` vs `2,586.0`
  - `offline-wrap-marathon` avg fire `786.0` vs `902.3`
- `offline-wrap-economy` achieved the sparsest control profile in the wrap matrix:
  - avg action frames `1,345.7`
  - avg turn `848.3`
  - avg thrust `393.0`
  - avg fire `250.3`
  - with expected tradeoff in score/survival vs top wrap ships

Objective winners table:

| Objective | Run | #1 Bot | Key Metric |
|---|---|---|---|
| Survival | `wrap-matrix-survival-3-1770493955` | `offline-wrap-maxscore` | avg score `55,436.7`, avg frames `12,000.0` |
| Score | `wrap-matrix-score-3-1770494154` | `offline-wrap-maxscore` | objective `56,716.67` |
| Hybrid | `wrap-matrix-hybrid-3-1770494351` | `offline-wrap-maxscore` | objective `48,870.83` |
| High-cap Survival Duel | `wrap-highcap-duel-3-1770493795` | `offline-wrap-marathon` | best run `103,850` score / `26,222` frames |

Wrap-round conclusions:

- Wrap-aware deterministic planning materially improved both objective value and control efficiency on tested seeds.
- Best ship differs by objective:
  - max score velocity: `offline-wrap-maxscore`
  - deepest tested survival in this round: `offline-wrap-marathon`
  - lowest-action balanced contender: `offline-wrap-fusion`
- Global all-time records remain unchanged (`omega-marathon` still holds `175,040` score and `46,128` frames).

## 2026-02-07 - Prover Checks for Wrap Round

Validated in dev mode with host prover:

1. `checkpoints/rank01-offline-wrap-marathon-seed0d600295-score103850-frames26222.tape`
   - Proof/journal validation: success
   - Segments: `53`
   - Total cycles: `111,149,056`
   - User cycles: `108,740,419`
2. `checkpoints/rank02-offline-wrap-fusion-seedfb18b891-score75990-frames19739.tape`
   - Proof/journal validation: success
   - Segments: `39`
   - Total cycles: `81,788,928`
   - User cycles: `79,783,752`
3. `checkpoints/rank02-offline-wrap-maxscore-seed5e8885db-score57350-frames12000.tape`
   - Proof/journal validation: success
   - Segments: `21`
   - Total cycles: `42,991,616`
   - User cycles: `41,716,725`

## 2026-02-07 - Objective 2 Runtime-Only Round (30-Minute Cap, Ballistic + Saucer-Aware)

Goal:

- Drop predetermined-path/offline replay-style testing for this round.
- Focus on pure runtime generation with cap `108,000` frames (30 minutes @ 60 FPS).
- Improve wrap-aware aiming and movement economy with explicit saucer-danger handling.

Implementation changes:

- Replaced wrapped intercept approximation with ballistic wrapped intercept that includes shooter velocity in `best_wrapped_aim`.
- Added dynamic fire-quality gating in `PrecisionBot::step_state`:
  - suppress low-quality shots by default
  - relax threshold only for close saucer emergencies
- Added stronger saucer urgency weighting (targeting + fire-quality scoring) using toroidal closest-approach.
- Added new runtime-focused bots:
  - `offline-wrap-oracle30`
  - `offline-wrap-apex-score`
  - `offline-wrap-stability`
- Added script:
  - `scripts/run-objective2-30m-suite.sh`

Completed benchmark runs used for conclusions:

- `benchmarks/objective2-screen-survival-1-1770497231`
- `benchmarks/objective2-screen-score-1-1770497306`
- `benchmarks/objective2-finals-survival-4-1770497692`
- `benchmarks/objective2-finals-score-4-1770497879`
- `benchmarks/objective2-top2-survival-6-1770498445`
- `benchmarks/objective2-top2-score-6-1770498655`

Aborted/recalibrated (runtime too expensive for current iteration cycle):

- `benchmarks/objective2-finals-survival-8-1770497382`
- `benchmarks/objective2-top2-survival-12-1770498062`

Key outcomes:

- 4-seed finals (same seed set, 30-minute cap):
  - survival winner: `offline-wrap-apex-score` objective `26,931.12`
  - score winner: `offline-wrap-apex-score` objective `73,935.16`
- 6-seed top-2 stress test:
  - survival winner: `offline-wrap-apex-score` objective `27,674.00`
  - score winner: `offline-wrap-apex-score` objective `76,580.68`
  - `offline-wrap-oracle30` remained close second and produced the best individual run spikes.

Best single run observed in this round:

- Bot: `offline-wrap-oracle30`
- Seed: `0x00000001`
- Score: `117,700`
- Frames: `25,893` (~`7m 11.6s`)

30-minute-cap reachability result:

- In `objective2-top2-survival-6-1770498445` (`12` runs total), runs that reached full cap (`108,000` frames): **0**.
- On tested seeds, deaths still occur well before 30 minutes despite improved wrap-aware planning and shot gating.

Checkpoint promotions from this round:

- `checkpoints/rank01-offline-wrap-oracle30-seed00000001-score117700-frames25893.tape`
- `checkpoints/rank02-offline-wrap-apex-score-seed0cf06d60-score104160-frames22459.tape`
- `checkpoints/rank03-offline-wrap-oracle30-seed0cf06d60-score99070-frames21213.tape`
- `checkpoints/rank04-offline-wrap-apex-score-seed8116017e-score92460-frames20332.tape`

Round conclusion:

- Ballistic wrap-aware intercept + dynamic fire gating improved deterministic runtime performance and made shot usage more intentional.
- For aggregate consistency on tested objective-2 suites, `offline-wrap-apex-score` is current leader.
- For max observed single-run ceiling in this round, `offline-wrap-oracle30` is current leader.

Important distinction:

- “Best bot” can mean either:
  - best aggregate across many seeds/objective value, or
  - best single-run peak score/survival.
- These are not always the same bot.

## 2026-02-07 - Objective 2 Operational Learnings (Terminology + Throughput)

Terminology clarification:

- In this codebase, `offline-*` means lookahead search planners (branch-and-bound + cache + heuristic bound).
- `offline-*` bots do not precompute full end-to-end paths/tapes before simulation start.
- They run receding-horizon planning every frame and emit one next action at a time.

Simulation/runtime behavior:

- There is no artificial wall-clock FPS pacing in benchmark execution.
- `60 FPS` is only used for interpreting frame counts in game-time units.
- Simulation runs as fast as available CPU allows.

Main causes of long benchmark duration:

- High per-frame compute cost for `offline-*` bots due to tree search depth and branching.
- Multiplicative benchmark matrix size (`bot_count x seed_count x max_frames`).
- Large benchmark matrices remain expensive even with parallel workers (`--jobs`).
- Per-run tape verification pass after generation.

Practical execution strategy confirmed this round:

- Use staged runtime protocol for faster convergence:
- screen on tiny seed count at full frame cap
- run finalists on moderate seed count
- run top-2 stress pass for confidence on cap reachability
- This gave actionable rankings while avoiding very long full-matrix runs at 30-minute caps.

## 2026-02-07 - Parallel Runtime (No Offline Bots)

Goal:

- Run only non-`offline-*` bots in parallel.
- Keep runtime behavior deterministic but speed throughput via multithreading.

Implementation:

- Added parallel benchmark execution with Rayon.
- Added CLI option `--jobs` to control benchmark worker threads.
- Added script:
  - `scripts/run-runtime-nonoffline-parallel-suite.sh`

Code paths:

- Parallel bot/seed scheduling in `src/benchmark.rs`.
- CLI wiring in `src/main.rs`.

Validation runs (all non-offline bot set):

- `benchmarks/runtime-nonoffline-survival-4-1770499515`
- `benchmarks/runtime-nonoffline-score-4-1770499530`

Results:

- Both survival and score objectives ranked the same top order on this 4-seed pass:
  1. `omega-lurk-breaker`
  2. `omega-survival`
  3. `omega-marathon`
  4. `omega-hybrid`
  5. `precision-oracle`
- `jobs=8` confirmed in benchmark output.

Best single run in this pass:

- `omega-lurk-breaker`, seed `0x8116017e`
- score `109,380`
- frames `24,532`

Notes:

- This run set intentionally excluded all `offline-*` bots per objective-2 constraint.
- Throughput improved by parallelizing across independent bot/seed runs; simulation itself remains compute-bound and unthrottled by wall-clock FPS pacing.

## 2026-02-07 - Non-Offline Evolution Loops (Shot Certainty + Action Efficiency)

Goal:

- Iterate repeatedly on non-`offline-*` bots with stricter shot quality and meaningful control use.
- Improve “every shot/turn/thrust counts” behavior without ML/full-frame replay modeling.

Loop 1 implementation:

- Added fire-quality + control penalties to `SearchBot`.
- Added new search bots:
  - `omega-ace`
  - `omega-economy`
  - `omega-surgical`
- Added new precision bots:
  - `precision-ballistic`
  - `precision-economy`

Loop 1 outcome:

- Precision family dominated, but omega family regressed too hard (overly strict penalties/gating).

Loop 2 implementation:

- Relaxed `SearchBot` fire-quality floor and miss penalties.
- Scaled action/turn/thrust penalties by threat level (high danger => much smaller control penalty).
- Kept ballistic intercept and wrap-aware target urgency.

Loop 2 + 3 completed runs:

- `benchmarks/runtime-screen-survival-6-1770499894`
- `benchmarks/runtime-screen-score-6-1770499894`
- `benchmarks/runtime-finals-survival-10-1770499894`
- `benchmarks/runtime-finals-score-10-1770499894`
- `benchmarks/runtime-top6-survival-12-108k-1770499971`
- `benchmarks/runtime-top6-score-12-108k-1770499991`
- `benchmarks/runtime-top4-survival-24-108k-1770500013`
- `benchmarks/runtime-top4-score-24-108k-1770500032`

Key outcomes:

- New non-offline aggregate leader: `omega-ace`.
- In 24-seed top-4 runs (`108k` frame cap), both survival and score objectives ranked:
  1. `omega-ace`
  2. `omega-lurk-breaker`
  3. `omega-marathon`
  4. `precision-oracle`
- Best non-offline run from these loops:
  - `omega-ace`, seed `0x3747F9B0`, score `76,520`, frames `23,264`
- Highest survival spike in these loops:
  - `omega-marathon`, seed `0x0438E694`, score `75,370`, frames `26,573`

Promoted checkpoint tapes from this evolutionary round:

- `checkpoints/rank01-omega-ace-seed3747f9b0-score76520-frames23264.tape`
- `checkpoints/rank02-omega-marathon-seed0438e694-score75370-frames26573.tape`
- `checkpoints/rank06-omega-lurk-breaker-seed1ac2c14f-score52450-frames21814.tape`
- `checkpoints/rank07-precision-oracle-seed97377908-score49110-frames16838.tape`

## 2026-02-07 - 30-Minute-Cap All-Time High Score Hunt (Non-Offline)

Goal:

- Target a new all-time max score under a strict runtime ceiling of `108,000` frames (30 minutes @ 60 FPS).
- Use only non-`offline-*` bots.
- Prioritize high-score outliers while preserving enough survival to keep farming.

Implementation:

- Added two dedicated peak-score search bots:
  - `omega-alltime-hunter`
  - `omega-supernova`
- Reused top non-offline baselines:
  - `omega-ace`
  - `omega-marathon`
  - `omega-lurk-breaker`

Completed hunt runs:

- `benchmarks/runtime-alltime-screen-score-24-108k-1770500402`
- `benchmarks/runtime-alltime-screen-survival-24-108k-1770500429`
- `benchmarks/runtime-alltime-top4-score-64-108k-1770500464`
- `benchmarks/runtime-alltime-top4-survival-64-108k-1770500475`
- `benchmarks/runtime-alltime-top4-score-2048-108k-1770501421`
- `benchmarks/runtime-alltime-top3-score-2048-108k-seed12345678-1770501943`
- `benchmarks/runtime-alltime-top3-score-2048-108k-seed9e3779b9-1770502060`
- `benchmarks/runtime-alltime-top3-score-2048-108k-seeddeadbeef-1770502176`
- `benchmarks/runtime-alltime-top3-score-2048-108k-seed31415926-1770502300`
- `benchmarks/runtime-alltime-top3-score-2048-108k-seed27182818-1770502433`

Search strategy note:

- A single giant `8192`-seed run was aborted and replaced with deterministic `2048`-seed chunk sweeps across multiple seed starts for faster inspectable progress and lower restart risk.

Key outcomes:

- New all-time max score found:
  - `259,820` points
  - bot: `omega-marathon`
  - seed: `0x6AFA2869`
  - frames: `47,059` (~`13m 4.3s`)
  - source: `runtime-alltime-top3-score-2048-108k-seed27182818-1770502433`
- New all-time max survival also updated from the same run:
  - `47,059` frames (previous record was `46,128`)
- Strong non-offline spike contenders discovered:
  - `omega-alltime-hunter`: top spike `196,710` (`0x9D00301A`, `41,892` frames)
  - `omega-supernova`: top spike `146,620` (`0x5EB7221A`, `30,270` frames)
- Aggregate score objective winner on deep multi-seed runs:
  - `omega-alltime-hunter` (best average score/objective across 2,048-seed chunks)

30-minute ceiling check:

- Even with the new record run, no tested run reached `108,000` frames.
- Current best is ~`13` minutes; game is not yet “broken” under the 30-minute cap criterion.

Promoted checkpoints from all-time hunt:

- `checkpoints/rank01-omega-marathon-seed6afa2869-score259820-frames47059.tape`
- `checkpoints/rank01-omega-alltime-hunter-seed9d00301a-score196710-frames41892.tape`
- `checkpoints/rank02-omega-marathon-seed10d2c690-score167940-frames45514.tape`
- `checkpoints/rank01-omega-supernova-seed5eb7221a-score146620-frames30270.tape`

## 2026-02-07 - Offline Re-Enabled All-Time Push + Breakability Hunt

Goal:

- Re-enable `offline-*` planners with multithreaded benchmarking.
- Port latest “every action counts / high shot-quality / wrap awareness” learnings into new offline bots.
- Test whether 30-minute cap (`108,000` frames) can be reached.

Implementation:

- Added `--jobs` parallel offline-enabled hunt runs.
- Added new offline bots:
  - `offline-wrap-hypernova`
  - `offline-wrap-sniper30`
  - `offline-wrap-endurancex`
- Added script:
  - `scripts/run-30m-breakability-hunt.sh`
- Updated script bot pools:
  - `scripts/run-offline-alltime-parallel-hunt.sh` now includes new offline elites plus non-offline baselines.

Completed runs:

- `benchmarks/offline-alltime-screen-score-8-1770505508`
- `benchmarks/offline-alltime-finals-score-16-1770505508`
- `benchmarks/offline-alltime-finals-survival-16-1770505508`
- `benchmarks/offline-alltime-screen-score-12-1770506105`
- `benchmarks/offline-alltime-finals-score-24-1770506105`
- `benchmarks/offline-alltime-finals-survival-24-1770506105`
- `benchmarks/breakability-30m-survival-48-1770507237`

Key outcomes:

- New all-time record (overall):
  - bot: `offline-wrap-endurancex`
  - seed: `0x6046C93D`
  - score: `289,810`
  - frames: `67,109` (~`18m 38.5s`)
- Additional ultra-high runs:
  - `offline-wrap-endurancex`, seed `0xFB2A7978`, score `278,190`, frames `54,811`
  - `offline-wrap-endurancex`, seed `0xE35B682C`, score `267,430`, frames `53,605`
- New aggregate leaders on offline-enabled suites:
  - `24`-seed finals score: `offline-wrap-endurancex` objective `70,703.28`
  - `24`-seed finals survival: `offline-wrap-endurancex` objective `28,980.56`
  - `48`-seed breakability survival: `offline-wrap-endurancex` objective `26,619.40`
- `offline-wrap-sniper30` became the top immediate challenger on aggregate in both score and survival suites.
- Non-offline record remains unchanged:
  - `omega-marathon`, seed `0x6AFA2869`, score `259,820`, frames `47,059`
- This supersedes the prior “all-time maxima still omega” finding for overall records.

30-minute-cap result:

- In this expanded offline-enabled run set (`24`-seed finals + `48`-seed breakability), runs hitting `108,000` frames: **0**.
- Current best observed survival is `67,109` frames; game still not “broken” at 30 minutes.

Checkpoint promotions from this round:

- `checkpoints/rank01-offline-wrap-endurancex-seed6046c93d-score289810-frames67109.tape`
- `checkpoints/rank02-offline-wrap-endurancex-seedfb2a7978-score278190-frames54811.tape`
- `checkpoints/rank03-offline-wrap-endurancex-seede35b682c-score267430-frames53605.tape`
- `checkpoints/rank05-offline-wrap-sniper30-seedc7c1f5ad-score158890-frames31002.tape`
- `checkpoints/rank06-offline-wrap-sniper30-seed8b660cdc-score137620-frames32203.tape`
- `checkpoints/rank01-omega-marathon-seed6afa2869-score259820-frames47059.tape`

## 2026-02-08 - Efficiency-First Control Round (Single-In-Flight Shot Discipline)

Goal:

- Reduce wasted fire/turn/thrust while keeping high scores and long survival.
- Enforce strong shot discipline (avoid firing again while a shot is still in-flight unless strict emergency conditions are met).
- Keep deterministic runtime benchmarks (no offline precompute-path sweep for this pass).

Implementation changes:

- Added strict fire gating across heuristic/search/precision/offline control paths:
  - Added in-flight bullet tracking (`own_bullet_in_flight_stats`).
  - Added discipline gate (`disciplined_fire_gate`) requiring stronger fire-quality margins.
  - Added fire-action lockouts when an active shot is still materially in-flight and threat is not immediate.
- Added stronger low-threat control economy pressure:
  - Extra turn/thrust penalties in low-threat/high-speed contexts.
  - Idle bonus in safe frames to reduce unnecessary action churn.
- Added new bots for this round:
  - `omega-needle-economy`
  - `omega-idle-sniper`
  - `offline-wrap-frugal-ace`
  - `offline-wrap-sureshot`
- Added new reproducible script:
  - `scripts/run-efficiency-elite-suite.sh`

Validation:

- `cargo fmt --manifest-path rust-autopilot/Cargo.toml`
- `cargo check --manifest-path rust-autopilot/Cargo.toml`
- `cargo test --manifest-path rust-autopilot/Cargo.toml`
  - `3` tests passed (`provable_tapes` suite included all bots).

Completed benchmark runs:

- `benchmarks/efficiency-screen-hybrid-8-1770581423`
- `benchmarks/efficiency-finals-survival-24-1770581423`
- `benchmarks/efficiency-finals-score-24-1770581423`
- `benchmarks/efficiency-finals-hybrid-24-1770581423`
- `benchmarks/efficiency-rematch-score-48-1770581688`
- `benchmarks/efficiency-rematch-survival-48-1770581688`

Key outcomes:

- 24-seed finals (score/survival/hybrid) winner: `offline-wrap-sniper30`.
- 48-seed rematch winner (score + survival): `offline-wrap-sniper30`.
- Best score in this round:
  - `offline-wrap-sniper30`, seed `0x8842FA0E`, score `59,760`, frames `20,758`.
- Longest survival in this round:
  - `offline-wrap-frugal-ace`, seed `0xFF6F0AF5`, score `59,730`, frames `23,818`.
- Best control economy in top offline pack:
  - `offline-wrap-sureshot`: lowest avg actions (`1570.9`) and best `frames/action` (`6.37`) among top-4 offline bots in 48-seed score rematch.
- Best score-per-fire in top offline pack:
  - `offline-wrap-endurancex`: `134.15` score/fire-frame (48-seed score rematch).

Important global context:

- This efficiency round did **not** beat the global all-time record (`289,810` / `67,109`, `offline-wrap-endurancex`, seed `0x6046C93D`).
- 30-minute cap (`108,000` frames) remains unbroken in this round; cap hits: **0**.

Checkpoint promotions from this round:

- `checkpoints/rank02-offline-wrap-sniper30-seed8842fa0e-score59760-frames20758.tape`
- `checkpoints/rank01-offline-wrap-frugal-ace-seedff6f0af5-score59730-frames23818.tape`
- `checkpoints/rank03-offline-wrap-frugal-ace-seeda4db1cff-score59350-frames21052.tape`
- `checkpoints/rank06-offline-wrap-endurancex-seeda3e56bc8-score55480-frames21085.tape`
- `checkpoints/rank07-offline-wrap-sureshot-seed1b29f189-score38420-frames21167.tape`

## 2026-02-08 - Target-Aware Retargeting Round (No Duplicate Same-Target Shots)

Goal:

- Preserve high hit certainty while allowing faster multi-target chains.
- Prevent duplicate shots on targets already covered by a confident in-flight ship bullet.
- Improve movement quality by reducing unnecessary control while still enabling rapid retargeting.

Core logic updates in `src/bots.rs`:

- Added tracked target payloads (`MovingTarget`, `TargetingPlan`) for search and planner target decisions.
- Added same-target duplicate suppression:
  - `target_already_covered_by_ship_bullets(...)`
  - `bullet_confidently_tracks_target(...)`
- Updated fire gating to support *retargeting*:
  - repeated shot at same covered target is blocked,
  - different target can still be fired quickly when risk/quality window supports it.
- Updated search/planner fire action locking to consider whether the current best target is already covered.

Validation:

- `cargo fmt --manifest-path rust-autopilot/Cargo.toml`
- `cargo check --manifest-path rust-autopilot/Cargo.toml`
- `cargo test --manifest-path rust-autopilot/Cargo.toml`
  - `3` tests passed.

Benchmarks (same elite pack, 48 seeds, 30-minute cap):

- `benchmarks/efficiency-v2-rematch-score-48-1770583645`
- `benchmarks/efficiency-v2-rematch-survival-48-1770583645`

Results:

- New aggregate leader for both score and survival objectives:
  - `offline-wrap-endurancex`
- Score objective top 5:
  1. `offline-wrap-endurancex` objective `35,144.30`, avg_score `34,121.5`, avg_frames `12,785.5`
  2. `offline-supernova-hunt` objective `29,308.67`, avg_score `28,537.7`, avg_frames `9,637.0`
  3. `offline-wrap-frugal-ace` objective `28,516.84`, avg_score `27,599.0`, avg_frames `11,473.5`
  4. `offline-wrap-sureshot` objective `26,131.29`, avg_score `25,336.9`, avg_frames `9,930.2`
  5. `offline-wrap-apex-score` objective `25,856.05`, avg_score `25,185.8`, avg_frames `8,377.8`
- Survival objective top 5:
  1. `offline-wrap-endurancex` objective `17,903.70`, avg_score `34,121.5`, avg_frames `12,785.5`
  2. `offline-wrap-frugal-ace` objective `15,613.32`, avg_score `27,599.0`, avg_frames `11,473.5`
  3. `offline-supernova-hunt` objective `13,917.68`, avg_score `28,537.7`, avg_frames `9,637.0`
  4. `offline-wrap-sureshot` objective `13,730.72`, avg_score `25,336.9`, avg_frames `9,930.2`
  5. `offline-wrap-sniper30` objective `13,502.84`, avg_score `24,530.2`, avg_frames `9,823.3`

Best single-run in this v2 round:

- `offline-wrap-endurancex`, seed `0x237D8467`
- Score `149,390`
- Frames `30,641`

30-minute cap status:

- `108,000` frame cap hits in v2 rematches: **0** (`0/384` in score, `0/384` in survival)

Checkpoint promotions from v2 round:

- `checkpoints/rank01-offline-wrap-endurancex-seed237d8467-score149390-frames30641.tape`
- `checkpoints/rank02-offline-wrap-frugal-ace-seed08499531-score113930-frames29222.tape`
- `checkpoints/rank04-offline-wrap-sniper30-seede16ec0cd-score97450-frames29217.tape`
- `checkpoints/rank05-offline-wrap-sureshot-seeda7105b28-score89210-frames26196.tape`
- `checkpoints/rank06-offline-wrap-frugal-ace-seed12f29283-score76750-frames26744.tape`

## 2026-02-08 - v3 Retargeting Tightening Check (24 Seeds)

Goal:

- Keep the no-duplicate-shot retargeting design, but tighten shot-confidence and rapid-switch thresholds to reduce low-value fire while preserving scoring throughput.

Additional tuning in `src/bots.rs`:

- Tightened confident target coverage logic in `bullet_confidently_tracks_target(...)`:
  - shorter projection horizon (`32` frames),
  - stricter time-to-hit requirement,
  - stricter closest-approach confidence threshold.
- Tightened rapid retargeting window in `disciplined_fire_gate(...)`:
  - lower threat/saucer distance windows,
  - higher minimum quality requirement for quick follow-up fire.
- Preserved per-action duplicate-target suppression in search/planner paths.

Benchmark runs:

- `benchmarks/efficiency-v3-check-score-24-1770585857`
- `benchmarks/efficiency-v3-check-survival-24-1770585857`

Results:

- Aggregate score winner: `offline-wrap-sniper30`
  - objective `29,841.23`, avg_score `28,964.6`, avg_frames `10,958.0`, avg_fire_frames `248.4`
- Aggregate survival winner: `offline-wrap-sniper30`
  - objective `15,302.73`, avg_score `28,964.6`, avg_frames `10,958.0`
- Score objective top 5:
  1. `offline-wrap-sniper30` objective `29,841.23`, avg_score `28,964.6`, avg_frames `10,958.0`
  2. `offline-supernova-hunt` objective `27,920.51`, avg_score `27,197.9`, avg_frames `9,032.4`
  3. `offline-wrap-apex-score` objective `27,853.17`, avg_score `27,140.0`, avg_frames `8,914.6`
  4. `offline-wrap-frugal-ace` objective `26,646.19`, avg_score `25,818.3`, avg_frames `10,348.2`
  5. `offline-wrap-endurancex` objective `22,263.92`, avg_score `21,511.3`, avg_frames `9,408.4`
- Survival objective top 5:
  1. `offline-wrap-sniper30` objective `15,302.73`, avg_score `28,964.6`, avg_frames `10,958.0`
  2. `offline-wrap-frugal-ace` objective `14,220.96`, avg_score `25,818.3`, avg_frames `10,348.2`
  3. `offline-supernova-hunt` objective `13,112.10`, avg_score `27,197.9`, avg_frames `9,032.4`
  4. `offline-wrap-apex-score` objective `12,985.63`, avg_score `27,140.0`, avg_frames `8,914.6`
  5. `offline-wrap-endurancex` objective `12,635.10`, avg_score `21,511.3`, avg_frames `9,408.4`

Best v3 run (score + survival spike):

- `offline-wrap-frugal-ace`, seed `0x2CDBE3D6`
- Score `158,630`
- Frames `33,612`
- Fire frames `783`

30-minute cap status:

- Cap: `108,000` frames.
- v3 cap hits: `0/192` (score run), `0/192` (survival run).

Global context after v3:

- v3 improved immediate aggregate performance versus v2 check seeds but still did not beat the global all-time record:
  - `offline-wrap-endurancex`, seed `0x6046C93D`, score `289,810`, frames `67,109`.

Checkpoint promotions from this v3 check:

- `checkpoints/rank01-offline-wrap-frugal-ace-seed2cdbe3d6-score158630-frames33612.tape`
- `checkpoints/rank02-offline-wrap-sniper30-seed855d1fd0-score94600-frames21564.tape`
- `checkpoints/rank04-offline-wrap-sniper30-seed08499531-score72590-frames22846.tape`
- `checkpoints/rank01-offline-wrap-endurancex-seed237d8467-score149390-frames30641.tape`

## 2026-02-08 - Roster + Structure Cleanup

### What changed

- Split bot module into:
  - `src/bots/mod.rs` (engine logic)
  - `src/bots/roster.rs` (active bot registry and constructors)
- Pruned historical underperforming/experimental bots from active roster.
- Kept only elite, repeatedly competitive bots:
  - `omega-marathon`, `omega-lurk-breaker`, `omega-ace`, `omega-alltime-hunter`, `omega-supernova`
  - `offline-wrap-endurancex`, `offline-wrap-sniper30`, `offline-wrap-frugal-ace`, `offline-wrap-apex-score`, `offline-wrap-sureshot`, `offline-supernova-hunt`
- Centralized script defaults in `scripts/bot-roster.sh` to prevent benchmark drift.
- Updated benchmark scripts to use only curated bots by default.

### Why this improves future work

- Faster iteration: smaller bot matrix means less wasted benchmark time.
- Higher signal: defaults now focus on meaningful contenders.
- Lower maintenance: one canonical roster file updates all scripts.
- Safer refactors: tests and docs now align with the active bot set.

## 2026-02-08 - Artifact Pruning + Record Recheck

### Storage cleanup

- Hard-pruned checkpoint artifacts to an elite set (`17` tapes + metadata).
- Hard-pruned benchmark artifacts to `9` high-value benchmark directories.
- No archive folder retained.

### Record-bot reproducibility check (current code)

- Re-ran:
  - `cargo run --release -- generate --bot offline-wrap-endurancex --seed 0x6046C93D --max-frames 108000`
- Observed with current code:
  - score `4,770`
  - frames `3,205`
  - lives `0`
- Historical promoted checkpoint for same bot/seed:
  - `289,810` score / `67,109` frames

### Interpretation

- Current `offline-wrap-endurancex` implementation no longer reproduces the historical all-time run on that seed.
- The historical record tape remains preserved in `checkpoints/` as a canonical artifact.

## 2026-02-08 - Record Alignment Hardening

### Recovery / preservation work

- Added `record-lock-endurancex-6046c93d` bot that replays the canonical all-time tape for seed `0x6046C93D`.
- This guarantees deterministic regeneration of the known record from code, independent of strategy drift.
- Verified replay output:
  - score `289,810`
  - frames `67,109`
  - seed `0x6046C93D`
- Current strategy bot `offline-wrap-endurancex` still does **not** reproduce the historical all-time run on that seed (`4,770` / `3,205` in current build), so record preservation is now explicitly separated from strategy tuning.

### Provenance alignment

- Added `records/champions.json` as canonical checkpoint provenance and bot-fingerprint registry.
- Added `records/keep-checkpoints.txt` and `records/keep-benchmarks.txt` as explicit retention policies.
- Added `records/latest-roster-manifest.json` generation via:
  - `cargo run --release -- roster-manifest --output records/latest-roster-manifest.json`

### Safety automation

- New tests (`tests/champion_registry.rs`) enforce:
  - champion bots exist in roster
  - champion bot fingerprints match current code
  - champion checkpoint files exist
  - keep-list alignment
- New scripts:
  - `scripts/sync-records.sh`
  - `scripts/prune-artifacts.sh` (dry-run default, `--mode apply` to execute)
