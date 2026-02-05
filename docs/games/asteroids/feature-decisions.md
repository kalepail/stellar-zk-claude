# Asteroids Feature Decisions

## 2026-02-05: Omit Hyperspace

Decision:
- Hyperspace is intentionally not part of this build.

Why:
- It introduces a high-variance escape mechanic that conflicts with the current deterministic gameplay goals.
- It increases rules/state surface area without improving the core loop of movement, aiming, and survival.
- Removing it simplifies controls, player onboarding, and implementation complexity.

Scope:
- No hyperspace control input.
- No hyperspace cooldown or teleport/fail logic.
- No hyperspace-specific VFX/SFX.
- No hyperspace references in UI copy or gameplay docs.
