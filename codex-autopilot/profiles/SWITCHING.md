# Profile switching shortcuts

- `champion-stable-719970.json`: stronger general/stress behavior.
- `champion-peak-782490.json`: first jackpot peak profile.
- `champion-peak-791480.json`: current top-score peak profile.
- `champion-peak-809220.json`: late-game pressure variant, current all-time peak.

To activate any profile:

```bash
cp codex-autopilot/profiles/<profile>.json codex-autopilot/profiles/champion.json
cp codex-autopilot/profiles/<profile>.json rust-autopilot/codex-/state/adaptive-profile.json
```
