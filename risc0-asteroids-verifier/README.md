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

Or if you prefer to clone first (note: set `WORKDIR` to match your clone path):

```bash
git clone https://github.com/kalepail/stellar-zk-claude.git /workspace/stellar-zk-claude
WORKDIR=/workspace/stellar-zk-claude bash /workspace/stellar-zk-claude/risc0-asteroids-verifier/VASTAI
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

SSH into your instance and build (replace the path with your actual `WORKDIR`):

```bash
cd <WORKDIR>/risc0-asteroids-verifier

# Default build (includes CUDA acceleration):
cargo build --locked --release -p api-server

# CPU-only (for testing without GPU):
cargo build --locked --release -p api-server --no-default-features
```

The first build will take a while (~15-30 min depending on hardware) because it compiles the RISC Zero zkVM guest ELF. Subsequent builds use incremental compilation and are much faster.

The release binary is at `target/release/api-server`.

### 4. Run the api-server

```bash
cd <WORKDIR>/risc0-asteroids-verifier

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

Verify it's running and confirm the accelerator:

```bash
curl -s http://127.0.0.1:8080/health | jq '.accelerator'
# Should print "cuda" on GPU instances, "cpu" if built with --no-default-features
```

### 5. Recommended production run (supervisord)

The API server intentionally aborts the process if a timed-out proof remains stuck after the grace window (`TIMED_OUT_PROOF_KILL_SECS`). Running under supervisord ensures automatic recovery. Vast.ai containers do not have systemd, so supervisord is the recommended process manager.

```bash
cd <WORKDIR>/risc0-asteroids-verifier

# 1) Install config
mkdir -p /etc/stellar-zk /var/lib/stellar-zk/prover
cp deploy/supervisord/risc0-asteroids-api.conf /etc/supervisor/conf.d/
cp api-server/.env.example /etc/stellar-zk/api-server.env

# 2) Update the conf file paths to match your actual clone directory.
#    The defaults assume /workspace/stellar-zk — edit command and directory
#    if your clone is elsewhere (e.g. /workspace/stellar-zk-claude).
nano /etc/supervisor/conf.d/risc0-asteroids-api.conf

# 3) Edit secrets/settings
nano /etc/stellar-zk/api-server.env
# Set API_KEY and any other overrides (PROD: keep RISC0_DEV_MODE=0)

# 4) Load configs (supervisord is already running on Vast.ai images)
supervisorctl reread && supervisorctl update

# 5) Inspect
supervisorctl status
tail -f /var/lib/stellar-zk/prover/api-server.log
curl -s http://127.0.0.1:8080/health | jq
```

If you update `/etc/stellar-zk/api-server.env`, apply changes with:

```bash
supervisorctl restart risc0-asteroids-api
```

### 6. Expose via Cloudflare Tunnel

Install cloudflared (if not done during setup — the VASTAI script handles this when `INSTALL_CLOUDFLARED=1`):

```bash
mkdir -p --mode=0755 /usr/share/keyrings
curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | \
  tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null
echo 'deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main' \
  > /etc/apt/sources.list.d/cloudflared.list
apt-get update -qq && apt-get install -y cloudflared
```

The tunnel is managed by supervisord alongside the api-server for automatic restart:

```bash
cp deploy/supervisord/cloudflared.conf /etc/supervisor/conf.d/
supervisorctl reread && supervisorctl update
supervisorctl status   # both risc0-asteroids-api and cloudflared should be RUNNING
```

By default this uses quick-tunnel mode (temporary `*.trycloudflare.com` URL). For production, set up a named tunnel so the URL is stable across restarts:

```bash
cloudflared tunnel login
cloudflared tunnel create risc0-prover
# Then edit /etc/supervisor/conf.d/cloudflared.conf:
#   command=cloudflared tunnel run risc0-prover
supervisorctl reread && supervisorctl update
```

Use the tunnel URL as `PROVER_BASE_URL` in your Cloudflare Worker config. For more details see the [Cloudflare Tunnel docs](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/).

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
  -X POST 'http://127.0.0.1:8080/api/jobs/prove-tape/raw?receipt_kind=composite&segment_limit_po2=21' \
  --data-binary @../test-fixtures/test-medium.tape \
  -H 'Content-Type: application/octet-stream' \
  -H 'x-api-key: YOUR_API_KEY' | jq -r '.job_id')

echo "Job ID: ${JOB_ID}"
```

**Query parameters:**

| Parameter | Default | Description |
|-----------|---------|-------------|
| `receipt_kind` | `composite` | `composite`, `succinct`, or `groth16` |
| `segment_limit_po2` | `21` | Segment size (2^n), range [16..22] |
| `max_frames` | `18000` | Max game frames to replay |
| `allow_dev_mode` | `false` | Allow dev-mode proving (disabled by policy) |
| `verify_receipt` | `false` | Verify the receipt after generation (off by default; verification happens on-chain) |

Zero-score tapes (`final_score == 0`) are rejected with `400` and
`error_code: "zero_score_not_allowed"`.

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
        "rules_digest": 1095980082
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
| `MAX_JOBS` | `64` | Max retained jobs in SQLite metadata store |
| `MAX_FRAMES` | `18000` | Max game frames for replay |
| `MIN_SEGMENT_LIMIT_PO2` | `16` | Min allowed segment limit |
| `MAX_SEGMENT_LIMIT_PO2` | `22` | Max allowed segment limit |
| `JOB_TTL_SECS` | `86400` | Finished job retention (24h) |
| `JOB_SWEEP_SECS` | `60` | Cleanup interval |
| `RUNNING_JOB_TIMEOUT_SECS` | `1800` | Timeout for active proofs before marking failed |
| `TIMED_OUT_PROOF_KILL_SECS` | `120` | Grace window after timeout before forced process abort (`0` disables) |
| `HTTP_MAX_CONNECTIONS` | `25000` | Max inbound connections |
| `HTTP_KEEP_ALIVE_SECS` | `75` | Keep-alive timeout |
| `HTTP_WORKERS` | _(auto)_ | Actix worker thread count |
| `CORS_ALLOWED_ORIGIN` | _(empty)_ | Optional single allowed browser origin |
| `ALLOW_DEV_MODE_REQUESTS` | `false` | Allow `allow_dev_mode=true` query param |

## Connecting the Cloudflare Worker

The Cloudflare Worker (`worker/`) proxies frontend proof requests to this api-server. Configure it via `wrangler.jsonc` vars or `wrangler secret`:

| Worker Setting | Description |
|----------------|-------------|
| `PROVER_BASE_URL` | Your tunnel URL (e.g., `https://xyz.trycloudflare.com`) |
| `PROVER_API_KEY` (secret) | Must match the `API_KEY` on the api-server |
| `PROVER_ACCESS_CLIENT_ID` (secret) | _(optional)_ Cloudflare Access service token ID |
| `PROVER_ACCESS_CLIENT_SECRET` (secret) | _(optional)_ Cloudflare Access service token secret |
| `PROVER_RECEIPT_KIND` | `groth16` by default (should match api-server policy) |
| `PROVER_SEGMENT_LIMIT_PO2` | `21` by default (must be within api-server's [min, max] range) |
| `PROVER_FALLBACK_SEGMENT_LIMIT_PO2` | `21` by default; auto-downgrade target when prover reports OOM/allocation failure |
| `PROVER_MAX_FRAMES` | `18000` (must be <= api-server's MAX_FRAMES) |
| `PROVER_VERIFY_RECEIPT` | `0` by default; on-chain verification is the source of truth |
| `PROVER_EXPECTED_IMAGE_ID` | _(optional)_ 32-byte hex image ID to pin worker to a specific prover build |
| `PROVER_HEALTH_CACHE_MS` | Cached prover health TTL in milliseconds (default `30000`) |
| `PROVER_POLL_INTERVAL_MS` | Poll cadence when prover job is still active |
| `PROVER_POLL_TIMEOUT_MS` | Absolute poll timeout before transition to retry/failure path |
| `PROVER_POLL_BUDGET_MS` | Per-alarm poll work budget in the DO |
| `PROVER_REQUEST_TIMEOUT_MS` | Timeout for each outbound prover request |
| `MAX_JOB_WALL_TIME_MS` | Worker-side max end-to-end job lifetime |
| `MAX_COMPLETED_JOBS` | Retention cap for terminal jobs in the coordinator DO |
| `COMPLETED_JOB_RETENTION_MS` | Time-based retention cutoff for terminal jobs in the coordinator DO |
| `ALLOW_INSECURE_PROVER_URL` | Keep `0` in production; only allow non-HTTPS for local/dev endpoints |

Set the secrets:

```bash
# From the repo root:
echo 'your-strong-random-secret' | npx wrangler secret put PROVER_API_KEY
echo 'https://xyz.trycloudflare.com' | npx wrangler secret put PROVER_BASE_URL
```

The worker submits tapes as `POST /api/jobs/prove-tape/raw` with `x-api-key` header, then polls `GET /api/jobs/{id}` until the status is `succeeded` or `failed`.

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

## Segment Sweep Benchmark (Remote Prover)

Use this when tuning `segment_limit_po2` on your x86/CUDA prover host:

```bash
bash scripts/bench-segment-sweep.sh https://your-prover.example.com \
  --seg-floor 19 \
  --seg-ceiling 22 \
  --receipt composite \
  --receipt succinct \
  --repeat 2 \
  --verify-receipt false \
  --include-real
```

Key knobs:
- `--seg-floor` / `--seg-ceiling`: requested sweep bounds (clamped to `/health` policy unless `--strict-bounds`).
- `--receipt`: repeatable; benchmark `composite`, `succinct`, and/or `groth16`.
- `--verify-receipt`: `false` by default to match production policy.
- `--max-frames`: optional override for stress scenarios.
- `--repeat`: run each configuration multiple times for stability.
- Zero-score tapes are skipped automatically (prover policy rejects `final_score=0`).

Output:
- CSV is written to `batch-results/segment-sweep-<timestamp>.csv` by default.
