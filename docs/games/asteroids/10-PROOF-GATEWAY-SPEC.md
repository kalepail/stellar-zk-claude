# ZK Asteroids Proof Gateway — System Spec

Last updated: 2026-02-06

## Purpose

This document describes the Cloudflare Worker proof gateway (`worker/`) and the
RISC Zero Asteroids verifier stack (`risc0-asteroids-verifier/`) as they are
implemented today.

---

## System Architecture

```
Browser (Vite/React)
  │
  │ POST /api/proofs/jobs (binary .tape)
  ▼
Cloudflare Worker (Hono API)
  ├── Durable Object: ProofCoordinatorDO
  │     • single active job state (SQLite-backed)
  │     • alarm-driven prover polling loop
  ├── R2: PROOF_ARTIFACTS
  │     • proof-jobs/{jobId}/input.tape
  │     • proof-jobs/{jobId}/result.json
  └── Queue: PROOF_QUEUE (initial dispatch only)
        │
        ▼
  Queue Consumer (same Worker)
        │
        │ POST /api/jobs/prove-tape/raw  (submit tape)
        ▼
RISC0 API Server (Actix, single-flight)
        │
        ▼                           ┌── DO alarm ◄──┐
RISC0 host + zkVM guest             │  polls prover  │
  + asteroids-core                   │  GET /api/jobs │
                                     └───────────────┘
```

---

## Cloudflare Worker (`worker/`)

### Runtime components

| File | Role |
|------|------|
| `worker/index.ts` | Hono app entrypoint, mounts API router + queue consumer |
| `worker/api/routes.ts` | HTTP route handlers |
| `worker/queue/consumer.ts` | Queue message handler (initial prover submission) |
| `worker/prover/client.ts` | Prover HTTP client (submit + poll + summarize) |
| `worker/durable/coordinator.ts` | `ProofCoordinatorDO` — job state machine + alarm polling |
| `worker/tape.ts` | Tape format validation |
| `worker/keys.ts` | R2 key helpers |
| `worker/types.ts` | Shared type definitions |
| `worker/constants.ts` | Default configuration values |
| `worker/utils.ts` | Parsing, retry delay, error formatting |
| `worker/env.ts` | `WorkerEnv` binding type |

### Public API surface

**`GET /api/health`**
- Returns service metadata, compatibility expectations, prover compatibility
  status, and the active job (if any).
- Response includes:
  - `expected.ruleset` and `expected.rules_digest_hex` (AST2 target)
  - optional `expected.image_id` (if pinning is enabled)
  - `prover.status` (`"compatible"` or `"degraded"`)
  - on compatible: `prover.ruleset`, `prover.rules_digest_hex`, `prover.image_id`
  - on degraded: `prover.error`

**`POST /api/proofs/jobs`**
- Request body: raw tape bytes (`application/octet-stream`).
- Validates tape format before accepting.
- Rejects zero-score tapes (`final_score == 0`) with `400`.
- On accept (`202`):
  - Creates job record in DO.
  - Stores tape in R2 at `proof-jobs/{jobId}/input.tape`.
  - Enqueues `{ jobId }` to `PROOF_QUEUE`.
  - Returns `{ success, status_url, job }`.
- On busy (`429`): rejects if another job is already active.

**`GET /api/proofs/jobs/:jobId`**
- Returns public job status snapshot from the Durable Object.

**`GET /api/proofs/jobs/:jobId/result`**
- Returns the stored proof artifact JSON from R2 when the job has succeeded.
- `409` if proof is not yet available. `404` if job or artifact not found.

### Tape validation at ingress

`worker/tape.ts` enforces:
- Non-empty payload, `<= MAX_TAPE_BYTES` (default 2 MiB)
- Magic = `0x5A4B5450`, version = `1`
- Exact byte length = header (16) + frameCount + footer (12)
- CRC-32 checksum match

Metadata extracted from the tape and stored with the job record:
- `seed`, `frameCount`, `finalScore`, `finalRngState`, `checksum`

### Job state model

`ProofCoordinatorDO` manages the lifecycle of each job through these states:

```
queued → dispatching → prover_running → succeeded
                │               │
                ▼               ▼
             retrying ◄────── retrying
                │
                ▼
              failed
```

| Status | Meaning |
|--------|---------|
| `queued` | Job created, tape stored in R2, message enqueued |
| `dispatching` | Queue consumer picked up the message, submitting tape to prover |
| `prover_running` | Prover accepted the job; DO alarm is polling for completion |
| `retrying` | Transient failure; DO alarm will retry (backoff with exponential delay, capped 5 min) |
| `succeeded` | Proof generated, result stored in R2, active slot released |
| `failed` | Terminal error, active slot released |

Single-active-job is enforced via `active_job_id` in DO storage. Only one job
may be active at a time; new submissions receive `429` while a job is in flight.

### Job record shape

```typescript
interface ProofJobRecord {
  jobId: string;
  status: "queued" | "dispatching" | "prover_running" | "retrying" | "succeeded" | "failed";
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  tape: {
    sizeBytes: number;
    key: string;               // R2 key
    metadata: TapeMetadata;    // seed, frameCount, finalScore, finalRngState, checksum
  };
  queue: {
    attempts: number;
    lastAttemptAt: string | null;
    lastError: string | null;
    nextRetryAt: string | null;
  };
  prover: {
    jobId: string | null;      // remote prover job ID
    status: "queued" | "running" | "succeeded" | "failed" | null;
    statusUrl: string | null;
    lastPolledAt: string | null;
    pollingErrors: number;
  };
  result: {
    artifactKey: string;       // R2 key to result.json
    summary: ProofResultSummary;
  } | null;
  error: string | null;
}
```

### Durable Object storage

`ProofCoordinatorDO` uses **SQLite-backed** Durable Object storage (configured
via the `new_sqlite_classes` migration in `wrangler.jsonc`).

Storage keys:
- `active_job_id` — current active job ID (or absent)
- `job:{jobId}` — full `ProofJobRecord`

### Queue consumer (initial dispatch only)

The queue consumer's only job is to submit the tape to the prover. Once the
prover accepts, all subsequent polling happens via DO alarms.

Per message:
1. Call `beginQueueAttempt()` on the DO.
2. If the prover job already exists (re-delivered message after crash),
   `beginQueueAttempt` ensures the alarm is running. Consumer acks immediately.
3. Check wall-clock timeout (`MAX_JOB_WALL_TIME_MS`). If exceeded, mark failed.
4. Load tape from R2.
5. Submit to prover via `POST /api/jobs/prove-tape/raw`.
6. On success: `markProverAccepted()` → schedules first DO alarm → ack.
7. On retry: `markRetry()` → `message.retry()` with exponential backoff.
8. On fatal: `markFailed()` → ack.

Queue config: `max_batch_size=1`, `max_batch_timeout=1`, `max_retries=10`,
`max_concurrency=1`.

### DO alarm polling loop

After `markProverAccepted()` schedules the first alarm, the DO's `alarm()`
method drives all subsequent prover polling:

1. Load active job. If terminal or missing, stop.
2. Check wall-clock timeout (`MAX_JOB_WALL_TIME_MS`, default 1 hour). Fail if exceeded.
3. Call `pollProver()` (budget-limited polling loop within a single alarm invocation).
4. **Running**: save updated prover status, schedule next alarm at `PROVER_POLL_INTERVAL_MS`.
5. **Success**: call `summarizeProof()`, store full prover response in R2 as `result.json`,
   call `markSucceeded()` to release the active slot.
6. **Retry with `clearProverJob`** (prover lost the job, e.g. restart):
   re-read tape from R2, re-submit to prover. If re-submit succeeds,
   `markProverAccepted()` schedules the next alarm. Otherwise backoff + retry.
7. **Retry without `clearProverJob`** (transient poll error): increment
   `pollingErrors`, backoff, schedule alarm.
8. **Fatal**: `markFailed()`.

R2 write failures during result storage are treated as retryable — the alarm
backs off and tries again rather than failing the job.

### Worker ↔ prover HTTP contract

**Submission:**
- `POST {PROVER_BASE_URL}/api/jobs/prove-tape/raw`
- Query params: `receipt_kind`, `segment_limit_po2`, `max_frames`
- Auth headers: `x-api-key` and/or `CF-Access-Client-Id` + `CF-Access-Client-Secret`
- Response: `{ success, job_id, status, status_url }`

**Polling:**
- `GET {PROVER_BASE_URL}/api/jobs/{proverJobId}`
- Response on success: `{ success, status: "succeeded", result: { proof, elapsed_ms } }`
- Worker uses a **budget-limited** polling loop within each alarm invocation
  (`PROVER_POLL_BUDGET_MS`, default 45s). If the budget expires while the
  prover is still running, the alarm returns `"running"` and the DO schedules
  the next alarm.

**Result summary extracted by Worker:**

```typescript
interface ProofResultSummary {
  elapsedMs: number;
  requestedReceiptKind: string;
  producedReceiptKind: string | null;
  journal: {
    seed: number;
    frame_count: number;
    final_score: number;
    final_rng_state: number;
    tape_checksum: number;
    rules_digest: number;
  };
  stats: {
    segments: number;
    total_cycles: number;
    user_cycles: number;
    paging_cycles: number;
    reserved_cycles: number;
  };
}
```

**R2 artifact shape** (`proof-jobs/{jobId}/result.json`):

```json
{
  "stored_at": "ISO8601",
  "prover_response": {
    "success": true,
    "status": "succeeded",
    "result": {
      "proof": {
        "journal": { ... },
        "requested_receipt_kind": "groth16",
        "produced_receipt_kind": "groth16",
        "stats": { ... },
        "receipt": { ... }
      },
      "elapsed_ms": 123456
    }
  }
}
```

The `receipt` field contains the full RISC Zero receipt including the `seal`
bytes needed for on-chain verification.

### Worker configuration (`wrangler.jsonc`)

| Variable | Value | Notes |
|----------|-------|-------|
| `PROVER_RECEIPT_KIND` | `"groth16"` | On-chain-verifiable proof type |
| `PROVER_SEGMENT_LIMIT_PO2` | `"21"` | Segment size limit (power of 2) |
| `PROVER_MAX_FRAMES` | `"18000"` | ~5 minutes at 60fps |
| `PROVER_VERIFY_RECEIPT` | `"1"` | Verifies receipt server-side before success |
| `PROVER_EXPECTED_IMAGE_ID` | _unset_ | Optional image ID pin to prevent prover drift |
| `PROVER_HEALTH_CACHE_MS` | `"30000"` | Prover health cache TTL in Worker isolate |
| `PROVER_POLL_INTERVAL_MS` | `"3000"` | Alarm interval between polls |
| `PROVER_POLL_TIMEOUT_MS` | `"900000"` | 15 min absolute poll timeout |
| `PROVER_POLL_BUDGET_MS` | `"45000"` | Max polling time per alarm |
| `PROVER_REQUEST_TIMEOUT_MS` | `"30000"` | HTTP request timeout |
| `MAX_TAPE_BYTES` | `"2097152"` | 2 MiB tape size limit |
| `MAX_JOB_WALL_TIME_MS` | `"3600000"` | 1 hour total job lifetime |
| `ALLOW_INSECURE_PROVER_URL` | `"0"` | Enforce HTTPS |

Bindings:
- R2 bucket: `stellar-zk-proof-artifacts`
- Queue: `stellar-zk-proof-jobs`
- Durable Object: `ProofCoordinatorDO` (SQLite class)

---

## RISC Zero Asteroids Verifier (`risc0-asteroids-verifier/`)

### Workspace crates

| Crate | Role |
|-------|------|
| `asteroids-core/` | `no_std` deterministic game replay, tape parser, verification journal |
| `methods/guest/` | zkVM guest entrypoint |
| `host/` | Proving library, CLI, benchmark binary |
| `api-server/` | Actix-web HTTP job interface over host prover |

### Verification journal committed by guest

```rust
struct VerificationJournal {
    seed: u32,
    frame_count: u32,
    final_score: u32,
    final_rng_state: u32,
    tape_checksum: u32,
    rules_digest: u32,  // RULES_DIGEST_V2 = 0x41535432 ("AST2")
}
```

Serialized as 24 bytes (6 × u32 LE).

### Host prover behavior

`prove_tape` in `host/src/lib.rs`:
1. Validates dev-mode policy (`RISC0_DEV_MODE` + `allow_dev_mode`).
2. Builds executor env with `max_frames`, original `tape_len`, padded tape, `segment_limit_po2`.
3. Proves using requested receipt kind (composite → succinct → groth16).
4. Detects produced receipt kind from receipt internals.
5. In non-dev mode, requires produced kind == requested kind.
6. Optionally verifies receipt against `VERIFY_TAPE_ID`.
7. Decodes and returns committed journal.

### API server (`api-server/src/main.rs`)

Endpoints:
- `GET /health`
- `POST /api/jobs/prove-tape/raw` (binary tape body)
- `GET /api/jobs/{job_id}`
- `DELETE /api/jobs/{job_id}`

Auth: if `API_KEY` is set, requires `x-api-key` or `Authorization: Bearer` header.

Job states: `queued` → `running` → `succeeded` | `failed`.
Single-flight enforced by global semaphore (concurrency 1) + active-job check.

Query param policy gates: `max_frames`, `receipt_kind`, `segment_limit_po2`,
`allow_dev_mode`, `verify_receipt`.

Operational defaults: `MAX_TAPE_BYTES=2097152`, `MAX_JOBS=64`,
`JOB_TTL_SECS=86400`, `MAX_FRAMES=18000`, `MIN_SEGMENT_LIMIT_PO2=16`,
`MAX_SEGMENT_LIMIT_PO2=22`.

---

## Repository Paths

| Path | Purpose |
|------|---------|
| `worker/index.ts` | Worker entrypoint |
| `worker/api/routes.ts` | HTTP route handlers |
| `worker/queue/consumer.ts` | Queue consumer (initial dispatch) |
| `worker/prover/client.ts` | Prover HTTP client |
| `worker/durable/coordinator.ts` | DO state machine + alarm polling |
| `worker/types.ts` | Shared TypeScript types |
| `worker/tape.ts` | Tape validation |
| `worker/constants.ts` | Default values |
| `wrangler.jsonc` | Worker + binding configuration |
| `risc0-asteroids-verifier/api-server/src/main.rs` | Prover API server |
| `risc0-asteroids-verifier/host/src/lib.rs` | Host proving library |
| `risc0-asteroids-verifier/methods/guest/src/main.rs` | zkVM guest entrypoint |
| `risc0-asteroids-verifier/asteroids-core/src/verify.rs` | Deterministic verification core |

---

## References

- Nethermind RISC Zero verifier contracts: <https://github.com/NethermindEth/stellar-risc0-verifier>
- Cloudflare Queues: <https://developers.cloudflare.com/queues/>
- Cloudflare Durable Objects: <https://developers.cloudflare.com/durable-objects/>
- RISC Zero docs: <https://dev.risczero.com/>
