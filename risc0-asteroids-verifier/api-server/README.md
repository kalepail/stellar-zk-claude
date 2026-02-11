# RISC0 Asteroids Proof API

Minimal REST API for generating Asteroids replay proofs from raw `.tape` bytes.

This server is intentionally single-flight:

- one proving job in flight at a time,
- no internal work queue,
- fast `429` when busy.

## Endpoints

- `GET /health`
- `POST /api/jobs/prove-tape/raw` (binary tape body, async job submit)
- `GET /api/jobs/{job_id}` (job status + proof on success)
- `DELETE /api/jobs/{job_id}` (optional cleanup)

## Auth

If `API_KEY` is set, all `/api/*` routes require either:

- `x-api-key: <API_KEY>`, or
- `Authorization: Bearer <API_KEY>`.

`/health` is always open.

`POST /api/jobs/prove-tape/raw` rejects zero-score tapes (`final_score == 0`)
with `400` and `error_code: "zero_score_not_allowed"`.

## Quick Local Run

From `risc0-asteroids-verifier/`:

```bash
RISC0_DEV_MODE=1 cargo run --release -p api-server
```

Health check:

```bash
curl -s http://127.0.0.1:8080/health | jq
```

Health includes prover identity fields (`image_id`, `rules_digest`, `rules_digest_hex`, `ruleset`)
so downstream services can verify they are targeting the expected prover build.

Submit a job:

```bash
JOB_ID=$(curl -sS \
  -X POST 'http://127.0.0.1:8080/api/jobs/prove-tape/raw?receipt_kind=composite&segment_limit_po2=21&verify_mode=policy' \
  --data-binary @../test-fixtures/test-medium.tape \
  -H 'Content-Type: application/octet-stream' \
  -H 'x-api-key: YOUR_API_KEY' | jq -r '.job_id')
```

Poll:

```bash
curl -sS \
  -H 'x-api-key: YOUR_API_KEY' \
  "http://127.0.0.1:8080/api/jobs/${JOB_ID}" | jq
```

## Environment Variables

See `.env.example` for full config.

Most relevant:

- `API_KEY`: optional shared secret for `/api/*`
- `MAX_TAPE_BYTES`: request payload cap
- `MAX_JOBS`: max retained jobs in SQLite metadata store
- `JOB_TTL_SECS`, `JOB_SWEEP_SECS`: retention + cleanup interval
- `MAX_FRAMES`: upper bound for replay length
- `MIN_SEGMENT_LIMIT_PO2`, `MAX_SEGMENT_LIMIT_PO2`: allowed segment bounds
- `HTTP_MAX_CONNECTIONS`: inbound socket ceiling
- `HTTP_KEEP_ALIVE_SECS`: keep-alive window
- `HTTP_WORKERS` (optional): explicit Actix worker count
- `CORS_ALLOWED_ORIGIN` (optional): allow browser access from one explicit origin (disabled by default)
- `RUNNING_JOB_TIMEOUT_SECS`: mark long-running proofs as timed out (default: 600s / 10 min)
- `TIMED_OUT_PROOF_KILL_SECS`: after timeout, abort process if proof task still has not returned (default: 60s; set `0` to disable)

Prover concurrency is fixed at `1` in code.

## Security Defaults

- `RISC0_DEV_MODE=0` in production (secure proving), `RISC0_DEV_MODE=1` only for local/dev
- `verify_mode` defaults to `policy` (verification happens on-chain)

These defaults keep proving in production-safe mode.

For timeout recovery in production, run the server under a process supervisor (supervisord, Docker restart policy, Kubernetes, etc.) so process aborts are automatically restarted.

## Vast.ai Deployment

See the parent [README.md](../README.md) for full Vast.ai setup, build, run, supervisord setup, and Cloudflare Tunnel instructions.

## Cloudflare Tunnel

The tunnel is managed by supervisord for automatic restart. See the parent [README.md](../README.md) for setup. For production, use a named tunnel and pair with Cloudflare Access service tokens, keeping `API_KEY` enabled as app-level defense in depth.
