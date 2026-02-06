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

## Quick Local Run

From `risc0-asteroids-verifier/`:

```bash
cargo run --release -p api-server
```

Health check:

```bash
curl -s http://127.0.0.1:8080/health | jq
```

Submit a job:

```bash
JOB_ID=$(curl -sS \
  -X POST 'http://127.0.0.1:8080/api/jobs/prove-tape/raw?receipt_kind=composite&segment_limit_po2=19' \
  --data-binary @../test-fixtures/test-short.tape \
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
- `MAX_JOBS`: max retained jobs in memory
- `JOB_TTL_SECS`, `JOB_SWEEP_SECS`: retention + cleanup interval
- `MAX_FRAMES`: upper bound for replay length
- `MIN_SEGMENT_LIMIT_PO2`, `MAX_SEGMENT_LIMIT_PO2`: allowed segment bounds
- `HTTP_MAX_CONNECTIONS`: inbound socket ceiling
- `HTTP_KEEP_ALIVE_SECS`: keep-alive window
- `HTTP_WORKERS` (optional): explicit Actix worker count

Prover concurrency is fixed at `1` in code.

## Security Defaults

- `ALLOW_DEV_MODE_REQUESTS=false`
- `ALLOW_UNVERIFIED_RECEIPTS=false`
- `RISC0_DEV_MODE=0`

These defaults keep proving in production-safe mode.

## Vast.ai Deployment

See the parent [README.md](../README.md) for full Vast.ai setup, build, run, and Cloudflare Tunnel instructions.

## Docker (alternative)

### Build

```bash
cd risc0-asteroids-verifier

docker build -f api-server/Dockerfile \
  --build-arg ENABLE_CUDA=1 \
  -t asteroids-zk-api:latest .
```

For CPU-only instances, use `--build-arg ENABLE_CUDA=0`.

### Run

```bash
docker run -d \
  --name asteroids-zk-api \
  --restart unless-stopped \
  --gpus all \
  -p 8080:8080 \
  --env-file api-server/.env.example \
  -e API_KEY='replace-with-strong-random-secret' \
  -e RUST_LOG=info \
  -e RISC0_DEV_MODE=0 \
  asteroids-zk-api:latest
```

### Cloudflare Tunnel

```bash
cloudflared tunnel --url http://127.0.0.1:8080
```

For production, pair Tunnel with Cloudflare Access service tokens and keep `API_KEY` enabled as app-level defense in depth.
