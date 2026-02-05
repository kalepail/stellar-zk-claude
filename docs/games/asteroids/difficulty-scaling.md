# Difficulty Scaling for ZK-Optimized Game Length

## Motivation

ZK proof costs scale linearly with game duration. On RISC Zero (H100 GPU):

| Game Length | ~Cycles | Proving Time |
|-------------|---------|-------------|
| 2 min | 72M | ~65s |
| 5 min | 180M | ~2.7 min |
| 10 min | 360M | ~5.5 min (too expensive) |

Target: games that naturally end in **2-5 minutes** for skilled players, matching classic arcade session lengths (1-3 min average, ~5 min skilled).

## Problem

Before these changes, difficulty plateaued at wave 4:
- Asteroid count capped at 11 (reached at wave 4)
- Only 1 saucer ever on screen
- Saucer spawn timer fixed at 7-14s regardless of wave
- Saucer accuracy only scaled with score, not wave progression
- Asteroid speed fixed per size category

Skilled players (or the autopilot) could survive indefinitely by exploiting the plateau.

## Changes Implemented

### 1. Asteroid Count Cap: 11 -> 16

`Math.min(16, 4 + (wave - 1) * 2)` -- caps at wave 7 instead of wave 4.

| Wave | Before | After |
|------|--------|-------|
| 1 | 4 | 4 |
| 2 | 6 | 6 |
| 3 | 8 | 8 |
| 4 | 10 | 10 |
| 5 | 11 (cap) | 12 |
| 6 | 11 | 14 |
| 7+ | 11 | 16 (cap) |

### 2. Multiple Saucers (Wave-Scaled)

Max saucers on screen at once:
- Waves 1-3: 1
- Waves 4-6: 2
- Waves 7+: 3

Changed spawn check from `=== 0` to `< maxSaucers`.

### 3. Saucer Spawn Timer Scales With Wave

Multiplier: `max(0.4, 1 - (wave - 1) * 0.08)`

| Wave | Multiplier | Spawn Range |
|------|-----------|-------------|
| 1 | 1.0 | 7-14s |
| 4 | 0.76 | 5.3-10.6s |
| 6 | 0.60 | 4.2-8.4s |
| 8+ | 0.44 | 3.1-6.2s |

Applied in both `updateSaucers()` (ongoing) and `startNewGame()` (initial timer after `spawnWave()`).

### 4. Saucer Accuracy Scales With Wave

Added `waveAccuracyBonus = min(15, wave * 2)` subtracted from the small saucer's error angle.

Before: `errorDegrees = clamp(baseError - score/2500, 4, baseError)`
After: `errorDegrees = clamp(baseError - score/2500 - waveAccuracyBonus, 4, baseError)`

At wave 5 with score 0, small saucer error drops from 30 to 20 degrees. By wave 8, error is at the 4-degree floor even with zero score.

### 5. Asteroid Speed Scales With Wave

Multiplier: `1 + min(0.5, (wave - 1) * 0.06)`

| Wave | Multiplier | Large Speed Range |
|------|-----------|-------------------|
| 1 | 1.0x | 34-58 |
| 3 | 1.12x | 38-65 |
| 5 | 1.24x | 42-72 |
| 8 | 1.42x | 48-82 |
| 9+ | 1.50x (cap) | 51-87 |

### 6. Anti-Lurk Threshold: 8s -> 6s

`LURK_TIME_THRESHOLD` reduced from 8 to 6 seconds. Combined with multiple saucers and faster spawns, lurking becomes much harder.

## What Stayed the Same

- Starting lives (3)
- Extra life interval (10,000 pts)
- Asteroid split behavior and cap (27)
- Saucer bullet speed/lifetime
- Ship stats (thrust, drag, turn speed, bullet speed)

## Design Rationale

### Wave 1 is unchanged
All multipliers evaluate to 1.0x at wave 1. First wave experience is identical to before.

### Difficulty compounds
Each individual change is modest, but they compound. At wave 7+ a player faces:
- 16 asteroids (vs 11 before) moving 1.36-1.5x faster
- Up to 3 saucers spawning every 3-6s with near-perfect aim
- Anti-lurk kicks in after just 6s of inactivity

### Classic Asteroids comparison
The original 1979 arcade game was arguably harder:
- Saucer spawn timer was 2.4s dropping to 0.5s (ours is 3.1-6.2s at hardest)
- Average session was 1-3 minutes per quarter
- Only the "lurking exploit" (hiding in corner, farming saucers) allowed marathon sessions -- which our anti-lurk system prevents

### ZK-friendly outcome
A good player should survive 3-5 waves (2-4 minutes). This maps to 1-3 minute proving times, keeping costs reasonable for on-chain verification.
