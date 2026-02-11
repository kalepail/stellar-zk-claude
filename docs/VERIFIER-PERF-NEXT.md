# Verifier Perf: Next Exploration Notes (AST3)

This doc captures the next performance/determinism investigations to run later, without weakening strict fairness guarantees.

## Constraints (Non-Negotiable)

- Proof verification must continue to use strict replay (`verify_tape -> replay_strict`) and enforce all game rules deterministically.
- No backwards compatibility: AST3 tapes are the only supported format (v2 header, no claimant bytes, old tapes rejected).
- Any gameplay-semantic change in Rust must be mirrored to TypeScript (sim + tape serializer) for 1:1 parity.
- Determinism: no floats, no nondeterministic iteration, no dependence on host timing, and no data-structure reordering that changes collision resolution order.

## Current Best Known Baseline

Hotspot (pprof, medium case): `Game::step_decoded` dominates (~75% cycles). Secondary costs: `replay_strict`/`validate_transition`, plus CRC/tape parsing.

Best measured `scripts/bench-core-cycles.sh` numbers so far (strict verifier):
- short: `310,952`
- medium: `~4,153,3xx`
- real: `~19,149,1xx` (segments `10`)

## What Already Proved "Not Worth It" (Regressed in A/B)

Avoid retesting these unless the surrounding code changes materially:

- `shortest_delta_q12_4` unsigned range-check variant (regressed).
- Branchless `abs`-style AABB collision reject (regressed).
- Pow2-mod RNG fast-path (`x & (n-1)`) in `next_int` (regressed despite being correct).
- Reusing `TransitionState` across frames to reduce `transition_state()` calls (regressed on RV32 due to spills/copies).
- Small “clever” branch rewrites in collision/turn logic (mixed-to-negative).

## Next Targets (Ordered By Expected ROI)

### 1) Replace `Vec::retain` With Manual Stable Compaction

Why: `retain` carries closure overhead and can be expensive in the guest. We already track `prune_mask`, so we can do a specialized, stable, in-place compaction per entity type.

Candidate approach (stable order preserved, semantics preserved):
- For each vector with dead/alive flags, do:
  - `write = 0`
  - for `read` in `0..len`:
    - if alive: copy `v[read] -> v[write]` when needed; `write += 1`
  - `truncate(write)`

Notes:
- Must preserve iteration order exactly (stable compaction, not swap-remove).
- Only run compaction when the relevant prune bit is set (matching current behavior).
- Mirror to TS only if TS uses identical pruning mechanics and you change visible behavior (should not be necessary if semantics are identical).

How to A/B:
- Implement compaction helpers for `asteroids`, `bullets`, `saucers`, `saucer_bullets`.
- Run `bash scripts/bench-core-cycles.sh` and keep only if it improves real + medium.

### 2) Fixed-Cap Storage for Bullets/Saucers (Stable Order)

Why: these collections have hard caps:
- ship bullets: 4
- saucer bullets: 2
- saucers: <= 3 (capacity currently 4)

Idea:
- Replace `Vec<T>` with a fixed-cap “slot array” and a `len`:
  - push = write at `len` (if `len < CAP`)
  - prune = stable compaction into the same array
  - iterate `0..len`

Constraints:
- Stable order is mandatory to keep collision resolution identical.
- Avoid `Option<T>` if it forces extra branching; prefer `len` + contiguous storage.

Expected outcome:
- Possibly reduces bounds/allocator overhead and makes compaction cheaper.

### 3) Type-Tightening Experiments (Only If Bench Confirms)

Tempting changes that may or may not help on RV32:
- `angle` as `u8` instead of `i32`
- timers/life as `i16/u16`
- positions as `i16` (Q12.4 world fits, but watch wrap/shortest-delta and intermediate math)

Reality check:
- On RV32, narrower integer types can get slower due to sign/zero extension and extra masking.
- Only pursue this if the data-layout/compaction work in (1)/(2) is promising, since smaller structs help compaction/memmove.

Must-do if attempted:
- Add explicit bound comments + debug asserts (debug-only) for safe casts.
- Mirror TS types/behavior if semantics change (usually it shouldn’t).
- A/B with `bench-core-cycles` immediately.

### 4) Micro-Optimizations Inside `validate_transition` (No Check Removal)

`validate_transition` is measurable (~8-9% in pprof) and runs every frame.

Permissible work:
- Reduce redundant computations (e.g., reuse computed deltas where possible within the function).
- Consolidate branches while keeping equivalent logic.

Do NOT:
- Remove checks.
- Skip checks based on “shouldn’t happen”.

### 5) Profiling Hygiene: Get Line-Level Attribution

Current `go tool pprof -list` can’t locate source (“unknown” file paths). Fixing this may reveal more actionable hotspots.

Ideas to investigate:
- Ensure the benchmark build emits usable debug paths for guest symbols.
- If needed, emit a symbol map or adjust build flags so pprof resolves to real file paths.

Even if guest attribution stays coarse, still capture pprof before/after large refactors (compaction/storage) to confirm where cycles moved.

## A/B Test Protocol (Keep It Boring)

- One change at a time (single PR/commit).
- `cargo test -p asteroids-verifier-core` before bench.
- `bash scripts/bench-core-cycles.sh` for the A/B.
- Accept only improvements in the “real” case; “short/medium” must not regress materially.
- Record the before/after cycle counts in the commit message.

## Determinism + Overflow Checklist

When touching math/types:
- Confirm all intermediate products fit in `i32` (especially `dx*dx + dy*dy` and any shifted radii).
- Ensure wrap/shortest-delta behavior is identical across Rust + TS (including edge cases near 0 and world size).
- Do not introduce float math or host-dependent randomness.
- Preserve entity iteration order and collision resolution order (stable compaction only).
