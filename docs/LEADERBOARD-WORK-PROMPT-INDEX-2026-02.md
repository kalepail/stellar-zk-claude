# Leaderboard Work Prompt Index (Feb 2026)

This appendix captures the high-signal user prompts used to reconstruct leaderboard work context.

## Session IDs Used
- `019c485f-407b-77d2-8e03-fa1e9d0becc5`
- `019c49aa-fea0-7322-bbc8-0c8d5c1e42c9`
- `019c4849-8879-7a73-b9a8-1cce8026e6ef`
- `019c4979-c384-73e0-8d40-1cc285038891`

## Chronological Prompt Excerpts

### 2026-02-10
- `2026-02-10T16:30:14Z` (`019c485f`): Request for 10-minute rolling leaderboard mechanics, seed generation strategy, replay/update behavior per seed/address, and storage TTL audit.
- `2026-02-10T16:40:58Z` (`019c485f`): Clarified that slots can be external; emphasized seed-driven flow, address-in-journal linkage, and strict no-replay goals.
- `2026-02-10T16:58:52Z` (`019c485f`): Clarified claimant naming/shape and submission/auth model (submission actor can differ from reward recipient).
- `2026-02-10T20:25:06Z` (`019c485f`): "Forward-only" confirmation and no backward compatibility/migration requirement.
- `2026-02-10T20:46:26Z` (`019c485f`): Requested Smart Account Kit integration path (testnet/relayer context) to pass address bytes with journals.
- `2026-02-10T22:00:35Z` (`019c4979`): Requested timeout simplification and a 10-minute acceptance posture.
- `2026-02-10T22:27:49Z` (`019c49aa`): Requested `/leaderboard` page plan from on-chain events with 10m/day/all-time, player pages, profile data, pagination/find-me, and low-cost scalable indexing.
- `2026-02-10T23:32:18Z` (`019c49aa`): Ranking and UTC clarifications; requested free/load-balanced provider options and resilience.
- `2026-02-10T23:53:00Z` (`019c49aa`): Requested deep research on Galexie/Quasar ingestion/backfill strategy and rate-limit-aware design.

### 2026-02-11
- `2026-02-11T01:12:06Z` (`019c4849`): Asked about retroactive proof after wallet connect.
- `2026-02-11T02:06:43Z` (`019c4849`): Asked whether claimant swap/replay could steal rewards.
- `2026-02-11T02:24:47Z` (`019c4849`): Strong requirement: runs should be cryptographically tied to addresses; demo-mode concept acceptable if rewards/leaderboard are gated.
- `2026-02-11T15:53:51Z` (`019c49aa`): Concern that work appeared unrelated to `/leaderboard` (Galexie backfill focus mismatch).
- `2026-02-11T16:01:27Z` (`019c49aa`): Reaffirmed expectation that ingestion/backfill and CI smoke tests be implemented, not docs-only.

## Worktree Context From Session Metadata
Observed working directories in these sessions:
- `/Users/kalepail/Desktop/stellar-zk`
- `/Users/kalepail/.codex/worktrees/85f0/stellar-zk`
- `/Users/kalepail/.codex/worktrees/74f1/stellar-zk`
- `/Users/kalepail/Desktop/stellar-zk-codex`

Current status at reconstruction time:
- `85f0/stellar-zk`, `74f1/stellar-zk`, and `stellar-zk-codex` are no longer present on disk.
- Prompt/session records remain available in `~/.codex/sessions` and `~/.codex/archived_sessions`.

## Extraction Artifacts Used During Reconstruction
Generated during forensic pass:
- `/tmp/stellar_zk_sessions.tsv`
- `/tmp/stellar_zk_user_prompts.clean.tsv`
- `/tmp/stellar_zk_user_prompts.leaderboard.highsignal.tsv`

These files were transient helpers; the permanent recovery narrative is in:
- `docs/LEADERBOARD-WORK-RECONSTRUCTION-2026-02.md`
