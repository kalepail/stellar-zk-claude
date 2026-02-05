# Asteroids Deep Research - 2026-02-05

Goal: expand implementation-grade research for a faithful, performant, extensible Asteroids clone using both Perplexity and Parallel research loops.

Consolidated master document: `docs/games/asteroids/asteroids-research-compendium-2026-02-05.md`

## Scope
- Gameplay correctness: scoring, waves, splitting, lives, UFO behavior, revision quirks.
- Browser engineering: fixed-step simulation, timing stability, input handling, canvas performance.
- Extensibility: architecture patterns, replay/determinism hooks, performance budgets.

## Research Runs and IDs

### Parallel Search IDs (2026-02-05)
- `search_3521fda1cc4f4d4c8afc9ebddce7912b`
- `search_f6b9325d1e434cf2bd42566d0705acee`
- `search_ed16629c808a41afbbd7c140ef747c14`
- `search_90743ebf84604a4fbbc952b7d21f0095`

### Parallel Fetch IDs (2026-02-05)
- `extract_b7bb52c171594b058856ea9bac1686ee`
- `extract_080e30486a844edc840235311b285963`
- `extract_4b1bff611ab04d6e97e3bb842c67e5f4`
- `extract_f757db2149224af4aa3678ad179e99bd`
- `extract_e156c442ff384733ac4d70c4e970fa78`

### Perplexity Calls (2026-02-05)
- `perplexity_search`: primary-source mechanics query (returned results, no MCP response id exposed).
- `perplexity_research`: faithful-clone mechanics deep research (succeeded, no MCP response id exposed).
- `perplexity_reason`: browser loop/timestep synthesis (succeeded, no MCP response id exposed).
- `perplexity_reason`: small saucer score contradiction synthesis (succeeded, no MCP response id exposed).
- `perplexity_research`: browser architecture/performance deep research (succeeded, no MCP response id exposed).
- `perplexity_research`: extensible TypeScript architecture deep research (succeeded, no MCP response id exposed).

Note: Perplexity MCP outputs do not currently include a call/result identifier in the returned payload. Traceability is recorded by prompt intent + run date + this document entry.

## Curated Source Tiers

## Tier A (authoritative for implementation)
- https://6502disassembly.com/va-asteroids/
- https://6502disassembly.com/va-asteroids/Asteroids.html
- https://www.computerarcheology.com/Arcade/Asteroids/Code.html
- https://www.computerarcheology.com/Arcade/Asteroids/Hardware.html
- https://gafferongames.com/post/fix_your_timestep/
- https://gameprogrammingpatterns.com/game-loop.html
- https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API
- https://developer.mozilla.org/en-US/docs/Web/API/Canvas_API/Tutorial/Optimizing_canvas
- https://gameprogrammingpatterns.com/object-pool.html
- https://gameprogrammingpatterns.com/spatial-partition.html
- https://gameprogrammingpatterns.com/state.html
- https://gameprogrammingpatterns.com/event-queue.html
- https://gameprogrammingpatterns.com/double-buffer.html

## Tier B (supporting revision/context references)
- https://www.brasington.org/arcade/products/hs/asteroids/revs.shtml
- https://www.vecfever.com/faq/asteroids/
- https://www.arcade-history.com/?n=asteroids-upright-model&page=detail&id=126
- https://github.com/historicalsource/asteroids
- https://arcarc.xmission.com/PDF_Arcade_Atari_Kee/

## Tier C (discarded for implementation decisions)
- Unvetted forum posts, generic blogs, isolated walkthrough/tip pages without technical backing.

## Synthesis: Build-Ready Mechanics

## Core rules (ship, rocks, UFO)
- Inertial ship with rotate/thrust/fire/hyperspace.
- Wraparound world for ship, asteroids, and shots.
- Splits: large -> 2 medium, medium -> 2 small, small -> removed.
- UFO variants: large (random fire), small (aimed fire with increasing accuracy as score rises).

## Scoring baseline
- Large asteroid: 20.
- Medium asteroid: 50.
- Small asteroid: 100.
- Large saucer: 200.
- Small saucer:
  - Disassembly constant commonly shown as `$99` (reported as 990 in disassembly commentary).
  - Public arcade-facing references commonly present 1000.
  - Implementation decision: default to 1000 for player-familiar mode; add optional "ROM-accurate score table" mode.

## Progression
- Waves start at 4 large asteroids, increase by +2, capped around 11 initial large asteroids.
- Extra ship every 10,000 points (default expectation).
- Score display rollover behavior around 99,990.

## Revision behavior deltas
- Rev1 attract text differs from Rev2 (`ASTEROIDS BY ATARI` vs `© 1979 ATARI INC`).
- Rev2/Rev4 harden anti-lurking behavior via more aggressive small UFO behavior.
- Rev4 references often describe cross-boundary small UFO threat increase.

## Notable edge behaviors
- Asteroid object-cap behavior can alter split outcomes at high object counts.
- Very high life counts can cause slowdown/watchdog effects on original hardware.

## Synthesis: Engineering Decisions

## Loop and timing
- Use fixed-step simulation (`dt = 1/60`) with accumulator.
- Clamp long frame deltas (recommended practical clamp around 0.25s).
- Cap max simulation substeps per frame to avoid spiral-of-death under load.
- Render with interpolation factor (`alpha = accumulator / dt`).

## Input model
- Store key state from event handlers.
- Consume intent in fixed simulation step, not in render pass.
- Keep queued "edge-trigger" events for one-shot actions (fire, pause) to avoid event loss.

## Visibility and lifecycle
- Observe `visibilitychange`; pause simulation and reset wall-clock baseline on resume.
- Prefer `requestAnimationFrame` for rendering cadence and browser scheduling.

## Canvas performance
- Avoid per-frame allocations and unnecessary context state churn.
- Keep draw coordinates integer-aligned where possible.
- Use layered/offscreen techniques for static or repeated visuals when helpful.
- Apply DPR scaling on resize rather than every frame.

## Extensibility architecture
- Data-oriented entity model with separate systems.
- Isolate systems: input, simulation, collision, rendering, audio, UI.
- Add object pools for high-churn entities (bullets, particles, temporary effects).
- Add event queue boundary between systems for loose coupling.
- Add deterministic replay hooks:
  - Seeded RNG.
  - Input recording by fixed frame index.
  - Optional periodic snapshots for long replay seek.

## Suggested Test Matrix (added detail)
- Determinism: same seed + same input stream -> identical key state hash at frame N.
- Gameplay: scoring increments, extra-life thresholds, wave advancement, UFO transitions.
- Physics: wraparound, collision outcomes, asteroid split counts under cap pressure.
- Lifecycle: hide/show tab pause correctness and catch-up stability.
- Performance: 60Hz desktop and degraded device profile with capped-step fallback.

## Open Items
- Confirm whether project should expose authenticity profile presets:
  - `Arcade Familiar` (1000 small saucer, default modern expectations)
  - `ROM Accurate` (disassembly-leaning constants where validated)
- Decide how much revision-specific behavior to surface in UI vs hard-code into one default mode.
