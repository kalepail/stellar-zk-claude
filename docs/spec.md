# ZK Asteroids Score Token - System Spec (Current Architecture)

Last updated: 2026-02-06

## Purpose

This document describes the architecture that is actually implemented in this repository today, with emphasis on:

1. The Cloudflare Worker proof gateway (`worker/`)
2. The RISC Zero Asteroids verifier stack (`risc0-asteroids-verifier/`)

It also calls out planned settlement components that are not yet implemented in this repo.

---

## Current Scope vs Target Scope

### Implemented now

- Browser captures deterministic Asteroids tape bytes.
- Browser submits tape to Worker endpoint: `POST /api/proofs/jobs`.
- Worker validates tape format, stores artifacts in R2, and schedules proving through Cloudflare Queues.
- Worker enforces single-active-job coordination with a Durable Object.
- Worker dispatches to RISC Zero API server: `POST /api/jobs/prove-tape/raw`.
- Worker polls prover status: `GET /api/jobs/{job_id}`.
- Worker stores full prover response and exposes status/result endpoints.

### Planned / not implemented in this repo yet

- Passkey smart-wallet transaction signing flow.
- Relay submission endpoint (`/api/send`) and fee sponsorship path.
- Soroban score contract source + on-chain mint integration in this codebase.
- Worker-side extraction of on-chain proof components (`seal`, `image_id`, `journal_raw`) from receipt JSON.

---

## System Architecture (Implemented)

```
Browser (Vite/React)
  |
  | POST /api/proofs/jobs (binary .tape)
  v
Cloudflare Worker (Hono API)
  |\
  | \-- Durable Object: ProofCoordinatorDO (single active job state)
  | \
  |  \-- R2: PROOF_ARTIFACTS
  |        - proof-jobs/{jobId}/input.tape
  |        - proof-jobs/{jobId}/result.json
  |
  \-- Queue: PROOF_QUEUE (max_concurrency=1, max_batch_size=1)
        |
        v
   Queue Consumer (same Worker)
        |
        | POST /api/jobs/prove-tape/raw
        | GET  /api/jobs/{proverJobId}
        v
RISC0 API Server (Actix, single-flight)
        |
        v
RISC0 host + zkVM guest + asteroids-core
```

---

## Cloudflare Worker Architecture (`worker/`)

### Runtime components

- HTTP router: `worker/index.ts`, `worker/api/routes.ts`
- Queue consumer: `worker/queue/consumer.ts`
- Prover HTTP client: `worker/prover/client.ts`
- Coordinator Durable Object: `worker/durable/coordinator.ts`
- Tape validation: `worker/tape.ts`
- Artifact keying: `worker/keys.ts`

### Public API surface

- `GET /api/health`
  - Returns service metadata and the active job (if any).
  - Response includes:
    - `service: "stellar-zk-proof-gateway"`
    - `mode: "single-active-job"`

- `POST /api/proofs/jobs`
  - Request body: raw tape bytes (`application/octet-stream`).
  - Validates tape before enqueue.
  - On accept (`202`):
    - Creates DO job record.
    - Stores tape in R2 (`proof-jobs/{jobId}/input.tape`).
    - Enqueues `{ jobId }` to `PROOF_QUEUE`.
    - Returns `status_url` and job state.
  - On busy (`429`): single-active-job guard rejects new job.

- `GET /api/proofs/jobs/:jobId`
  - Returns public job status snapshot from Durable Object.

- `GET /api/proofs/jobs/:jobId/result`
  - Returns stored result artifact JSON from R2 when available.
  - `409` if result is not ready.

### Tape validation performed at ingress

Worker-side validation (`worker/tape.ts`) enforces:

- Non-empty payload
- `<= MAX_TAPE_BYTES` (default `2 MiB`)
- Magic = `0x5A4B5450`
- Version = `1`
- Exact byte length = `header + frame_count + footer`
- CRC32 checksum match

Metadata extracted and stored with job:

- `seed`
- `frameCount`
- `finalScore`
- `finalRngState`
- `checksum`

### Job state model (Durable Object)

`ProofCoordinatorDO` tracks lifecycle per job:

- `queued`
- `dispatching`
- `prover_running`
- `retrying`
- `succeeded`
- `failed`

Single-active-job semantics are enforced with `active_job_id` in DO storage.

### Queue consumer execution model

Processing is intentionally sequential (one message at a time).

Per message:

1. `beginQueueAttempt` updates queue attempt metadata.
2. If no prover job exists yet, load input tape from R2 and submit to prover.
3. Poll prover status (bounded budget per queue delivery).
4. On success:
   - Summarize proof fields.
   - Persist full prover payload in R2 (`result.json`) as `{ stored_at, prover_response }`.
   - Mark DO job `succeeded` and release active slot.
5. On retryable errors:
   - Mark job `retrying`.
   - Requeue with exponential backoff (`2s` min, capped at `300s`).
6. On fatal errors:
   - Mark job `failed` and release active slot.

### Worker <-> prover HTTP contract used today

Submission:

- `POST {PROVER_BASE_URL}/api/jobs/prove-tape/raw`
- Query params set by Worker:
  - `receipt_kind` from `PROVER_RECEIPT_KIND`
  - `segment_limit_po2` from `PROVER_SEGMENT_LIMIT_PO2` (default `19`)
  - `max_frames` from `PROVER_MAX_FRAMES` (default `18000`)
- Headers may include:
  - `x-api-key`
  - `CF-Access-Client-Id`
  - `CF-Access-Client-Secret`

Polling:

- `GET {PROVER_BASE_URL}/api/jobs/{proverJobId}`

Worker success summary fields:

- `elapsedMs`
- `requestedReceiptKind`
- `producedReceiptKind`
- `journal`
- `stats`

### Worker defaults from `wrangler.jsonc`

- `PROVER_RECEIPT_KIND = "composite"`
- `PROVER_SEGMENT_LIMIT_PO2 = "19"`
- `PROVER_MAX_FRAMES = "18000"`
- `PROVER_POLL_INTERVAL_MS = "3000"`
- `PROVER_POLL_TIMEOUT_MS = "900000"` (15m)
- `PROVER_POLL_BUDGET_MS = "45000"`
- `PROVER_REQUEST_TIMEOUT_MS = "30000"`
- `MAX_TAPE_BYTES = "2097152"`
- `MAX_QUEUE_ATTEMPTS = "180"`
- `ALLOW_INSECURE_PROVER_URL = "0"`

Queue consumer config in `wrangler.jsonc`:

- `max_batch_size = 1`
- `max_concurrency = 1`
- `max_retries = 100`
- `retry_delay = 3`

Note: queue-level `max_retries` can cap effective retries before `MAX_QUEUE_ATTEMPTS` is reached.

---

## RISC Zero Asteroids Verifier Architecture (`risc0-asteroids-verifier/`)

### Workspace crates

- `asteroids-core/`
  - Deterministic replay, tape parser/serializer, verification journal.

- `methods/guest/`
  - zkVM guest entrypoint.
  - Reads `(max_frames, tape_len, tape)` from executor env.
  - Calls `verify_guest_input` and commits `VerificationJournal`.

- `host/`
  - Proving library and CLI.
  - Receipt kind options: `composite | succinct | groth16`.

- `api-server/`
  - Async HTTP job interface over host prover.
  - Single-flight proving policy.

### Verification journal committed by guest

`VerificationJournal` fields:

- `seed: u32`
- `frame_count: u32`
- `final_score: u32`
- `final_rng_state: u32`
- `tape_checksum: u32`
- `rules_digest: u32` (`RULES_DIGEST_V1 = 0x41535431`, "AST1")

### Host prover behavior (`host/src/lib.rs`)

`prove_tape`:

1. Validates dev-mode policy (`RISC0_DEV_MODE` + `allow_dev_mode`).
2. Builds executor env with `max_frames`, original `tape_len`, padded tape bytes, and `segment_limit_po2`.
3. Proves using requested receipt kind.
4. Detects produced receipt kind from receipt internals.
5. In non-dev mode, requires produced kind == requested kind.
6. Optionally verifies receipt against `VERIFY_TAPE_ID`.
7. Decodes committed journal.

### API server contract (`api-server/src/main.rs`)

Endpoints:

- `GET /health` (open)
- `POST /api/jobs/prove-tape/raw` (binary tape body, auth if configured)
- `GET /api/jobs/{job_id}` (auth if configured)
- `DELETE /api/jobs/{job_id}` (auth if configured)

Auth:

- If `API_KEY` is set, accepts either:
  - `x-api-key: <API_KEY>`
  - `Authorization: Bearer <API_KEY>`

Job model:

- States: `queued`, `running`, `succeeded`, `failed`
- Single-flight enforced by:
  - global semaphore with concurrency `1`
  - active-job check during enqueue

Policy gates on submit query params:

- `max_frames`
- `receipt_kind`
- `segment_limit_po2`
- `allow_dev_mode`
- `verify_receipt`

Strict defaults:

- `ALLOW_DEV_MODE_REQUESTS=false`
- `ALLOW_UNVERIFIED_RECEIPTS=false`
- `RISC0_DEV_MODE=0` in production

Operational defaults:

- `MAX_TAPE_BYTES=2097152`
- `MAX_JOBS=64`
- `JOB_TTL_SECS=86400`
- `JOB_SWEEP_SECS=60`
- `MAX_FRAMES=18000`
- `MIN_SEGMENT_LIMIT_PO2=16`
- `MAX_SEGMENT_LIMIT_PO2=22`

### API success payload shape used by Worker

For `GET /api/jobs/{job_id}` when succeeded, Worker relies on:

- `status = "succeeded"`
- `result.proof.journal`
- `result.proof.requested_receipt_kind`
- `result.proof.produced_receipt_kind`
- `result.proof.stats`
- `result.elapsed_ms`

Worker stores the full prover response JSON as artifact and a compact summary in DO state.

---

## Compatibility Notes and Gaps

### Reconciled from prior spec

- Workflows are not in use; pipeline is Queue + Durable Object.
- Public proof endpoint is `/api/proofs/jobs`, not `/prove`.
- Worker default receipt kind is currently `composite`, not `groth16`.
- Result delivery is polling + R2 artifact retrieval; no WebSocket/push path exists.

### Gaps to on-chain Groth16 settlement

To reach contract-verifiable Groth16 submission flow, the codebase still needs:

1. Proof component extraction path (`seal`, `image_id`, `journal_raw` or digest) suitable for Soroban verifier inputs.
2. A concrete client/worker submission path to contract invocation (relay + signed auth entries).
3. Score contract implementation and deployment artifacts integrated in this repo.
4. Production decision on receipt-kind policy (`groth16` end-to-end) with matching worker defaults.

---

## Repository Paths (Current)

- Worker API and queue runtime: `worker/index.ts`, `worker/api/routes.ts`, `worker/queue/consumer.ts`
- DO coordinator: `worker/durable/coordinator.ts`
- Worker prover client: `worker/prover/client.ts`
- Worker config: `wrangler.jsonc`
- Verifier API server: `risc0-asteroids-verifier/api-server/src/main.rs`
- Verifier host proving library: `risc0-asteroids-verifier/host/src/lib.rs`
- Guest method entrypoint: `risc0-asteroids-verifier/methods/guest/src/main.rs`
- Deterministic verification core: `risc0-asteroids-verifier/asteroids-core/src/verify.rs`

---

## References

- Nethermind RISC Zero verifier contracts: <https://github.com/NethermindEth/stellar-risc0-verifier>
- Cloudflare Queues: <https://developers.cloudflare.com/queues/>
- Cloudflare Durable Objects: <https://developers.cloudflare.com/durable-objects/>
- RISC Zero docs: <https://dev.risczero.com/>
