# Leaderboard Event Sourcing + Galexie Backfill

## What Is Enforced
- Leaderboard API is sourced from ingested chain events, not proof-job records.
- Source event expected: `score_submitted` (canonical, forward-only).
- Event payload consumed for leaderboard/analytics:
  - Journal mirror: `seed`, `frame_count`, `final_score`, `final_rng_state`, `tape_checksum`, `rules_digest`
  - Reward context: `claimant`, `previous_best`, `new_best`, `minted_delta`, `journal_digest`
  - Metadata: `event_id`, `tx_hash`, `event_index`, `ledger`, `closed_at`
- Ingestion is idempotent by `eventId` and safe to re-run.
- Ingestion supports RPC-first and Galexie backfill modes:
  - `auto` (recommended): RPC `getEvents` primary with Galexie fallback.
  - `rpc`: direct Soroban RPC `getEvents`.
  - `events_api`: direct Galexie event endpoint (`GALEXIE_SCORE_EVENTS_PATH`).
  - `datalake`: Galexie `.xdr.zst` ledger batches from `/v1` with event extraction.
  - `datalake` parser accepts both manifest-style keys (`ledgersPerBatch`, `batchesPerPartition`)
    and schema-style keys (`ledgers_per_file`, `files_per_partition`).
  - Object retrieval tries compression extensions in a safe order (`zstd`/`zst`) and uses SEP-54 key
    formatting (`%08X--low[-high].xdr.<ext>`), including non-partitioned layouts.

## Worker Configuration

Set non-secret vars in `wrangler.jsonc`:
- `LEADERBOARD_DB` (D1 binding in `d1_databases`)
- `SCORE_CONTRACT_ID`
- `GALEXIE_API_BASE_URL`
- `GALEXIE_SOURCE_MODE` (`auto`, `rpc`, `datalake`, or `events_api`)
- `GALEXIE_RPC_BASE_URL`
- `CLAIM_NETWORK_PASSPHRASE` (optional; used for RPC auto-selection when `GALEXIE_RPC_BASE_URL` is unset)
- `GALEXIE_SCORE_EVENTS_PATH`
- `GALEXIE_DATASTORE_ROOT_PATH`
- `GALEXIE_DATASTORE_OBJECT_EXTENSION`
- `GALEXIE_REQUEST_TIMEOUT_MS`
- `LEADERBOARD_SYNC_CRON_ENABLED`
- `LEADERBOARD_SYNC_CRON_LIMIT`
- `LEADERBOARD_CATCHUP_INTERVAL_MINUTES`
- `LEADERBOARD_CATCHUP_WINDOW_LEDGERS`
- `LEADERBOARD_FORWARD_REPLAY_WINDOW_LEDGERS`

Defaulted in this worktree for Quasar Pro:
- `GALEXIE_API_BASE_URL=https://galexie-pro.lightsail.network`
- `GALEXIE_SOURCE_MODE=auto`
- `GALEXIE_RPC_BASE_URL=` (optional explicit override)
- `CLAIM_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"` for testnet
- `GALEXIE_SCORE_EVENTS_PATH=/events`
- `GALEXIE_DATASTORE_ROOT_PATH=/v1`
- `GALEXIE_DATASTORE_OBJECT_EXTENSION=zst`
- cron: `*/5 * * * *`

Set secrets:

```bash
npx wrangler secret put GALEXIE_API_KEY
npx wrangler secret put LEADERBOARD_ADMIN_KEY
```

If your provider key is labeled as a Lightsail key, use that value for `GALEXIE_API_KEY`.

## Network Alignment Requirement
- `galexie-pro.lightsail.network` config currently reports mainnet passphrase (`Public Global Stellar Network ; September 2015`).
- If `SCORE_CONTRACT_ID` points to a testnet deployment, sync will stay empty by design because provider and contract are on different networks.
- For testnet E2E ingestion, use a testnet provider endpoint or a testnet Galexie dataset.
- RPC defaulting behavior:
  - If `GALEXIE_RPC_BASE_URL` is set, that URL is always used.
  - If it is unset and `CLAIM_NETWORK_PASSPHRASE` is testnet, ingestion tries:
    - `https://rpc-testnet.lightsail.network`
    - `https://soroban-testnet.stellar.org`
    - `https://soroban-rpc.testnet.stellar.gateway.fm`
    - `https://rpc.ankr.com/stellar_testnet_soroban`
  - Otherwise, ingestion defaults to `https://rpc-pro.lightsail.network`.
- On testnet, Galexie fallback (`datalake`/`events_api`) is only attempted when `GALEXIE_API_BASE_URL` host is testnet-compatible (`testnet` in hostname).

## Sync Endpoints

Admin header required:
- `x-leaderboard-admin-key: <LEADERBOARD_ADMIN_KEY>`

### Status

```bash
curl -sS "http://127.0.0.1:8787/api/leaderboard/sync/status" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY"
```

### One-Time Legacy DO -> D1 Migration

```bash
curl -sS -X POST "http://127.0.0.1:8787/api/leaderboard/migrate/do-to-d1" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY" \
  -H "x-migration-confirm: do-to-d1"
```

For large legacy datasets, migrate in smaller pages:

```bash
curl -sS -X POST "http://127.0.0.1:8787/api/leaderboard/migrate/do-to-d1?chunk_size=500" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY" \
  -H "x-migration-confirm: do-to-d1"
```

### Forward Fill (cursor-based)

```bash
curl -sS -X POST "http://127.0.0.1:8787/api/leaderboard/sync" \
  -H "content-type: application/json" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY" \
  -d '{"mode":"forward","limit":200}'
```

### Backfill (bounded ledger range)

```bash
curl -sS -X POST "http://127.0.0.1:8787/api/leaderboard/sync" \
  -H "content-type: application/json" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY" \
  -d '{"mode":"backfill","from_ledger":123456,"to_ledger":124000,"limit":200}'
```

### Force Datalake Backfill (for verification/recovery)

```bash
curl -sS -X POST "http://127.0.0.1:8787/api/leaderboard/sync" \
  -H "content-type: application/json" \
  -H "x-leaderboard-admin-key: $LEADERBOARD_ADMIN_KEY" \
  -d '{"mode":"backfill","source":"datalake","from_ledger":123456,"to_ledger":124000,"limit":200}'
```

Backfill safety rules:
- Require `from_ledger`.
- Reject `from_ledger > to_ledger`.
- Upsert is idempotent, so repeated backfill windows are safe.

## Catch-Up Cron
- The Worker scheduled handler runs forward sync every cron tick.
- Periodic overlapping backfill is optional and controlled by:
  - `LEADERBOARD_CATCHUP_INTERVAL_MINUTES`
  - `LEADERBOARD_CATCHUP_WINDOW_LEDGERS`
- Forward sync also replays a small overlapping window (`LEADERBOARD_FORWARD_REPLAY_WINDOW_LEDGERS`) and relies on idempotent upserts to heal transient provider/cursor gaps.
- Catch-up backfill requests are forced through `datalake` first and then degrade to RPC if Galexie is unavailable.
- This overlap is the automatic recovery path for missed files/events and short RPC retention windows.

## Public Read Endpoints
- `GET /api/leaderboard?window=10m|day|all&limit=<n>&offset=<n>&address=<G...|C...>`
- `GET /api/leaderboard/player/:claimantAddress`
- `POST /api/leaderboard/player/:claimantAddress/profile/auth/options`
- `PUT /api/leaderboard/player/:claimantAddress/profile`

Profile update flow:
- `POST .../profile/auth/options` accepts `credential_id`, `credential_public_key`, and optional `transports`, then returns one-time WebAuthn assertion options + `challenge_id`.
- Client calls `startAuthentication(options)` in browser, then sends `PUT .../profile` with `auth.challenge_id` + `auth.response`.
- Worker verifies passkey assertion server-side, consumes challenge (single-use), updates authenticator counter, then writes profile.
- No `x-claimant-address` header is accepted for profile updates anymore.

Pagination notes:
- `limit` is capped at `100`.
- `offset` is capped at `10000`.

## Operational Notes
- Keep sync/admin endpoints private behind the admin key.
- Run frequent forward sync (RPC primary), plus controlled backfill windows for full historical coverage.
- Use `ingestion.last_synced_at` and `ingestion.highest_ledger` in `/api/leaderboard` response to monitor freshness.
