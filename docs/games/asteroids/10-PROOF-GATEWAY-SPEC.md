# ZK Asteroids Proof Gateway — System Spec

Last updated: 2026-02-11

## Purpose

This document describes the Cloudflare Worker proof gateway (`worker/`) and the
RISC Zero Asteroids verifier stack (`risc0-asteroids-verifier/`) as they are
implemented today.

---

## System Architecture

```
Browser (Vite/React)
  │
  │ POST /api/proofs/jobs (binary .tape + x-claimant-address)
  ▼
Cloudflare Worker (Hono API)
  ├── Durable Object: ProofCoordinatorDO
  │     • single active job state (SQLite-backed)
  │     • proof state + claim state
  │     • alarm-driven prover polling loop
  ├── R2: PROOF_ARTIFACTS
  │     • proof-jobs/{jobId}/input.tape
  │     • proof-jobs/{jobId}/result.json
  ├── Queue: PROOF_QUEUE
  │     • dispatches tape to prover API
  └── Queue: CLAIM_QUEUE
        • dispatches proved result to claim relay

Proof queue path:
  PROOF_QUEUE → POST /api/jobs/prove-tape/raw → RISC0 API Server
  DO alarm polls prover (GET /api/jobs/{id}) until success/failure

Claim queue path:
  CLAIM_QUEUE → claim relay HTTP endpoint (submit_score on-chain)
```

---

## Cloudflare Worker (`worker/`)

### Runtime components

| File | Role |
|------|------|
| `worker/index.ts` | Hono app entrypoint, mounts API router + queue consumer |
| `worker/api/routes.ts` | HTTP route handlers |
| `worker/queue/consumer.ts` | Queue message handler (proof dispatch + claim dispatch) |
| `worker/prover/client.ts` | Prover HTTP client (submit + poll + summarize) |
| `worker/claim/direct.ts` | Relayer-only Channels submitter (`submit_score` via `func+auth`) |
| `worker/claim/submit.ts` | Claim submission coordinator (relayer-only) |
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
  - `expected.ruleset` and `expected.rules_digest_hex` (AST3 target)
  - optional `expected.image_id` (if pinning is enabled)
  - `prover.status` (`"compatible"` or `"degraded"`)
  - on compatible: `prover.ruleset`, `prover.rules_digest_hex`, `prover.image_id`
  - on degraded: `prover.error`

**`POST /api/proofs/jobs`**
- Request body: raw tape bytes (`application/octet-stream`).
- Required header: `x-claimant-address` (validated Stellar strkey).
- Validates tape format before accepting.
- Rejects zero-score tapes (`final_score == 0`) with `400`.
- On accept (`202`):
  - Creates job record in DO.
  - Stores tape in R2 at `proof-jobs/{jobId}/input.tape`.
  - Enqueues `{ jobId }` to `PROOF_QUEUE`.
  - Returns `{ success, status_url, job }`.
- On busy (`429`): rejects if another job is already active.

**`GET /api/proofs/jobs/:jobId`**
- Returns public job status snapshot from the Durable Object, including
  claim relay status (`queued | submitting | retrying | succeeded | failed`).

**`GET /api/proofs/jobs/:jobId/result`**
- Returns the stored proof artifact JSON from R2 when the job has succeeded.
- `409` if proof is not yet available. `404` if job or artifact not found.

### Tape validation at ingress

`worker/tape.ts` enforces:
- Non-empty payload, `<= MAX_TAPE_BYTES` (default 2 MiB)
- Magic = `0x5A4B5450`, version = `2`
- Rules tag = `AST3` and header reserved bytes are zero
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
| `retrying` | Transient failure; DO alarm will retry (backoff with exponential delay, capped 60 s) |
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
    segmentLimitPo2: number | null;
    lastPolledAt: string | null;
    pollingErrors: number;
    recoveryAttempts: number;
  };
  claim: {
    claimantAddress: string;
    status: "queued" | "submitting" | "retrying" | "succeeded" | "failed";
    attempts: number;
    lastAttemptAt: string | null;
    lastError: string | null;
    nextRetryAt: string | null;
    submittedAt: string | null;
    txHash: string | null;
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

### Queue consumers (proof + claim)

The worker has two queue consumers:

1. **Proof queue (`stellar-zk-proof-jobs`)**
   - Submits the stored tape to prover API.
   - Retries transient errors with bounded exponential backoff.
   - Marks terminal failures in DO state.

2. **Claim queue (`stellar-zk-claim-jobs`)**
   - Runs only after proof success.
   - Builds `journal_raw_hex` from the proved 24-byte journal.
   - Computes `journal_digest_hex = sha256(journal_raw_hex)`.
   - Submits `{ claimant_address, journal_raw_hex, journal_digest_hex, prover_response }`
     to `RELAYER_URL`.
   - Tracks claim retry/failure/success state in DO.

Both queues use `max_batch_size=1`, `max_concurrency=1`, `max_retries=10`,
and dedicated DLQs (`stellar-zk-proof-jobs-dlq`, `stellar-zk-claim-jobs-dlq`).

### DO alarm polling loop

After `markProverAccepted()` schedules the first alarm, the DO's `alarm()`
method drives all subsequent prover polling:

1. Load active job. If terminal or missing, stop.
2. Check wall-clock timeout (`MAX_JOB_WALL_TIME_MS`, default 11 minutes). Fail if exceeded.
3. Call `pollProver()` (budget-limited polling loop within a single alarm invocation).
4. **Running**: save updated prover status, schedule next alarm at `PROVER_POLL_INTERVAL_MS`.
5. **Success**: call `summarizeProof()`, store full prover response in R2 as `result.json`,
   call `markSucceeded()` which also enqueues a claim job.
6. **Retry with `clearProverJob`** (prover lost the job, e.g. restart):
   re-read tape from R2, re-submit to prover. If re-submit succeeds,
   `markProverAccepted()` schedules the next alarm. Otherwise backoff + retry.
   This recovery path is intentionally capped to 1 attempt per job to avoid
   long retry loops on deterministic prover failures.
   The same bounded recovery policy applies if an alarm observes a missing
   prover job ID and needs to re-submit from stored tape.
7. **Retry without `clearProverJob`** (transient poll error): increment
   `pollingErrors`, backoff, schedule alarm.
8. **Fatal**: `markFailed()`.

R2 write failures during result storage are treated as retryable — the alarm
backs off and tries again rather than failing the job.

### Worker ↔ prover HTTP contract

**Submission:**
- `POST {PROVER_BASE_URL}/api/jobs/prove-tape/raw`
- Query params currently sent by worker: `receipt_kind=groth16`, `verify_mode=policy`, `segment_limit_po2`
- Auth headers: `x-api-key` and/or `CF-Access-Client-Id` + `CF-Access-Client-Secret`
- Response: `{ success, job_id, status, status_url }`

**Polling:**
- `GET {PROVER_BASE_URL}/api/jobs/{proverJobId}`
- Response shape: prover job record (`job_id`, `status`, `options`, optional `result`, optional `error`)
- Success state includes `status: "succeeded"` with `result: { proof, elapsed_ms }`
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
| `PROVER_BASE_URL` | `https://replace-with-your-prover.example.com` | Base URL for the prover API (typically a Cloudflare Tunnel URL) |
| `PROVER_API_KEY` | _(secret)_ | Required for remote prover endpoints unless using Cloudflare Access service-token auth (`wrangler secret put PROVER_API_KEY`) |
| `PROVER_ACCESS_CLIENT_ID` | _(secret)_ | Optional Cloudflare Access service token ID |
| `PROVER_ACCESS_CLIENT_SECRET` | _(secret)_ | Optional Cloudflare Access service token secret |
| `PROVER_EXPECTED_IMAGE_ID` | _unset_ | Optional image ID pin to prevent prover drift |
| `PROVER_HEALTH_CACHE_MS` | `"30000"` | Prover health cache TTL in Worker isolate |
| `PROVER_POLL_INTERVAL_MS` | `"3000"` | Alarm interval between polls |
| `PROVER_POLL_TIMEOUT_MS` | `"660000"` | 11 min poll-loop safety bound |
| `PROVER_POLL_BUDGET_MS` | `"45000"` | Max polling time per alarm |
| `PROVER_REQUEST_TIMEOUT_MS` | `"30000"` | HTTP request timeout |
| `MAX_TAPE_BYTES` | `"2097152"` | 2 MiB tape size limit |
| `MAX_JOB_WALL_TIME_MS` | `"660000"` | 11 min total job lifetime |
| `MAX_COMPLETED_JOBS` | `"200"` | Cap for completed job records retained in DO storage |
| `COMPLETED_JOB_RETENTION_MS` | `"86400000"` | Time-based retention cutoff for terminal jobs in DO storage |
| `ALLOW_INSECURE_PROVER_URL` | `"0"` | Enforce HTTPS |
| `RELAYER_URL` | `https://replace-with-your-claim-relay.example.com/submit` | Claim relay endpoint invoked after proof success |
| `RELAYER_API_KEY` | _(secret, optional)_ | Optional app-level auth for claim relay |
| `RELAYER_REQUEST_TIMEOUT_MS` | `"30000"` | Claim relay HTTP timeout |

Prover submit defaults (hardcoded in `worker/prover/client.ts`):
- `receipt_kind=groth16` (required for Stellar on-chain verification)
- `verify_mode=policy` (skip prover-side receipt verification)
- `segment_limit_po2=21`

Bindings:
- R2 bucket: `stellar-zk-proof-artifacts`
- Queue producer: `stellar-zk-proof-jobs`
- Queue producer: `stellar-zk-claim-jobs`
- Queue consumers: proof queue + proof DLQ + claim queue + claim DLQ
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
    rules_digest: u32,  // RULES_DIGEST = 0x41535433 ("AST3")
}
```

Serialized as 24 bytes (6 × u32 LE).

### Host prover behavior

`prove_tape` in `host/src/lib.rs`:
1. Validates dev-mode policy (`RISC0_DEV_MODE` + `proof_mode`).
2. Builds executor env with `max_frames`, original `tape_len`, padded tape, `segment_limit_po2`.
3. Proves using the requested receipt kind (`composite`, `succinct`, or `groth16`).
4. Detects produced receipt kind from receipt internals.
5. In non-dev mode, requires produced kind == requested kind.
6. Verifies receipt against `VERIFY_TAPE_ID` when `verify_mode=verify`.
7. Decodes and returns committed journal.

### API server (`api-server/src/main.rs`)

Endpoints:
- `GET /health`
- `POST /api/jobs/prove-tape/raw` (binary tape body)
- `GET /api/jobs/{job_id}`
- `DELETE /api/jobs/{job_id}`

Auth: `API_KEY` is required by default; `/api/*` requires `x-api-key` or `Authorization: Bearer` header.
For local-only development, set `ALLOW_MISSING_API_KEY=1` together with `RISC0_DEV_MODE=1`.

Job states: `queued` → `running` → `succeeded` | `failed`.
Single-flight enforced by global semaphore (concurrency 1) + active-job check.

Query param policy gates: `max_frames`, `receipt_kind`, `segment_limit_po2`,
`verify_mode`.

`proof_mode` is forced from `RISC0_DEV_MODE` at prover startup (not a query param).

Operational defaults: `MAX_TAPE_BYTES=2097152`, `MAX_JOBS=64`,
`JOB_TTL_SECS=86400`, `MAX_FRAMES=18000`, `MIN_SEGMENT_LIMIT_PO2=16`,
`MAX_SEGMENT_LIMIT_PO2=21`.

---

## Repository Paths

| Path | Purpose |
|------|---------|
| `worker/index.ts` | Worker entrypoint |
| `worker/api/routes.ts` | HTTP route handlers |
| `worker/queue/consumer.ts` | Queue consumers (proof dispatch + claim submit) |
| `worker/claim/direct.ts` | Channels relayer client (relayer-only) |
| `worker/claim/submit.ts` | Claim submit coordinator |
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
