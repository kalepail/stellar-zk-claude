# RISC0 Asteroids Proof API

REST API for generating and verifying Asteroids replay proofs from `.tape` payloads.

## Endpoints

- `GET /health`
- `POST /api/prove-tape` (JSON, base64 tape, synchronous)
- `POST /api/prove-tape/raw` (binary tape body, synchronous)
- `POST /api/jobs/prove-tape` (JSON, base64 tape, async job)
- `POST /api/jobs/prove-tape/raw` (binary tape body, async job)
- `GET /api/jobs/{job_id}`
- `DELETE /api/jobs/{job_id}`
- `POST /api/verify-receipt`

## Request and Response Notes

`/api/prove-tape` request body:

```json
{
  "tape_b64": "<base64_tape_bytes>",
  "receipt_kind": "composite",
  "max_frames": 18000,
  "segment_limit_po2": 19
}
```

Successful response:

```json
{
  "success": true,
  "proof": {
    "proof": {
      "journal": {
        "final_score": 1234,
        "frame_count": 500,
        "seed": 123456789,
        "final_rng_state": 987654321,
        "tape_checksum": 12345678,
        "rules_digest": 1096041521
      },
      "receipt": {"...": "..."},
      "requested_receipt_kind": "composite",
      "produced_receipt_kind": "composite",
      "stats": {
        "segments": 1,
        "total_cycles": 524288,
        "user_cycles": 518000,
        "paging_cycles": 6288,
        "reserved_cycles": 0
      }
    },
    "elapsed_ms": 2531
  }
}
```

## Quick Local Run

From `risc0-asteroids-verifier/`:

```bash
cargo run --release -p api-server
```

Health check:

```bash
curl -s http://127.0.0.1:8080/health | jq
```

Sync prove (raw binary endpoint):

```bash
curl -sS \
  -X POST 'http://127.0.0.1:8080/api/prove-tape/raw?receipt_kind=composite&segment_limit_po2=19' \
  --data-binary @../test-fixtures/test-short.tape \
  -H 'Content-Type: application/octet-stream' | jq
```

Async prove job flow:

```bash
JOB_ID=$(curl -sS \
  -X POST 'http://127.0.0.1:8080/api/jobs/prove-tape/raw?receipt_kind=composite' \
  --data-binary @../test-fixtures/test-short.tape \
  -H 'Content-Type: application/octet-stream' | jq -r '.job_id')

curl -sS "http://127.0.0.1:8080/api/jobs/${JOB_ID}" | jq
```

## Security Defaults

The server is locked down by default:

- `ALLOW_DEV_MODE_REQUESTS=false`
- `ALLOW_UNVERIFIED_RECEIPTS=false`
- `RISC0_DEV_MODE=0`
- `MAX_FRAMES=18000`
- `MAX_JOBS=64`
- `MAX_TAPE_BYTES=2097152`
- `MIN_SEGMENT_LIMIT_PO2=16`
- `MAX_SEGMENT_LIMIT_PO2=22`

These defaults prevent unsafe proving modes and bound resource usage for public deployment.

## Environment Variables

See `api-server/.env.example` for all options.

Most important knobs:

- `PROVER_CONCURRENCY`: parallel proof workers (start at `1` unless you have large CPU/GPU headroom)
- `MAX_JOBS`: max retained jobs in memory
- `MAX_TAPE_BYTES`: request payload cap
- `MAX_FRAMES`: upper bound for replay length
- `MIN_SEGMENT_LIMIT_PO2`, `MAX_SEGMENT_LIMIT_PO2`: allowed segment bounds

## Vast.ai Deployment (Docker)

### 1. Build image

On your Vast.ai instance:

```bash
git clone https://github.com/kalepail/stellar-zk-codex
cd stellar-zk-codex/risc0-asteroids-verifier

# GPU-enabled build (recommended on Vast.ai GPU instances)
docker build -f api-server/Dockerfile \
  --build-arg ENABLE_CUDA=1 \
  -t asteroids-zk-api:latest .
```

For CPU-only instances, use `--build-arg ENABLE_CUDA=0`.

### 2. Run API container

```bash
docker run -d \
  --name asteroids-zk-api \
  --restart unless-stopped \
  --gpus all \
  -p 8080:8080 \
  --env-file api-server/.env.example \
  -e RUST_LOG=info \
  -e PROVER_CONCURRENCY=1 \
  -e RISC0_DEV_MODE=0 \
  asteroids-zk-api:latest
```

### 3. Attach Cloudflare Tunnel

If `cloudflared` is installed on the host:

```bash
cloudflared tunnel --url http://127.0.0.1:8080
```

For long-running service mode with a named tunnel token:

```bash
cloudflared service install <YOUR_TUNNEL_TOKEN>
systemctl restart cloudflared
```

## Operational Recommendations

- Keep `RISC0_DEV_MODE=0` in production.
- Keep `ALLOW_UNVERIFIED_RECEIPTS=false` unless benchmarking internally.
- Start with `PROVER_CONCURRENCY=1` and increase only after measuring memory headroom.
- Use async job endpoints for long tapes to avoid client timeouts.
