# codex-tuner

Autopilot tuning lab focused on one goal: iteratively tune a high-scoring Codex ship without growing the core autopilot crate.

## What this lab does

- Tunes `codex-potential-adaptive` by mutating its adaptive profile scales.
- Learns from winning mutation directions (momentum) and increases exploration when progress stalls.
- Benchmarks each candidate on deterministic seeds.
- Promotes the best profile each iteration.
- Writes a champion profile you can keep iterating from.

## Layout

- `profiles/base.json`: starting profile.
- `profiles/champion.json`: current champion profile (AST3 baseline starts equal to `base.json`).
- `profiles/SWITCHING.md`: profile reset/activation shortcuts.
- `seeds/screen-seeds.txt`: iterative seed set.
- `seeds/validation-seeds.txt`: tougher validation seed set.
- `scripts/iterative-search.py`: core iterative tuner.
- `scripts/run-super-score-loop.sh`: one-command tune + validation flow.
- `runs/`: per-session artifacts and benchmark outputs (gitignored).

## Quick run

```bash
./autopilot/codex-tuner/scripts/run-super-score-loop.sh
```

Defaults:

- iterations: `6`
- candidates/iteration: `6`
- max frames: `108000` (30 minutes at 60 FPS)
- jobs: `8`
- selection metric: `score` (`objective`, `score`, or `insane`)
- install mode: `champion` (`champion` or `restore`)
- anchor mode: `core` (`core` or `all`)

Custom run:

```bash
./autopilot/codex-tuner/scripts/iterative-search.py \
  --iterations 8 \
  --candidates 8 \
  --max-frames 108000 \
  --selection-metric insane \
  --anchor-mode core \
  --install-mode champion \
  --jobs 8
```

Notes:

- The tuner uses the proven sim + verifier in `autopilot/`.
- It writes the active profile into `autopilot/codex-/state/adaptive-profile.json` during evaluation.
- `autopilot/codex-/` is local runtime state and should remain untracked.
- Only keep archived `champion-*.json` profiles if they were validated under the current ruleset.
