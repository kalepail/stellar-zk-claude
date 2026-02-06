# RISC0 Asteroids Verifier

ZK proof generation for deterministic Asteroids game replays using RISC Zero.

## Architecture

```
┌─────────────────────┐     ┌──────────────────────────────┐
│  Cloudflare Worker   │     │  Vast.ai GPU Instance        │
│                      │     │                              │
│  POST /api/proofs/   │────>│  POST /api/jobs/prove-tape/  │
│       jobs           │     │       raw                    │
│                      │     │                              │
│  Queue + Coordinator │     │  api-server (Actix-web)      │
│  + R2 artifacts      │<────│  + host prover               │
│                      │poll │  + RISC Zero zkVM            │
└─────────────────────┘     └──────────────────────────────┘
```

The Cloudflare Worker accepts game tapes from the frontend, stores them in R2, and dispatches proving jobs to the RISC0 api-server running on a Vast.ai GPU instance. The api-server runs the tape through the zkVM guest, generates a proof, and returns the result.

## Workspace Layout

| Crate | Purpose |
|-------|---------|
| `asteroids-core/` | Shared deterministic game engine (`no_std`), bit-for-bit matches the TypeScript implementation |
| `methods/guest/` | RISC Zero guest: reads tape, replays game, commits journal (seed, score, frames) |
| `host/` | Proving runner library + CLI binary + benchmark binary |
| `api-server/` | Actix-web HTTP API for async proving jobs |

## Toolchain

- Rust: 1.93.0 (pinned in `rust-toolchain.toml`)
- RISC Zero: 3.0.5 (`risc0-zkvm`)
- CUDA: 12.9.1 (for GPU-accelerated proving on Vast.ai)

## Setup on Vast.ai

### 1. Provision an instance

Rent a GPU instance on [Vast.ai](https://vast.ai) with:
- Ubuntu 22.04 or 24.04
- CUDA 12.x drivers
- At least 32 GB RAM recommended
- SSH access enabled

### 2. Run the setup script

The `VASTAI` script installs all dependencies (system packages, Rust, RISC Zero toolchain) and clones the repo. It does **not** compile or start anything.

```bash
# From the Vast.ai instance (as root):
curl -sSf https://raw.githubusercontent.com/kalepail/stellar-zk-claude/main/risc0-asteroids-verifier/VASTAI | bash
```

Or if you prefer to clone first:

```bash
git clone https://github.com/kalepail/stellar-zk-claude.git /workspace/stellar-zk
bash /workspace/stellar-zk/risc0-asteroids-verifier/VASTAI
```

**Environment variables for the setup script:**

| Variable | Default | Purpose |
|----------|---------|---------|
| `REPO_URL` | `https://github.com/kalepail/stellar-zk-claude.git` | Git remote |
| `WORKDIR` | `/workspace/stellar-zk` | Clone destination |
| `GIT_REF` | `main` | Branch, tag, or commit to checkout |
| `RUST_TOOLCHAIN_VERSION` | `1.93.0` | Rust version to install |
| `INSTALL_CLOUDFLARED` | `0` | Set to `1` to install Cloudflare Tunnel |

### 3. Build the api-server

SSH into your instance and build:

```bash
cd /workspace/stellar-zk/risc0-asteroids-verifier

# With CUDA acceleration (recommended for Vast.ai GPU instances):
cargo build --locked --release -p api-server --features cuda

# CPU-only (for testing without GPU):
cargo build --locked --release -p api-server
```

The first build will take a while (~15-30 min depending on hardware) because it compiles the RISC Zero zkVM guest ELF. Subsequent builds use incremental compilation and are much faster.

The release binary is at `target/release/api-server`.

### 4. Run the api-server

```bash
cd /workspace/stellar-zk/risc0-asteroids-verifier

# Minimal (with API key auth):
API_KEY='your-strong-random-secret' cargo run --release -p api-server

# Or run the binary directly:
API_KEY='your-strong-random-secret' ./target/release/api-server

# With full env config:
API_KEY='your-strong-random-secret' \
RUST_LOG=info \
RISC0_DEV_MODE=0 \
MAX_FRAMES=18000 \
cargo run --release -p api-server
```

Verify it's running:

```bash
curl -s http://127.0.0.1:8080/health | jq
```

### 5. Expose via Cloudflare Tunnel

Install cloudflared (if not done during setup):

```bash
# Install
mkdir -p --mode=0755 /usr/share/keyrings
curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | \
  tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null
echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' \
  > /etc/apt/sources.list.d/cloudflared.list
apt-get update -qq && apt-get install -y cloudflared
```

Start the tunnel:

```bash
# Quick tunnel (generates a temporary *.trycloudflare.com URL):
cloudflared tunnel --url http://127.0.0.1:8080
```

The tunnel will print a URL like `https://something-random.trycloudflare.com`. Use this as the `PROVER_BASE_URL` in your Cloudflare Worker config.

For a persistent named tunnel, see the [Cloudflare Tunnel docs](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/).

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/health` | No | Server health + config summary |
| `POST` | `/api/jobs/prove-tape/raw` | Yes | Submit a tape for proving (binary body) |
| `GET` | `/api/jobs/{job_id}` | Yes | Get job status + result |
| `DELETE` | `/api/jobs/{job_id}` | Yes | Delete a finished job |

### Authentication

If `API_KEY` is set, all `/api/*` routes require either:
- `x-api-key: <API_KEY>` header, or
- `Authorization: Bearer <API_KEY>` header.

`/health` is always open.

### Submit a proof job

```bash
JOB_ID=$(curl -sS \
  -X POST 'http://127.0.0.1:8080/api/jobs/prove-tape/raw?receipt_kind=composite&segment_limit_po2=19' \
  --data-binary @../test-fixtures/test-short.tape \
  -H 'Content-Type: application/octet-stream' \
  -H 'x-api-key: YOUR_API_KEY' | jq -r '.job_id')

echo "Job ID: ${JOB_ID}"
```

**Query parameters:**

| Parameter | Default | Description |
|-----------|---------|-------------|
| `receipt_kind` | `composite` | `composite`, `succinct`, or `groth16` |
| `segment_limit_po2` | `19` | Segment size (2^n), range [16..22] |
| `max_frames` | `18000` | Max game frames to replay |
| `allow_dev_mode` | `false` | Allow dev-mode proving (disabled by policy) |
| `verify_receipt` | `true` | Verify the receipt after generation |

### Poll for completion

```bash
curl -sS \
  -H 'x-api-key: YOUR_API_KEY' \
  "http://127.0.0.1:8080/api/jobs/${JOB_ID}" | jq
```

**Job statuses:** `queued` -> `running` -> `succeeded` | `failed`

The server is single-flight: only one proving job runs at a time. New submissions return `429` while a job is active.

### Successful result shape

```json
{
  "job_id": "uuid",
  "status": "succeeded",
  "result": {
    "proof": {
      "journal": {
        "seed": 12345,
        "frame_count": 500,
        "final_score": 100,
        "final_rng_state": 67890,
        "tape_checksum": 11111,
        "rules_digest": 22222
      },
      "receipt": { "..." },
      "requested_receipt_kind": "composite",
      "produced_receipt_kind": "composite",
      "stats": {
        "segments": 4,
        "total_cycles": 1048576,
        "user_cycles": 800000,
        "paging_cycles": 200000,
        "reserved_cycles": 48576
      }
    },
    "elapsed_ms": 45000
  }
}
```

## Environment Variables

See `api-server/.env.example` for all options. Key variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `API_BIND_ADDR` | `0.0.0.0:8080` | Listen address |
| `API_KEY` | _(empty)_ | Shared secret for `/api/*` auth |
| `RUST_LOG` | `info` | Log level |
| `RISC0_DEV_MODE` | `0` | Set to `1` for fake proofs (testing only) |
| `MAX_TAPE_BYTES` | `2097152` | Max tape payload size (2 MB) |
| `MAX_JOBS` | `64` | Max retained jobs in memory |
| `MAX_FRAMES` | `18000` | Max game frames for replay |
| `MIN_SEGMENT_LIMIT_PO2` | `16` | Min allowed segment limit |
| `MAX_SEGMENT_LIMIT_PO2` | `22` | Max allowed segment limit |
| `JOB_TTL_SECS` | `86400` | Finished job retention (24h) |
| `JOB_SWEEP_SECS` | `60` | Cleanup interval |
| `HTTP_MAX_CONNECTIONS` | `25000` | Max inbound connections |
| `HTTP_KEEP_ALIVE_SECS` | `75` | Keep-alive timeout |
| `HTTP_WORKERS` | _(auto)_ | Actix worker thread count |
| `ALLOW_DEV_MODE_REQUESTS` | `false` | Allow `allow_dev_mode=true` query param |
| `ALLOW_UNVERIFIED_RECEIPTS` | `false` | Allow `verify_receipt=false` query param |

## Connecting the Cloudflare Worker

The Cloudflare Worker (`worker/`) proxies frontend proof requests to this api-server. Configure it via `wrangler.jsonc` vars or `wrangler secret`:

| Worker Setting | Description |
|----------------|-------------|
| `PROVER_BASE_URL` | Your tunnel URL (e.g., `https://xyz.trycloudflare.com`) |
| `PROVER_API_KEY` (secret) | Must match the `API_KEY` on the api-server |
| `PROVER_ACCESS_CLIENT_ID` (secret) | _(optional)_ Cloudflare Access service token ID |
| `PROVER_ACCESS_CLIENT_SECRET` (secret) | _(optional)_ Cloudflare Access service token secret |
| `PROVER_RECEIPT_KIND` | `composite` (should match api-server policy) |
| `PROVER_SEGMENT_LIMIT_PO2` | `19` (must be within api-server's [min, max] range) |
| `PROVER_MAX_FRAMES` | `18000` (must be <= api-server's MAX_FRAMES) |

Set the secrets:

```bash
# From the repo root:
echo 'your-strong-random-secret' | npx wrangler secret put PROVER_API_KEY
echo 'https://xyz.trycloudflare.com' | npx wrangler secret put PROVER_BASE_URL
```

The worker submits tapes as `POST /api/jobs/prove-tape/raw` with `x-api-key` header, then polls `GET /api/jobs/{id}` until the status is `succeeded` or `failed`.

## Docker (alternative)

If you prefer Docker instead of native compilation:

### Build

```bash
cd risc0-asteroids-verifier

# GPU (CUDA):
docker build -f api-server/Dockerfile \
  --build-arg ENABLE_CUDA=1 \
  -t asteroids-zk-api:latest .

# CPU only:
docker build -f api-server/Dockerfile \
  --build-arg ENABLE_CUDA=0 \
  -t asteroids-zk-api:latest .
```

### Run

```bash
docker run -d \
  --name asteroids-zk-api \
  --restart unless-stopped \
  --gpus all \
  -p 8080:8080 \
  --env-file api-server/.env.example \
  -e API_KEY='your-strong-random-secret' \
  -e RISC0_DEV_MODE=0 \
  asteroids-zk-api:latest
```

## CLI Prover

For local testing without the HTTP API:

```bash
cd risc0-asteroids-verifier

# Dev mode (fast, fake proof):
RISC0_DEV_MODE=1 cargo run -p host --release -- --allow-dev-mode --tape ../test-fixtures/test-medium.tape

# Real proof:
RISC0_DEV_MODE=0 cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape

# With journal output:
cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape --journal-out ./journal.json

# Execute-only benchmark (no proof):
cargo run -p host --release --bin benchmark -- --tape ../test-fixtures/test-medium.tape
```

## Tests

```bash
cargo test -p asteroids-verifier-core
```
