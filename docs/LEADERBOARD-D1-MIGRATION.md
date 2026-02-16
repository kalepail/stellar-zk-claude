# Leaderboard D1 Migration Runbook

This project now stores leaderboard data in D1 (`LEADERBOARD_DB`) while keeping proof orchestration in Durable Objects.

## Scope

- Moved to D1:
  - leaderboard events
  - leaderboard profiles
  - leaderboard ingestion state
- Kept in Durable Objects:
  - proof/job coordinator state machine
  - queue/alarm-based proof lifecycle

## Prerequisites

1. Create a D1 database:
   - `npx wrangler d1 create stellar-zk-leaderboard`
2. Set the returned `database_id` in:
   - `wrangler.jsonc` (`d1_databases[].database_id`)
3. Deploy worker:
   - `npx wrangler deploy`

## One-time data migration from DO to D1

Use the admin endpoint:

- `POST /api/leaderboard/migrate/do-to-d1`
- Required header: `x-leaderboard-admin-key: <LEADERBOARD_ADMIN_KEY>`
- Required safety header: `x-migration-confirm: do-to-d1`
- Optional query: `chunk_size=<50..2000>` (default `500`) for paged migration

What it does:

1. Reads legacy leaderboard events/profiles/state from `ProofCoordinatorDO`.
2. Upserts into D1 (idempotent by `event_id`).
3. Copies ingestion state into D1.
4. Clears in-memory leaderboard response cache.

This endpoint is safe to retry.

## Verification

1. Check ingestion status:
   - `GET /api/leaderboard/sync/status`
2. Trigger sync:
   - `POST /api/leaderboard/sync` with admin key
3. Validate API reads:
   - `GET /api/leaderboard?window=10m`
   - `GET /api/leaderboard?window=day`
   - `GET /api/leaderboard?window=all`
   - `GET /api/leaderboard/player/:claimantAddress`

## Rollback

If needed, keep using pre-migration deployment revision. Legacy data remains in DO storage until you explicitly remove it.

## Notes

- D1 schema is self-initialized by the worker on first access.
- Leaderboard reads remain cache-assisted (`ETag` + short TTL response cache), with rolling cache buckets for time-windowed views.
