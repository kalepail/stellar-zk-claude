# Profile switching

This lab is on an AST3 clean baseline.

- `base.json`: neutral profile.
- `champion.json`: current working profile (starts equal to `base.json` after resets).

To reset champion to baseline:

```bash
cp autopilot/codex-tuner/profiles/base.json autopilot/codex-tuner/profiles/champion.json
```

To install champion for runtime evaluation:

```bash
cp autopilot/codex-tuner/profiles/champion.json autopilot/codex-/state/adaptive-profile.json
```

Add archived `champion-*.json` variants only after they are re-benchmarked and validated under current rules.
