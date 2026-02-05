# Asteroids Research Compendium - 2026-02-05

Purpose: single, comprehensive research reference for building and extending the Asteroids clone.

## 1) What Was Researched
- Original arcade gameplay mechanics and edge behaviors.
- Revision-level differences (Rev1/Rev2/Rev4 behavior notes).
- Browser game-loop engineering for deterministic, smooth, high-performance play.
- Architecture patterns for maintainability and extension (pooling, event queue, partitioning, state handling, buffering).
- Practical defaults for the current codebase and optional authenticity toggles for future modes.

## 2) Complete Research Run Ledger

## Parallel Search IDs
- `search_b4d3aabf79cb49e7beadfc5f4e282243`
- `search_c015f90207bc421a9e2caa7908f3353c`
- `search_d936248f4bae4c0faa825cb1f043078c`
- `search_b955b33f517f40128d8c1b61353dea4b`
- `search_cb4fe22c99d14647bfdd53afd1303c18`
- `search_157bebd036f74ff1b17e3c9db2d9935f`
- `search_e9704fe0d2814db5b946e936d0013a6d`
- `search_3521fda1cc4f4d4c8afc9ebddce7912b`
- `search_f6b9325d1e434cf2bd42566d0705acee`
- `search_ed16629c808a41afbbd7c140ef747c14`
- `search_90743ebf84604a4fbbc952b7d21f0095`

## Parallel Fetch IDs
- `extract_67b7647978294447a738a1df8dd48124`
- `extract_99091d73e07c44f6899e1de6de5fb727`
- `extract_ffb9db2d7f7e41b68d6bc57b753fc771`
- `extract_bcccc39363c749c4ba9bc5922e850e79`
- `extract_1a53a82d53d2411192c8aa1c9ee99187`
- `extract_b7bb52c171594b058856ea9bac1686ee`
- `extract_080e30486a844edc840235311b285963`
- `extract_4b1bff611ab04d6e97e3bb842c67e5f4`
- `extract_f757db2149224af4aa3678ad179e99bd`
- `extract_e156c442ff384733ac4d70c4e970fa78`

## Perplexity Calls (Recorded by purpose/date)
- `perplexity_search`: source discovery for canonical mechanics.
- `perplexity_reason`: synthesis for loop/performance guidance.
- `perplexity_reason`: synthesis for `990 vs 1000` small-saucer discrepancy.
- `perplexity_research`: mechanics/revisions deep pass.
- `perplexity_research`: browser architecture/performance deep pass.
- `perplexity_research`: extensible TypeScript architecture deep pass.
- Additional retries and earlier timeout attempts are logged in `docs/games/asteroids/asteroids-research-log.md`.

Important traceability note: Perplexity MCP payloads in these sessions did not expose a response ID field. We logged prompt intent + date + result status.

## 3) Source Quality Policy (Applied)

## Tier A (primary/technical references used for decisions)
- https://6502disassembly.com/va-asteroids/
- https://6502disassembly.com/va-asteroids/Asteroids.html
- https://www.computerarcheology.com/Arcade/Asteroids/Code.html
- https://www.computerarcheology.com/Arcade/Asteroids/Hardware.html
- https://gafferongames.com/post/fix_your_timestep/
- https://gameprogrammingpatterns.com/game-loop.html
- https://gameprogrammingpatterns.com/object-pool.html
- https://gameprogrammingpatterns.com/spatial-partition.html
- https://gameprogrammingpatterns.com/state.html
- https://gameprogrammingpatterns.com/event-queue.html
- https://gameprogrammingpatterns.com/double-buffer.html
- https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API
- https://developer.mozilla.org/en-US/docs/Web/API/Canvas_API/Tutorial/Optimizing_canvas

## Tier B (supporting context)
- https://www.arcade-history.com/?n=asteroids-upright-model&page=detail&id=126
- https://www.brasington.org/arcade/products/hs/asteroids/revs.shtml
- https://www.vecfever.com/faq/asteroids/
- https://github.com/historicalsource/asteroids
- https://arcarc.xmission.com/PDF_Arcade_Atari_Kee/
- https://en.wikipedia.org/wiki/Asteroids_(video_game)

## Tier C (explicitly downweighted/discarded for implementation decisions)
- Generic forum posts, unverified strategy pages, low-authority summaries without technical grounding.

## 4) Canonical Mechanics (Consolidated)

## Controls and feel
- Ship uses inertial movement: rotate + thrust + fire + hyperspace.
- World wraps on both axes for ship, asteroids, and bullets.

## Asteroid behavior
- Split chain: large -> two medium, medium -> two small, small -> destroyed.
- Wave start count rises over rounds (baseline: starts at 4 large, increases by 2, capped around 11 large in round starts).
- Cap-related split behavior exists in original code paths when object counts are high.

## Saucer behavior
- Large saucer: less accurate/random fire profile.
- Small saucer: more dangerous/aimed profile that increases pressure with progression.
- Revision-specific anti-lurking behavior differs across ROM sets (Rev2/Rev4 references harden small UFO behavior).

## Lives and progression
- Default operator expectation: extra life each 10,000 points.
- Score display/rollover behavior around 99,990.

## Scoring baseline
- Large asteroid: 20
- Medium asteroid: 50
- Small asteroid: 100
- Large saucer: 200
- Small saucer discrepancy:
  - Disassembly commentary often points to internal constant shown as `$99` (interpreted as 990 in many analyses).
  - Many player-facing references present 1000.
  - Decision used for build: default to 1000 in "arcade-familiar" mode; optionally expose ROM-accurate score profile.

## 5) Engineering Synthesis (Browser + Performance)

## Loop architecture
- Fixed simulation timestep (`1/60`) with accumulator.
- Clamp long frame deltas (practical guard: `0.25s`) after tab inactivity or spikes.
- Cap max simulation substeps to avoid spiral-of-death.
- Render using interpolation alpha (`accumulator / dt`).

## Input architecture
- Event handlers update input state (and optionally edge-trigger queues).
- Simulation consumes input in fixed-step update, not in render.
- Keep one-shot actions (fire/pause/hyperspace) robust with edge-trigger tracking.

## Visibility handling
- Use `visibilitychange` to pause simulation on hidden tabs.
- On resume, reset timing baseline before stepping.

## Canvas performance guidance
- Use `requestAnimationFrame`, avoid `setInterval` for render cadence.
- Minimize per-frame allocations and context state churn.
- Prefer integer-aligned draw coords where feasible.
- Apply DPR scaling on resize events (not every frame).
- Consider layered/offscreen approaches for static/repeated drawing paths.

## 6) Extensibility Architecture (Recommended)

## System boundaries
- Keep simulation separate from rendering.
- Isolate systems: input, simulation, collision, rendering, audio, UI/state.

## Data model
- Componentized entities or data-oriented structures for predictable updates.
- Keep update order explicit and deterministic.

## Object lifecycle
- Use object pooling for high-churn entities (bullets, particles, temp effects).
- Monitor pool pressure and define overflow policy (drop, recycle oldest, or hard-fail per entity type).

## Event orchestration
- Use event queue/bus boundaries between systems to reduce tight coupling.
- Keep gameplay-critical events deterministic and ordered.

## Replay and determinism hooks
- Seeded RNG.
- Input recording keyed by fixed simulation frame index.
- Optional periodic state snapshots for long replay seek/recovery.

## 7) Testing Matrix (Comprehensive)

## Determinism
- Same seed + same input stream -> same state hash at frame N.
- Replayed session score/lives/wave timeline must match source run.

## Gameplay correctness
- Scoring increments by entity type.
- Extra life awarded at configured threshold(s).
- Wave progression and saucer transitions follow expected rules.
- Asteroid split outcomes verified, including high-count cap edge behavior.

## Physics/collision
- Wraparound continuity on all object types.
- Collision outcomes validated for ship/asteroid/bullet/saucer interactions.

## Lifecycle
- Tab hide/show pause behavior stable and free of time-jump artifacts.

## Performance
- Desktop 60Hz: steady frame budget compliance.
- Degraded profile: capped-step fallback behaves gracefully (no runaway catch-up).

## 8) Current Codebase Decision Snapshot

The current implementation values are centralized in `src/game/constants.ts` and align with research-driven defaults:
- Loop: `FIXED_TIMESTEP`, `MAX_FRAME_DELTA`, `MAX_SUBSTEPS`.
- Progression/scoring: `STARTING_LIVES`, `EXTRA_LIFE_SCORE_STEP`, asteroid/saucer score constants.
- Classic feel controls: thrust/turn/speed/drag, bullet constraints, object caps, spawn timings.

Core system files:
- `src/game/AsteroidsGame.ts`
- `src/game/constants.ts`
- `src/game/types.ts`
- `src/game/math.ts`
- `src/game/input.ts`

Rendering/UI wrapper:
- `src/components/AsteroidsCanvas.tsx`
- `src/App.tsx`

## 9) Failed/Partial Retrieval Notes (Captured)
- Some remote artifact fetches returned limited/blank extraction in MCP excerpt mode (notably raw historical source files).
- One PDF fetch path (`Asteroids Rom Change Info`) returned fetch-error metadata despite HTTP 200 in the tool result.
- These are documented to preserve auditability and avoid false confidence in missing evidence.

## 10) Final Open Decisions
- Whether to ship authenticity presets immediately:
  - `Arcade Familiar` (1000 small saucer, expected modern familiarity)
  - `ROM Accurate` (constants/quirks aligned to validated disassembly behavior)
- How much revision behavior to expose as user-facing options vs keeping single default mode.

---
This compendium, plus `docs/games/asteroids/asteroids-research-log.md` and `docs/games/asteroids/asteroids-deep-research-2026-02-05.md`, is intended to fully capture research inputs, synthesis, IDs, and implementation implications.
