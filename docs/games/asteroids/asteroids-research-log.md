# Asteroids Clone Research Log

Date: 2026-02-04
Goal: gather implementation-grade research for a faithful, performant, extensible Asteroids clone.

Master synthesis reference: `docs/games/asteroids/asteroids-research-compendium-2026-02-05.md`

## Tooling + IDs

### Parallel Search IDs
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

### Parallel Fetch IDs
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

### Perplexity Calls
- `perplexity_research` (2 calls): timed out at 60s; no response id returned by tool.
- `perplexity_search` (3 calls): returned mixed-quality result sets; no response id returned by tool.
- `perplexity_reason` (2 calls): returned synthesized summaries; no response id returned by tool.
- Retry on `2026-02-04`:
  - `perplexity_research` gameplay deep-research retry: timed out at 60s; no response id.
  - `perplexity_research` architecture deep-research retry: timed out at 60s; no response id.
  - `perplexity_research` brief-scope retry: timed out at 60s; no response id.
- Retry on `2026-02-05` after MCP timeout config update:
  - `perplexity_research` faithful-clone prompt retry: timed out at 60s; no response id.
  - Observation: updated `tool_timeout_sec = 300` in global MCP config, but active tool calls still enforced 60s timeout in this session.
- Retry on `2026-02-05` (post-config + current session retry):
  - `perplexity_research` faithful-clone prompt: succeeded (long-form report with citation list).
  - Tool output contained citations but still did not expose a Perplexity response ID in MCP payload.
  - Quality review: mixed citations; retained Tier A/B links already listed below, discarded noisy forum/blog/video references.
- Expansion on `2026-02-05` (user-requested deeper loop):
  - `perplexity_search` additional source discovery pass: succeeded; mixed quality.
  - `perplexity_research` gameplay/revision deep pass: succeeded; mixed citations, curated to Tier A/B only.
  - `perplexity_reason` browser loop/timestep synthesis: succeeded.
  - `perplexity_reason` small-saucer `990` vs `1000` discrepancy synthesis: succeeded.
  - `perplexity_research` architecture/performance deep pass: succeeded; curated and filtered.
  - `perplexity_research` extensible TypeScript architecture pass: succeeded; curated and filtered.
  - Perplexity MCP still did not provide response IDs in payload.

---

## Research Loops (Query -> Review -> Refine)

### Loop 1: broad discovery
- Actions:
  - Parallel + Perplexity broad queries on original mechanics and web game architecture.
- Outcome:
  - Captured baseline material, but many noisy/non-authoritative results.
- Review:
  - Kept only references that could be triangulated by higher-quality sources.

### Loop 2: primary-source targeting
- Actions:
  - Targeted Atari manual/flyer and technical references.
- Outcome:
  - Identified official/manual mirrors and ROM-analysis sources.
- Review:
  - PDF extraction from some manual mirrors failed; needed fallback sources.

### Loop 3: authoritative web-tech guidance
- Actions:
  - Pulled canonical loop/perf docs (Gaffer, Game Programming Patterns, MDN).
- Outcome:
  - Clear consensus on fixed timestep, accumulator, interpolation, RAF, visibility throttling.
- Review:
  - Elevated to Tier A for architecture decisions.

### Loop 4: reverse-engineering constants
- Actions:
  - Pulled Asteroids disassembly and annotated code references.
- Outcome:
  - Captured concrete values (e.g., max asteroids constant, score storage behavior, ship settings).
- Review:
  - Used for implementation defaults where manuals were inaccessible.

### Loop 5: discrepancy audit
- Actions:
  - Compared sources for scoring/limits quirks.
- Outcome:
  - Found `small saucer = 990` in disassembly mirrors vs common published `1000`.
- Review:
  - Decision: use classic user-facing `1000` by default, support a “ROM-accurate scoring” toggle later.

### Loop 6: implementation mapping
- Actions:
  - Converted findings into build-ready system design constraints.
- Outcome:
  - Research considered complete enough to start implementation.

### Loop 7: strict-source refresh (Perplexity + Parallel)
- Actions:
  - Re-ran gameplay and revision research with primary-source targeting.
- Outcome:
  - Strengthened traceability for constants, revisions, and edge behaviors.
- Review:
  - Dropped noisy tertiary links; retained disassembly/manual-level references.

### Loop 8: browser engineering deep pass
- Actions:
  - Expanded game-loop/performance research on fixed-step timing and browser APIs.
- Outcome:
  - Reinforced fixed timestep + accumulator + delta clamp + interpolation strategy.
- Review:
  - Tiered sources toward MDN, Gaffer, and Game Programming Patterns.

### Loop 9: extensibility architecture pass
- Actions:
  - Added architecture-focused research (object pooling, event queue, spatial partition, state machine, buffering).
- Outcome:
  - Produced implementation-ready extensibility and testing guidance.
- Review:
  - Pattern docs accepted; generic blog/forum architecture advice downgraded.

---

## Tiered Source Quality

### Tier A (used for core decisions)
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
- https://6502disassembly.com/va-asteroids/
- https://6502disassembly.com/va-asteroids/Asteroids.html
- https://www.computerarcheology.com/Arcade/Asteroids/Code.html
- https://www.computerarcheology.com/Arcade/Asteroids/Hardware.html

### Tier B (supporting/contextual)
- https://www.arcade-history.com/?n=asteroids-upright-model&page=detail&id=126
- https://en.wikipedia.org/wiki/Asteroids_(video_game)
- https://atari.fandom.com/wiki/Asteroids
- https://www.brasington.org/arcade/products/hs/asteroids/revs.shtml
- https://www.vecfever.com/faq/asteroids/
- https://github.com/historicalsource/asteroids

### Tier C (discarded/noisy for implementation)
- Generic blogs, forum anecdotes, watch marketing pages, unrelated StackOverflow entries.

---

## Consolidated Gameplay Findings (for clone baseline)

## Core feel
- Inertial ship controls (rotate, thrust, fire, hyperspace).
- Screen-wrap world for ship, asteroids, and shots.
- Asteroids split large -> medium -> small; each size faster/harder.
- UFO variants: large (less accurate/random), small (aimed and deadlier over progression).

## Canonical scoring (player-facing default)
- Large asteroid: 20
- Medium asteroid: 50
- Small asteroid: 100
- Large saucer: 200
- Small saucer: 1000 (with optional ROM-accurate mode later)

## Progression and lives
- Start with configurable 3 or 4 ships.
- Extra ship every 10,000 points (default arcade expectation).
- Next wave spawns more large asteroids up to cap.
- Score rollover around 99,990 (5-digit display behavior).

## Important quirks to optionally support
- Lurking exploit behavior in original revisions.
- Spawn/saucer behavior differences between revisions.
- Object-cap edge behavior (fragment split simplification near caps).

---

## Engineering Findings (for implementation)

## Loop and determinism
- Use fixed-step simulation (`dt = 1/60`) with accumulator.
- Clamp long frame delta (e.g., max 0.25s) after tab return.
- Bound max updates per frame to prevent spiral-of-death.
- Render with interpolation alpha (`accumulator / dt`) for smoothness.

## Input
- Event listeners only update key-state map.
- Actual movement/fire decisions happen inside fixed update step.
- Prevent browser scroll and focus leakage from gameplay keys.

## Performance
- Use `requestAnimationFrame` (not `setInterval`) for render scheduling.
- Pause/resume simulation via `visibilitychange` + `document.hidden`.
- Use simple data-oriented entities and reuse objects to minimize GC churn.
- Avoid expensive per-frame allocations and needless state changes.
- Handle DPR scaling once per resize; keep canvas draw path integer-friendly.

## Reliability/testing
- Decouple simulation from renderer so core logic is testable headlessly.
- Add deterministic tests for:
  - asteroid splitting and counts
  - scoring increments and extra-life awards
  - collision outcomes
  - wraparound position correctness
- Add smoke test for pause/resume and tab-hide recovery.

---

## Research-to-Design Traceability

- Fixed timestep/accumulator/interpolation:
  - https://gafferongames.com/post/fix_your_timestep/
  - https://gameprogrammingpatterns.com/game-loop.html
- RAF behavior and high-refresh correctness:
  - https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame
- Hidden-tab behavior / pause policy:
  - https://developer.mozilla.org/en-US/docs/Web/API/Page_Visibility_API
- Canvas rendering optimization:
  - https://developer.mozilla.org/en-US/docs/Web/API/Canvas_API/Tutorial/Optimizing_canvas
- Asteroids constants and revision-level mechanics:
  - https://6502disassembly.com/va-asteroids/
  - https://6502disassembly.com/va-asteroids/Asteroids.html
  - https://www.computerarcheology.com/Arcade/Asteroids/Code.html
  - https://www.computerarcheology.com/Arcade/Asteroids/Hardware.html
  - https://www.vecfever.com/faq/asteroids/
  - https://www.brasington.org/arcade/products/hs/asteroids/revs.shtml
- Extensibility architecture patterns:
  - https://gameprogrammingpatterns.com/object-pool.html
  - https://gameprogrammingpatterns.com/spatial-partition.html
  - https://gameprogrammingpatterns.com/state.html
  - https://gameprogrammingpatterns.com/event-queue.html
  - https://gameprogrammingpatterns.com/double-buffer.html

---

## Open Questions (to resolve during build)

- Default to strictly “arcade-familiar” scoring (`1000` small saucer) or expose ROM-accurate toggle from day 1.
- Which revision behavior to emulate for saucer edge-fire/lurking anti-exploit.
- How far to go on authenticity vs fun in first playable (recommended: fun-first with optional “classic mode” flags).
