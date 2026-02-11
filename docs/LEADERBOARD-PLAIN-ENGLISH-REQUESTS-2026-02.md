# Leaderboard Prompt Translation (Plain English)

## Purpose
This doc translates the original user prompts into plain-English product/engineering requests and maps each to the area of the system it refers to.

Use this as the quick kickoff brief for continuing leaderboard work.

## Legend
- Scope tag:
  - `contract`
  - `worker`
  - `frontend`
  - `ingestion`
  - `security`
  - `ops`
  - `docs`

## Prompt Translation Map

| Date (UTC) | Plain-English Request | What It Was In Reference To | Scope |
|---|---|---|---|
| 2026-02-10 | Build rolling 10-minute leaderboards keyed by deterministic seeds, with day/all-time views and reward accounting. | Core leaderboard product model and lifecycle around proof timing vs competition windows. | contract, ingestion, frontend |
| 2026-02-10 | Keep "slot" orchestration mostly off-chain; use seed-driven competition and deterministic UI rules. | Simplifying on-chain state while preserving official competition definition in UI/backend rules. | contract, frontend |
| 2026-02-10 | Prevent journal replay and claimant theft (same user or different user replaying a journal). | Journal uniqueness rules, claimant binding, anti-theft guarantees. | security, contract, worker |
| 2026-02-10 | Use explicit claimant address semantics, but allow backend relayers to submit on behalf of users. | Distinguishing submission actor from reward recipient; relay-friendly auth model. | contract, worker, security |
| 2026-02-10 | Move forward-only: no migrations/dual legacy paths unless absolutely necessary. | Removal of compatibility branches in verifier, worker, API store, and docs. | security, docs, ops |
| 2026-02-10 | Integrate Smart Account Kit and ensure address data can flow into proof/journal submission. | Wallet login/signing and claimant identity wiring from frontend to backend flow. | frontend, worker |
| 2026-02-10 | Align proving and gateway timeout expectations with real runtime (about 5m prove, accept up to 10m). | API/worker timeout constants, queue behavior, and user-facing expectations. | worker, ops, docs |
| 2026-02-10 | Plan a full `/leaderboard` feature: rolling + day + all-time boards, player pages, profile links, rich stats, pagination, and "find me". | Product UI/API specification for leaderboard and profile surfaces. | frontend, ingestion |
| 2026-02-10 | Pick a low-cost, reliable indexing strategy and handle outages/backfill. | Event indexing architecture and reliability strategy (including provider diversification). | ingestion, ops |
| 2026-02-10 | Research Lightsail/Quasar/Galexie options deeply (rate limits, backfill fit, long-term maintainability). | Provider evaluation and data ingestion design due diligence. | ingestion, ops |
| 2026-02-11 | Clarify whether gameplay can happen pre-login and be proven later, vs requiring login first. | Product/security decision around demo mode, retroactive claimant binding, and reward eligibility. | frontend, security, contract |
| 2026-02-11 | Require cryptographic binding so runs cannot be relabeled to another claimant. | Strong anti-theft guarantee in tape/journal/proof pipeline. | security, contract, worker |
| 2026-02-11 | Keep code simple and modern by deleting backward-compatible branches and legacy paths. | Codebase hygiene and simplification across services, scripts, and docs. | docs, ops, security |
| 2026-02-11 | Ensure leaderboard work stays on actual leaderboard deliverables, not drifting to unrelated refactors. | Delivery alignment and scope control for `/leaderboard` implementation. | frontend, ingestion, ops |
| 2026-02-11 | Implement ingestion/backfill paths in code (not docs only), plus CI smoke coverage. | End-to-end ingestion reliability with automated verification of API/cache behavior. | ingestion, worker, ops |

## Priority Implications
Based on prompt intent, the effective delivery order should be:
1. Lock security invariants (claimant binding + no replay).
2. Finalize ingestion/backfill pipeline with reliability and tests.
3. Complete `/leaderboard` and player/profile UI surfaces.
4. Tune ops/docs for mainnet readiness after the above are proven.

## Source References
Detailed prompt excerpts are in:
- `docs/LEADERBOARD-WORK-PROMPT-INDEX-2026-02.md`

Narrative reconstruction and commit mapping are in:
- `docs/LEADERBOARD-WORK-RECONSTRUCTION-2026-02.md`
