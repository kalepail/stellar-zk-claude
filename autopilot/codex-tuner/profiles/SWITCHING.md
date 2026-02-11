# Profile switching shortcuts

- `champion-stable-719970.json`: stronger general/stress behavior.
- `champion-peak-782490.json`: first jackpot peak profile.
- `champion-peak-791480.json`: current top-score peak profile.
- `champion-peak-809220.json`: late-game pressure variant, current all-time peak.

To activate any profile:

```bash
cp autopilot/codex-tuner/profiles/<profile>.json autopilot/codex-tuner/profiles/champion.json
cp autopilot/codex-tuner/profiles/<profile>.json autopilot/codex-/state/adaptive-profile.json
```
