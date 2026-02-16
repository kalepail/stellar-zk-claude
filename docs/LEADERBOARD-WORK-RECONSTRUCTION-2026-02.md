# Leaderboard Work Reconstruction (Feb 2026)

## Purpose
This document reconstructs the leaderboard-related work done in `stellar-zk` during February 2026, using:
- Codex session metadata and user prompts preserved under `~/.codex/sessions` and `~/.codex/archived_sessions`
- Git commit history in this repository
- Current repository state on branch `autopilot`

This is written as a recovery artifact so work can be resumed without guessing.

## Scope
Included:
- `/leaderboard` planning and ingestion discussions
- 10-minute rolling leaderboard requirements
- claimant-address/tape/journal anti-replay requirements tied to rewards and leaderboard integrity
- worker/api timeout and ingestion-path requests related to leaderboard reliability

Not included:
- pure autopilot tuning loops unless they directly affected leaderboard-proof flow requirements

## Evidence Sources
Primary session IDs and files (high signal):
- `019c485f-407b-77d2-8e03-fa1e9d0becc5`
- `019c49aa-fea0-7322-bbc8-0c8d5c1e42c9`
- `019c4849-8879-7a73-b9a8-1cce8026e6ef`
- `019c4979-c384-73e0-8d40-1cc285038891`

See appendix file `docs/LEADERBOARD-WORK-PROMPT-INDEX-2026-02.md` for timestamped prompt excerpts.

## What You Asked For (Reconstructed Intent)
From your prompts, the target system was:
1. A leaderboard product with:
- rolling 10-minute seed-based competition
- daily and all-time aggregates
- `/leaderboard` page with rich stats
- player pages + editable profile metadata (username/link)
- pagination-friendly "find me" behavior

2. Security and correctness constraints:
- no journal replay by the same user or a different user
- claimant identity bound to proof/journal flow so rewards cannot be stolen
- explicit forward-only posture (no legacy compatibility paths)

3. Data pipeline constraints:
- index on-chain events reliably and cheaply
- support backfill + forward-fill (Galexie/Data Lake + Archive RPC discussion)
- protect upstream API keys behind worker API

4. Operational constraints:
- 10-minute acceptance window aligned with proving reality (~5m prove, ~10m accept window)
- aggressive simplification and removal of old compatibility branches

## Worktree and Branch Reconstruction
Observed historical working contexts from Codex logs:
- `/Users/kalepail/Desktop/stellar-zk`
- `/Users/kalepail/.codex/worktrees/85f0/stellar-zk` (leaderboard planning session)
- `/Users/kalepail/.codex/worktrees/74f1/stellar-zk`
- `/Users/kalepail/Desktop/stellar-zk-codex` (earlier branch/worktree phase)

Current disk status:
- `.codex/worktrees/85f0/stellar-zk` and `.codex/worktrees/74f1/stellar-zk` no longer exist
- `stellar-zk-codex` no longer exists
- session logs still exist and are the source of truth for prompt history

Current git state:
- Branch: `autopilot`
- Head: `5a69766`

## Commit-Level Reconstruction (What Landed)
The strongest leaderboard-adjacent implementation cluster landed on 2026-02-10 and 2026-02-11.

### A) Claimant/Journal/Contract hardening (core anti-replay direction)
- `e034300` `feat(contract): make claimant explicit arg and lock 24-byte journals`
- `6f3b908` `feat(worker): add claim relay queue and claimant-bound submissions`
- `4d24e33` `refactor(claim-flow): remove manual fallback path and keep relay-only claims`

Interpretation:
- Contract and worker path were moved toward explicit claimant handling and stricter replay-related behavior.
- Manual fallback claim path was intentionally removed in favor of relay-only flow.

### B) AST3 and tape/header flow churn
- `d81248b` `ui: bind tape claimant at submit; core: faster shortest_delta`
- `330812d` `verifier-core: require claimant strkey in tape header`
- `9f46584` `ts/worker: enforce claimant strkey header (no padding)`
- `a72a6b3` `scripts: require claimant in generated tapes`
- `91a6bfe` `fixtures: regenerate tapes with claimant header`
- `c0ff6e6` `refactor(ast3): remove claimant from tape and verifier paths`

Interpretation:
- There was a rapid iteration where claimant handling moved between tape/header and adjacent surfaces.
- Net effect requires careful re-validation when resuming work, because policy was evolving quickly.

### C) Timeout and gateway alignment
- `1241d02` `gateway: groth16 submit + 10m timeout alignment`
- `3876a03` `docs: tighten prover/gateway timeouts`

Interpretation:
- The requested 10-minute window concern was reflected in code/docs updates.

### D) Legacy-removal / forward-only cleanup
- `1980aa3` `chore(ast3): remove legacy coordinator path and refresh forward-only docs`
- `506f1e2` `refactor(api-store): remove legacy schema migration branches`
- `ce70982` `hardening(prover-health): require explicit AST3 ruleset match`

Interpretation:
- The explicit "no legacy cruft" direction was actively implemented.

## What Was Planned vs What Was Fully Landed

### Clearly landed
- Claimant/journal/relay and AST3 hardening workstreams
- timeout alignment and relay-flow simplification
- forward-only cleanup in multiple services

### Partially landed or uncertain from git alone
- Full `/leaderboard` feature scope requested in session `019c49aa`:
  - rolling 10-minute + day + all-time UI
  - player pages/profile editing
  - indexer-backed API path with "find me" UX
- There is no clearly named commit in this branch history explicitly indicating complete `/leaderboard` UI and end-to-end ingestion release.

This lines up with your later prompt saying work felt unrelated to `/leaderboard`.

## Key Recovery Conclusion
You did a large amount of important prerequisite work (claimant security, replay constraints, relay flow, AST3 cleanup), but the end-state `/leaderboard` product scope appears only partially reflected in visible commit labels and needs explicit closure verification.

## Suggested Resume Checklist
When continuing from here, verify these before coding:
1. Freeze final claimant-binding policy (single canonical location and encoding).
2. Confirm replay guarantees at contract level with tests that cover same journal + different claimant attempts.
3. Audit whether `/leaderboard` page and player/profile flows exist end-to-end or only as plan/prototype.
4. Validate ingestion/backfill path in code (not docs only): Archive RPC + Galexie fallback + cache strategy.
5. Re-run smoke tests specifically for ingestion + leaderboard API + cache-bust behavior.

## Relevant Paths Touched by This Workstream
Representative files from high-signal commits:
- `stellar-asteroids-contract/contracts/asteroids_score/src/lib.rs`
- `worker/durable/coordinator.ts`
- `worker/claim/relay.ts`
- `worker/prover/client.ts`
- `src/App.tsx`
- `src/proof/api.ts`
- `src/components/AsteroidsCanvas.tsx`
- `docs/games/asteroids/10-PROOF-GATEWAY-SPEC.md`
- `docs/games/asteroids/11-CLIENT-INTEGRATION-SPEC.md`
- `risc0-asteroids-verifier/api-server/src/store/mod.rs`

## Notes
- This reconstruction intentionally uses both user-intent logs and git history because commit messages alone do not capture all requested product scope.
- The prompt index doc is included to preserve intent details that were not encoded in commit subjects.
