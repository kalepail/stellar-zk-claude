# Provisioning & Timeout Audit

Comprehensive inventory of every timeout, timing gate, and retry setting across the prover API server and Cloudflare Worker proof gateway. Recommended settings target **~5 minute typical proofs** while accepting up to **10 minutes** end-to-end in production.

---

## Table of Contents

1. [Current Timeout Inventory](#current-timeout-inventory)
   - [Prover API Server](#prover-api-server-risc0-asteroids-verifier)
   - [Cloudflare Worker Gateway](#cloudflare-worker-gateway)
   - [Benchmark Timing Gates](#benchmark-timing-gates)
2. [Proposed Changes](#proposed-changes)
3. [Unchanged Settings](#unchanged-settings)
4. [Timeout Flow Diagram](#timeout-flow-diagram)
5. [Deploy Order](#deploy-order)
6. [Verification Steps](#verification-steps)
7. [Risk Analysis](#risk-analysis)

---

## Current Timeout Inventory

### Prover API Server (`risc0-asteroids-verifier/`)

| Setting | Current Value | Env Var | File | Purpose |
|---------|--------------|---------|------|---------|
| Running job timeout | **10 min** (600 s) | `RUNNING_JOB_TIMEOUT_SECS` | `api-server/src/main.rs:33` | `tokio::select!` deadline around `prove_tape()`. When exceeded, the proof task is detached and the job is marked failed with `error_code=proof_timeout`. |
| Timed-out proof kill | **60 s** | `TIMED_OUT_PROOF_KILL_SECS` | `api-server/src/main.rs:38` | After a timeout, a background task waits this long for the detached proof to finish. If it doesn't, calls `std::process::abort()` so the supervisor (supervisord) restarts the process. Set to `0` to wait forever (not recommended). |
| Job TTL | **24 h** (86400 s) | `JOB_TTL_SECS` | `api-server/src/main.rs:30` | Completed/failed jobs are swept from SQLite after this duration. |
| Job sweep interval | **60 s** | `JOB_SWEEP_SECS` | `api-server/src/main.rs:31` | How often the background cleanup task runs `store.sweep()`. |
| Max jobs | **64** | `MAX_JOBS` | `api-server/src/main.rs:32` | Maximum stored jobs before oldest is evicted on new submission. |
| Prover concurrency | **1** (hardcoded) | — | `api-server/src/main.rs:29` | Tokio semaphore permits. Only one proof runs at a time. |
| HTTP keep-alive | **75 s** | `HTTP_KEEP_ALIVE_SECS` | `api-server/src/main.rs:37` | Actix-web keep-alive timeout for idle connections. |
| HTTP max connections | **25,000** | `HTTP_MAX_CONNECTIONS` | `api-server/src/main.rs:36` | Actix-web connection limit. |
| Max tape bytes | **2 MiB** | `MAX_TAPE_BYTES` | `api-server/src/main.rs:28` | Request body size limit for tape uploads. |
| Max frames | **18,000** | `MAX_FRAMES` | `api-server/.env.example:20` | Capped at game-engine constant `MAX_FRAMES_DEFAULT`. Passed to guest prover. |
| Segment limit po2 | **21** (range 16–21) | `MIN/MAX_SEGMENT_LIMIT_PO2` | `api-server/src/main.rs:34-35` | RISC Zero segment size. Higher = fewer segments but more memory. |

**Relevant code paths:**
- Timeout + detach + abort: `api-server/src/main.rs:348-467` (`run_proof_job`)
- Sweep task: `api-server/src/main.rs:469-484` (`spawn_job_cleanup_task`)
- Sweep SQL: `api-server/src/store.rs:521-572` (deletes running jobs older than `running_timeout_secs`, completed jobs older than `ttl_secs`)

### Cloudflare Worker Gateway (`worker/`)

| Setting | Current Value | Env Var | File | Purpose |
|---------|--------------|---------|------|---------|
| Poll timeout (absolute) | **11 min** (660,000 ms) | `PROVER_POLL_TIMEOUT_MS` | `constants.ts:10`, `wrangler.jsonc:15` | Safety bound for a single `pollProver()` loop. In practice the DO alarm cadence is governed by `PROVER_POLL_BUDGET_MS` + `PROVER_POLL_INTERVAL_MS`. |
| Poll interval | **3 s** (3,000 ms) | `PROVER_POLL_INTERVAL_MS` | `constants.ts:9`, `wrangler.jsonc:14` | Sleep between GET status calls inside `pollProver()`. |
| Poll budget | **45 s** (45,000 ms) | `PROVER_POLL_BUDGET_MS` | `constants.ts:12`, `wrangler.jsonc:16` | Per-alarm-invocation budget. The DO alarm fires, polls for up to 45 s, then yields and reschedules. Keeps each alarm invocation under the CF CPU limit. |
| HTTP request timeout | **30 s** (30,000 ms) | `PROVER_REQUEST_TIMEOUT_MS` | `constants.ts:11`, `wrangler.jsonc:17` | `fetchWithTimeout` deadline for each individual GET/POST to the prover API. |
| Wall-time cap | **11 min** (660,000 ms) | `MAX_JOB_WALL_TIME_MS` | `constants.ts:13`, `wrangler.jsonc:19` | Hard ceiling on total job lifetime. Checked both in the queue consumer (`consumer.ts:39-48`) and the DO alarm loop (`coordinator.ts:434-449`). |
| Max queue retries | **10** | `MAX_QUEUE_RETRIES` / `max_retries` | `constants.ts:22`, `wrangler.jsonc:42` | Queue delivery attempts before the message goes to the DLQ. Must match in both places. |
| Retry delay cap | **60 s** | `MAX_RETRY_DELAY_SECONDS` | `constants.ts:17` | Ceiling for exponential backoff: `min(2^(attempt-1), 60)`, floored at 2 s. |
| Max completed jobs | **200** | `MAX_COMPLETED_JOBS` | `constants.ts:14`, `wrangler.jsonc:20` | DO evicts oldest completed jobs beyond this count. |
| Completed job retention | **24 h** (86,400,000 ms) | `COMPLETED_JOB_RETENTION_MS` | `constants.ts:15`, `wrangler.jsonc:21` | TTL for completed job records in DO storage. |
| Max tape bytes | **2 MiB** | `MAX_TAPE_BYTES` | `constants.ts:8`, `wrangler.jsonc:18` | Must match prover-side limit. |
| Queue batch size | **1** | — | `wrangler.jsonc:40` | One message per batch. Matches single-flight semantics. |
| Queue batch timeout | **1 s** | — | `wrangler.jsonc:41` | Max wait before delivering a partial batch. |
| DLQ max retries | **3** | — | `wrangler.jsonc:49` | Retries for the dead-letter queue consumer itself. |

**Relevant code paths:**
- `pollProver()`: `prover/client.ts:182-315` — budget/absolute deadline loop
- DO alarm handler: `durable/coordinator.ts:434-584` — wall-time check, poll dispatch, retry backoff
- Queue consumer: `queue/consumer.ts:19-48` — wall-time check before submitting to prover

### Benchmark Timing Gates

| Gate | Current Value | File | Purpose |
|------|--------------|------|---------|
| Dev short (CPU) | **2.0 s** | `benchmarks/thresholds.env:7` | CI regression gate for dev-mode short tape |
| Dev medium (CPU) | **2.0 s** | `benchmarks/thresholds.env:8` | CI regression gate for dev-mode medium tape |
| Dev short cycles | **700,000** | `benchmarks/thresholds.env:11` | CI regression gate for execute-only cycle counts |
| Dev medium cycles | **7,000,000** | `benchmarks/thresholds.env:12` | CI regression gate for execute-only cycle counts |

These are CI quality gates, not runtime settings. Secure proving latency should be
measured on the actual prover host (CUDA vs CPU) with `scripts/bench-segment-sweep.sh`
and used to set the 10-minute acceptance window (`RUNNING_JOB_TIMEOUT_SECS`) plus
the gateway overhead (`MAX_JOB_WALL_TIME_MS`).

---

## Proposed Changes

Configuration changes to tighten the pipeline around a 10-minute acceptance window. Values are environment-variable overridable unless noted.

### 1. Prover: `RUNNING_JOB_TIMEOUT_SECS` — 1800 → 600

| | |
|---|---|
| **File** | `api-server/.env.example:23` (and deployed `.env`) |
| **Old** | `1800` (30 min) |
| **New** | `600` (10 min) |
| **Rationale** | Typical Groth16 proofs are ~5 minutes on CUDA. A 10-minute timeout provides ~2x headroom for variance while keeping single-flight capacity from being blocked indefinitely. If a proof can't complete in 10 minutes on your production GPU, treat it as stuck and fail fast. |

### 2. Prover: `TIMED_OUT_PROOF_KILL_SECS` — 120 → 60

| | |
|---|---|
| **File** | `api-server/.env.example:26` (and deployed `.env`) |
| **Old** | `120` (2 min) |
| **New** | `60` (1 min) |
| **Rationale** | Grace period for the detached proof to finish after timeout. With a 10-minute timeout, an additional 2-minute wait before `abort()` is excessive. 60 s is enough for orderly cleanup. Total time from proof start to forced restart: 10 min + 1 min = 11 min. |

### 3. Worker: `PROVER_POLL_TIMEOUT_MS` — 900000 → 660000

| | |
|---|---|
| **File** | `wrangler.jsonc:15` |
| **Old** | `900000` (15 min) |
| **New** | `660000` (11 min) |
| **Rationale** | Keep this above the prover timeout (10 min) to give the worker time to observe the terminal status. 11 minutes provides a small buffer for network jitter and a final poll cycle. |

### 4. Worker: `MAX_JOB_WALL_TIME_MS` — 3600000 → 660000

| | |
|---|---|
| **File** | `wrangler.jsonc:19` |
| **Old** | `3600000` (60 min) |
| **New** | `660000` (11 min) |
| **Rationale** | End-to-end safety net. Set to ~11 minutes = prover timeout (10 min) + poll/R2 overhead + short retry headroom for transient network/prover issues. |

### 5. Worker: `PROVER_POLL_BUDGET_MS` — 45000 → 45000 (consider 30000)

| | |
|---|---|
| **File** | `wrangler.jsonc:16` |
| **Old** | `45000` (45 s) |
| **New** | `45000` (45 s) — **optional reduction to 30000** |
| **Rationale** | The per-alarm poll budget is already well-tuned. 45 s allows ~15 poll cycles at the 3 s interval, which is a good balance between responsiveness and alarm overhead. A reduction to 30 s (~10 cycles) is safe but offers marginal benefit. **Keep at 45 s unless you observe alarm CPU limits being hit.** |

### 6. Worker: `MAX_RETRY_DELAY_SECONDS` — 300 → 60

| | |
|---|---|
| **File** | `worker/constants.ts` |
| **Old** | `300` (5 min) |
| **New** | `60` (1 min) |
| **Rationale** | With a 10-minute proving SLA, a 5-minute retry backoff just burns the acceptance window. Cap backoff at 60s so transient failures recover quickly (or fail fast) without hammering the prover. |

### 7. (No change needed) `DEFAULT_POLL_INTERVAL_MS` stays at 3000

Included here for completeness. See [Unchanged Settings](#unchanged-settings).

---

## Unchanged Settings

These settings are correct at their current values and should **not** be changed.

| Setting | Value | Why it stays |
|---------|-------|-------------|
| `PROVER_POLL_INTERVAL_MS` | 3,000 ms | 3 s is a good balance: fast enough to detect completion quickly, slow enough to avoid hammering the prover API. |
| `PROVER_REQUEST_TIMEOUT_MS` | 30,000 ms | Individual HTTP request timeout. 30 s covers slow responses without affecting the overall proof timeout. |
| `JOB_TTL_SECS` | 86,400 s (24 h) | Completed job retention on the prover. Independent of proof duration. |
| `JOB_SWEEP_SECS` | 60 s | Sweep frequency. Already minimal. |
| `MAX_JOBS` | 64 | Queue depth. Independent of timing. |
| `MAX_QUEUE_RETRIES` | 10 | Delivery attempts. The wall-time cap now provides a tighter bound, but retries are still useful for transient failures. |
| `MAX_RETRY_DELAY_SECONDS` | 60 s | Backoff ceiling. With a 12-min wall-time cap, the worst-case backoff sequence (2, 2, 4, 8, 16, 32, 60, 60, 60, 60) sums to ~304 s, leaving enough headroom for transient retries without consuming the full acceptance window. |
| `COMPLETED_JOB_RETENTION_MS` | 86,400,000 ms (24 h) | DO storage retention. Independent of proof duration. |
| `MAX_COMPLETED_JOBS` | 200 | DO storage cap. Independent of timing. |
| `HTTP_KEEP_ALIVE_SECS` | 75 s | Actix-web idle connection timeout. Independent of proof duration. |
| `FIXED_PROVER_CONCURRENCY` | 1 | Single-flight proving. Cannot change without architecture rework. |

---

## Timeout Flow Diagram

### Happy path (proof completes in ~90 s)

```
t=0s     Queue consumer receives message
         ├── Wall-time check: age < 720,000 ms? ✓
         ├── POST tape to prover API
         └── Prover accepts → markProverAccepted() → schedule alarm

t=0s     Prover: tokio::select! { prove_tape() vs sleep(600s) }
         └── prove_tape() starts on GPU

t=3s     DO alarm fires → pollProver() budget=45s
         ├── GET /api/jobs/{id} → "running"
         ├── sleep(3s), GET → "running"
         ├── ... (repeat ~14 times over 45s)
         └── Budget exhausted → return "running" → reschedule alarm in 3s

t=48s    DO alarm fires → pollProver() budget=45s
         ├── GET /api/jobs/{id} → "running"
         ├── sleep(3s), GET → "running"
         └── ...

t=90s    Prover: prove_tape() returns success ← proof done
         └── Job marked "succeeded" in SQLite

t=93s    DO alarm fires → pollProver()
         ├── GET /api/jobs/{id} → "succeeded" + result payload
         └── Store to R2, markSucceeded() ← worker done
```

### Timeout path (proof stuck)

```
t=0s     Queue consumer → POST tape → prover accepts
         Prover: tokio::select! { prove_tape() vs sleep(600s) }

t=3s     DO alarm → poll → "running" → reschedule
t=48s    DO alarm → poll → "running" → reschedule
         ...
t=600s   Prover: sleep(600s) fires → proof_timeout error
         ├── Job marked "failed" with error_code="proof_timeout"
         ├── Detached proof task continues in background
         └── Kill timer starts: 60s until abort()

t=603s   DO alarm → poll → GET returns "failed" (proof_timeout)
         ├── error_code in RETRYABLE_JOB_ERROR_CODES? NO
         └── Mark job failed and release the active slot

t=660s   Prover: if proof still running after 60s grace →
         └── std::process::abort() → supervisord restarts process

t=720s   Worker: wall-time cap (MAX_JOB_WALL_TIME_MS)
         └── Hard kill regardless of retry state
```

### Retry backoff sequence

The worker uses exponential backoff with a 60 s ceiling:

| Attempt | Delay (s) | Cumulative (s) |
|---------|-----------|-----------------|
| 1 | 2 | 2 |
| 2 | 2 | 4 |
| 3 | 4 | 8 |
| 4 | 8 | 16 |
| 5 | 16 | 32 |
| 6 | 32 | 64 |
| 7 | 60 | 124 |
| 8 | 60 | 184 |
| 9 | 60 | 244 |
| 10 | 60 | 304 |

With a 12-min (720 s) wall-time cap, retries stay bounded and responsive; the wall-time cap remains the ultimate guardrail.

---

## Deploy Order

### Step 1: Deploy Prover API (Vast.ai)

Update `.env` on the Vast.ai instance:

```env
RUNNING_JOB_TIMEOUT_SECS=600
TIMED_OUT_PROOF_KILL_SECS=60
```

Restart via supervisord:

```bash
supervisorctl restart risc0-asteroids-api
```

**Why prover first:** The worker gateway can tolerate a tighter prover timeout. If the prover times out at 10 minutes while the worker still has looser settings, the worker simply observes `proof_timeout` and fails the job — no breakage. The reverse (worker first) risks the worker failing jobs while the prover is still willing to run much longer proofs, which is harmless but wastes prover capacity.

### Step 2: Deploy Worker (Cloudflare)

Update `wrangler.jsonc`:

```jsonc
"PROVER_POLL_TIMEOUT_MS": "660000",
"MAX_JOB_WALL_TIME_MS": "660000"
```

Deploy:

```bash
npx wrangler deploy
```

**Why worker second:** By now the prover is already enforcing the 10-minute timeout. The worker update tightens its own safety nets to match.

---

## Verification Steps

### After prover deploy

1. **Health check** — confirm timeout and rules metadata are reported:
   ```bash
   curl -s https://risc0-kalien.stellar.buzz/health | jq '{accelerator, timed_out_proof_kill_secs, ruleset, rules_digest_hex, image_id}'
   ```
   Expected: `accelerator` is `"cuda"`, timeout configured correctly, and `ruleset` / `rules_digest_hex` match the deployed contract + worker expectations.

2. **Submit a known-good tape** — verify it completes well under 10 min:
   ```bash
   curl -X POST 'https://risc0-kalien.stellar.buzz/api/jobs/prove-tape/raw?receipt_kind=groth16&verify_mode=policy&segment_limit_po2=21' \
     -H "x-api-key: $API_KEY" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @test-fixtures/test-medium.tape
   ```
   Poll the returned `status_url` and confirm `"status": "succeeded"` with `elapsed_ms < 600000`.

3. **Check logs for sweep behavior** — the sweep should now reap running jobs older than 600 s:
   ```bash
   tail -n 200 /var/lib/stellar-zk/prover/api-server.log | rg sweep
   ```

### After worker deploy

4. **Submit a job via the worker** and confirm end-to-end success:
   ```bash
   curl -X POST https://your-worker.example.com/api/proofs/jobs \
     -H "x-claimant-address: GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @test-fixtures/test-medium.tape
   ```

5. **Verify wrangler vars** — confirm the deployed values:
   ```bash
   npx wrangler vars list
   ```

6. **Observe alarm cadence** — in the CF dashboard, check the DO alarm logs to confirm alarms fire at 3 s intervals and the poll budget stays under 45 s.

---

## Risk Analysis

### 1. CPU fallback (no CUDA)

**Risk:** If the prover loses GPU access (driver failure, wrong instance type), proofs fall back to CPU. CPU proving can exceed the 10-minute acceptance window and will start hitting `proof_timeout`.

**Mitigation:** The prover health endpoint reports `"accelerator": "cuda"` or `"cpu"`. The worker (or a monitoring script) should check this and alert if the accelerator is not `cuda`. CPU proving is not viable for production — a GPU failure should block new proof submissions, not silently time out.

**Action:** Consider adding a worker-side check that rejects submissions when the prover reports `"accelerator": "cpu"`.

### 2. Max-frame games (18,000 frames)

**Risk:** A full 18,000-frame game produces the largest tapes and longest proof times. If your workload includes many max-length tapes, you should benchmark worst-case prove latency on the production GPU and size the timeout accordingly.

**Mitigation:** The 10-minute timeout provides headroom for variance while keeping single-flight capacity from being pinned indefinitely. If specific tapes consistently time out, investigate guest execution cost and/or segment limits instead of blindly raising timeouts.

**Action:** Monitor `elapsed_ms` on succeeded proofs. If any approach 540 s (90% of timeout), investigate before they start timing out.

### 3. Deploy ordering mismatch

**Risk:** If the worker is deployed first with a tight wall-time cap (~11 min) while the prover still allows much longer proofs, a slow proof could be killed by the worker before the prover times it out. The prover would continue working on a proof that nobody is waiting for.

**Mitigation:** Deploy prover first (see [Deploy Order](#deploy-order)). Even if the ordering is reversed, the impact is limited: the worker kills its job tracking, but the prover eventually times out and sweeps the orphaned job. No data loss occurs.

### 4. Retry storm after prover restart

**Risk:** If the prover `abort()`s due to a stuck proof (after 10 min + 1 min grace), supervisord restarts it. During restart (~10 s), the worker may see connection errors or 404s, triggering retries with backoff.

**Mitigation:** The prover is single-flight (concurrency = 1). The worker's retry backoff is capped and the gateway wall-time cap is the ultimate guardrail, preventing infinite re-submission loops.
