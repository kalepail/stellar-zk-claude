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
- `profiles/champion.json`: latest champion profile.
- `seeds/screen-seeds.txt`: iterative seed set.
- `seeds/validation-seeds.txt`: tougher validation seed set.
- `scripts/iterative-search.py`: core iterative tuner.
- `scripts/run-super-score-loop.sh`: one-command tune + validation flow.
- `runs/`: per-session artifacts and benchmark outputs.

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
- anchor mode: `all` (`all` or `core`)

Custom run:

```bash
./autopilot/codex-tuner/scripts/iterative-search.py \
  --iterations 8 \
  --candidates 8 \
  --max-frames 108000 \
  --selection-metric insane \
  --anchor-mode all \
  --install-mode champion \
  --jobs 8
```

Notes:

- The tuner uses the proven sim + verifier in `autopilot/`.
- It writes the active profile into `autopilot/codex-/state/adaptive-profile.json` during evaluation.
- Profiles in `autopilot/codex-tuner/profiles/` use the full adaptive schema (all scale keys), so anchors and blends behave consistently across runs.
