# Asteroids Original Ruleset Variance Audit (TS + zk)

## Objective
Compare the current TypeScript and zk (Rust) deterministic ruleset against original Atari Asteroids behavior, identify every material variance, and separate:
- TS vs zk parity issues
- modernized design choices that diverge from original arcade behavior

Date: 2026-02-09

## Scope and Baseline
- Current implementation reviewed:
  - `src/game/AsteroidsGame.ts`
  - `src/game/constants.ts`
  - `src/game/tape.ts`
  - `risc0-asteroids-verifier/asteroids-core/src/sim.rs`
  - `risc0-asteroids-verifier/asteroids-core/src/constants.rs`
  - `risc0-asteroids-verifier/asteroids-core/src/tape.rs`
- Existing project spec reviewed:
  - `docs/games/asteroids/01-GAME-SPEC.md`
- Original arcade baseline used for comparison:
  - 6502 disassembly + RAM/hardware maps (Computer Archeology and 6502disassembly rev4 corpus)

## TS vs zk Parity (Current Ruleset Internal Consistency)
No deterministic gameplay-rule drift was found between TS and zk for the audited logic.

Verification run results:
- `bun run scripts/verify-tape.ts test-fixtures/test-short.tape`: passed
- `bun run scripts/verify-tape.ts test-fixtures/test-medium.tape`: passed
- `cargo test -p asteroids-verifier-core --manifest-path risc0-asteroids-verifier/Cargo.toml`: all tests passed, including TS checkpoint parity fixtures

Interpretation:
- Core deterministic simulation ordering, scoring, wave progression, entity caps/limits as currently defined, and replay verification are aligned between TS and zk.
- Findings below are therefore primarily current-ruleset-vs-original variances, not TS-vs-zk bugs.

## Variance Matrix vs Original Asteroids

Legend:
- Severity: `Critical`, `High`, `Medium`, `Low`
- Confidence: `High`, `Medium` (where historical revision behavior is version-dependent)

| ID | Area | Current TS/zk Ruleset | Original Arcade Behavior | Variance | Severity | Confidence | Recommended Alignment Action |
|---|---|---|---|---|---|---|---|
| V-01 | Control surface | Input model is 4 bits: left/right/thrust/fire (`src/game/tape.ts:15`, `risc0-asteroids-verifier/asteroids-core/src/tape.rs:38`). No hyperspace action in sim. | Dedicated hyperspace control exists (`SWHYPER`), with explicit hyperspace state machine. | Hyperspace is removed entirely. | Critical | High | Add optional hyperspace bit + deterministic hyperspace logic path (including failure rules). |
| V-02 | Hyperspace behavior | N/A (omitted). Spec explicitly says omitted (`docs/games/asteroids/01-GAME-SPEC.md:58`). | Hyperspace has random failure and asteroid-density failure checks; safe return checks are part of flow. | Major gameplay escape/risk mechanic absent. | Critical | High | Implement rev-targeted hyperspace algorithm and expose as mode flag (`classic` vs `zk-short`). |
| V-03 | Wave large-asteroid cap | `largeCount = min(16, 4 + 2*(wave-1))` (`src/game/AsteroidsGame.ts:739`, `risc0-asteroids-verifier/asteroids-core/src/sim.rs:958`). | Wave starts at 4, increases by 2, max 11 (`AstPerWave` path in disassembly). | Difficulty ramp exceeds original by +5 large asteroids at cap. | High | High | Change cap to 11 in classic mode. |
| V-04 | Session-pressure policy | Design targets 2-5 minute runs with anti-lurk pressure (`docs/games/asteroids/01-GAME-SPEC.md:40`). | Original supports marathon play and historically allowed lurking strategy in earlier ROM behavior. | System is tuned for short-run proving, not arcade endurance. | High | High | Keep as intentional in zk mode; add true-classic mode profile. |
| V-05 | Anti-lurk system | Explicit anti-lurk timers and forced spawn-pressure (`LURK_*`, `src/game/constants.ts:67`, saucer logic `src/game/AsteroidsGame.ts:992`; mirrored in Rust). | No equivalent explicit 6s anti-lurk timer in original core logic. | Current ruleset deliberately penalizes stalling far more aggressively. | High | High | Gate anti-lurk under non-classic mode flag. |
| V-06 | Concurrent saucers | Max saucers by wave: 1/2/3 (`src/game/AsteroidsGame.ts:996`, `risc0-asteroids-verifier/asteroids-core/src/sim.rs:534`). | Original runtime model is single active saucer status slot. | Multi-saucer concurrency is non-original and materially harder. | High | High | Cap to 1 in classic mode. |
| V-07 | Saucer bullet slot cap | Saucer bullets are not hard-capped to original slot count; vector can accumulate by cadence/lifetime. | Original allocates 2 saucer bullet slots. | Potential projectile density exceeds original limits. | High | High | Add hard saucer bullet cap = 2 for classic mode. |
| V-08 | Ship bullet autofire behavior | Hold-fire with cooldown (`SHIP_BULLET_COOLDOWN_FRAMES=10`, firing in `src/game/AsteroidsGame.ts:859`). | Original ship-fire shift-register logic includes anti-autofire gating in update ship path. | Fire-input semantics differ (hold behavior easier/more modern). | High | Medium | Recreate shift-register fire gate in classic mode. |
| V-09 | Ship bullet lifetime | 51 frames (`src/game/constants.ts:33`, Rust constants mirror). | Original bullet status initialized to 18 and decremented at frame-gated cadence; effective lifetime behavior differs (commonly derived ~72-frame equivalent). | Shot persistence differs meaningfully. | High | Medium | Adopt original timer/cadence model in classic mode. |
| V-10 | Saucer bullet lifetime | 84 frames (`src/game/constants.ts:35`, Rust mirror). | Original saucer bullets share slot/timer model closer to ship shot timing behavior, with 2-slot limits. | Saucer projectiles live longer and can accumulate more. | High | Medium | Use original slot/timer lifetime behavior with 2-slot cap. |
| V-11 | Saucer type selection thresholds | Small-saucer chance: 22% base, 70% if score > 4000, 90% if lurking (`src/game/AsteroidsGame.ts:1049`; Rust mirror). | Original uses score/timer logic with notably higher thresholding before strong small-saucer pressure (disassembly compares around higher score bands). | Small saucer arrives earlier and more often. | High | Medium | Replace with revision-specific threshold logic (rev-targeted). |
| V-12 | Small saucer accuracy curve | Error shrinks by score/2500 + wave bonus + lurk bonus (`src/game/AsteroidsGame.ts:1089`, Rust mirror). | Original uses mask-table randomization and higher-score accuracy inflection (notably around 35,000). | Accuracy progression curve is fundamentally different. | High | High | Port original randomization/mask-table style aiming in classic mode. |
| V-13 | Saucer firing cadence | Randomized cooldown windows by size/lurk (e.g., 27-96 frame ranges) (`src/game/AsteroidsGame.ts:1034`; Rust mirror). | Original uses timer-based cadence with fixed re-arm values in saucer-shoot path. | Encounter rhythm differs from original timing feel. | Medium | Medium | Implement original saucer timer reload behavior in classic mode. |
| V-14 | Small saucer scoring | `SCORE_SMALL_SAUCER = 1000` (`src/game/constants.ts:48`, Rust constants mirror). | Original score-add path uses BCD `#$99` (990 in tens-based internal representation). | +10 score per small saucer relative to disassembly behavior. | Medium | High | Use revision-accurate scoring (990 if matching disassembly behavior). |
| V-15 | Score model / rollover | TS number and Rust `u32` (saturating add in Rust), effectively no 99,990 arcade rollover. | Original score stored in 2-byte BCD tens/thousands; display-turnover behavior at 99,990. | Endgame/high-score semantics differ. | Medium | High | If strict classic fidelity needed, emulate BCD scoreboard and rollover. |
| V-16 | Starting lives configurability | Fixed `STARTING_LIVES = 3` (`src/game/constants.ts:8`, Rust constants mirror). | Original supports operator-configured starting ships via DIP (3/4 in mapped hardware doc set). | Operator configurability removed. | Medium | High | Expose starting-lives config and include classic defaults. |
| V-17 | Two-player support | No two-player alternating state in deterministic ruleset. | Original includes 1P/2P alternating progression and state handling. | Feature removed from current ruleset. | Medium | High | Add optional 2P mode only if product scope requires strict cabinet parity. |
| V-18 | Asteroid speed progression | Wave multiplier boosts asteroid speed up to +50% cap (`src/game/AsteroidsGame.ts:768`, Rust mirror). | Original asteroid generation/speeds are randomized but not implemented via this explicit per-wave multiplier model. | Speed curve is modernized and steeper. | High | Medium | Replace with original spawn-speed distributions in classic mode. |
| V-19 | Asteroid-cap enforcement edge case | Split logic checks `>= ASTEROID_CAP` after marking source dead; can produce `ASTEROID_CAP + 1` alive (covered by unit test `risc0-asteroids-verifier/asteroids-core/src/sim.rs:1899`). | Original uses fixed slot model for asteroid objects. | Current cap intent is 27 but effective alive count can hit 28. | Medium | High | Enforce hard alive-cap invariant when spawning split children. |
| V-20 | Ship spawn survivability model | Fixed 120-frame invulnerability on spawn (`src/game/constants.ts:32`, `src/game/AsteroidsGame.ts:668`; Rust mirror). | Original respawn/hyperspace safety uses placement checks and timers; no equivalent fixed 2-second invulnerability phase in this form. | Current respawn is more protected and less punishing. | Medium | Medium | Port original safe-return/spawn timing behavior for classic mode. |
| V-21 | Spawn location policy | Ship respawns at center with safety-radius checks (`src/game/AsteroidsGame.ts:672`, Rust mirror). | Original includes random hyperspace destination logic with constrained ranges and safety checks. | Spatial risk profile differs. | Medium | High | Add original random return behavior under hyperspace/classic implementation. |
| V-22 | Saucer vs asteroid interaction | No saucer-asteroid collision resolution path in current collision handler. | Original saucers can be destroyed by asteroid collisions. | Missing interaction changes emergent difficulty and scoring flow. | High | High | Add saucer-asteroid collision handling in both TS and zk cores. |
| V-23 | Wrap-aware saucer aiming | Small saucer aiming uses shortest wrapped delta (`shortestDeltaQ12_4`) (`src/game/AsteroidsGame.ts:1083`; Rust mirror). | Original cross-boundary behavior is revision-dependent; earlier revisions supported edge-lurk exploit due aiming limitations. | Current always-wrap-aware aim removes classic lurk behavior profile. | High | Medium | Make wrap-aim behavior revision-selectable (`rev1/rev2` vs `rev4`). |
| V-24 | Physics/world units | Modern fixed-point world (`960x720`, Q12.4/Q8.8) and drag constants (`src/game/constants.ts:1`, `14`, `74`, etc.). | Original hardware coordinate/timing model differs (DVG-era memory/timing system). | Kinematic feel cannot be 1:1 by constants alone. | Medium | High | If strict feel parity is required, calibrate against frame-captured emulator traces. |

## Items That Already Match Original (Important)
- Ship bullet cap is 4 (`src/game/constants.ts:22`, Rust `constants.rs:21`) and original ship-shot slots are 4.
- Asteroid score bands 20/50/100 match.
- Large saucer score 200 matches.
- Extra life step 10,000 matches.
- Base wave increment pattern (+2 per wave) matches, though cap diverges.
- Asteroid split chain large -> medium -> small matches structurally.
- TS and zk deterministic ordering and transition checks are internally aligned.

## Priority Alignment Plan (If Goal = Near-Original Fidelity)
1. Implement `classic` rules profile with hyperspace, single saucer, 2 saucer-bullet slots, and original wave cap (11).
2. Port original saucer selection + aiming thresholds (including 35k accuracy behavior) and original saucer shot cadence model.
3. Port original shot-timer semantics (slot-based lifetimes and fire behavior), including anti-autofire behavior.
4. Fix hard asteroid-cap enforcement and add saucer-asteroid collision handling.
5. Decide rev-target explicitly (`rev1`, `rev2`, or `rev4`) and codify revision-specific behaviors (especially lurking/cross-boundary aiming differences).
6. Add regression tests comparing deterministic trace checkpoints against a chosen emulator/disassembly reference corpus for classic mode.

## Primary Sources Used
- 6502 disassembly (rev4 corpus):  
  `https://6502disassembly.com/va-asteroids/Asteroids.html`
- Computer Archeology code walkthrough:  
  `https://www.computerarcheology.com/Arcade/Asteroids/Code.html`
- Computer Archeology RAM map:  
  `https://www.computerarcheology.com/Arcade/Asteroids/RAMUse.html`
- Computer Archeology hardware map:  
  `https://www.computerarcheology.com/Arcade/Asteroids/Hardware.html`
- Historical source archive pointer:  
  `https://github.com/historicalsource/asteroids`

