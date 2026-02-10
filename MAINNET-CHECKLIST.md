# Mainnet Readiness Checklist

Comprehensive checklist for deploying Stellar ZK Asteroids to Stellar mainnet.
Work through each section before going live.

---

## 1. Prover API Security (Vast.ai)

The RISC0 prover runs on Vast.ai bare metal and is the most security-critical
external service. Without hardening, anyone on the internet can submit proving
jobs and consume expensive GPU time.

- [ ] **Set a strong `API_KEY`** on the prover instance. The prover currently
      allows unauthenticated access when `API_KEY` is empty
      (`api-server/src/main.rs:302-305`). Generate a random secret (32+ chars)
      and set it in the prover environment.
- [ ] **Set `PROVER_API_KEY` on the Cloudflare Worker** via
      `npx wrangler secret put PROVER_API_KEY` so the worker authenticates to
      the prover.
- [ ] **Restrict CORS on the prover**. Currently `allow_any_origin()` is set
      (`api-server/src/main.rs:611-616`). Restrict to only the Cloudflare Worker
      origin, or better yet, make the prover only accessible via Cloudflare
      Tunnel with no public CORS at all.
- [ ] **Use a persistent Cloudflare Tunnel** instead of temporary
      `*.trycloudflare.com` URLs. Set `INSTALL_CLOUDFLARED=1` in the VASTAI
      script, then configure a named tunnel in the Cloudflare dashboard.
- [ ] **Consider Cloudflare Access service tokens** for defense-in-depth. The
      worker already supports `PROVER_ACCESS_CLIENT_ID` and
      `PROVER_ACCESS_CLIENT_SECRET` (`worker/env.ts:11-12`). Set up a Cloudflare
      Zero Trust application policy on the tunnel.
- [ ] **Verify `RISC0_DEV_MODE=0`** on the prover instance. Dev mode generates
      fake proofs that would pass the mock verifier but not the Groth16 verifier.
- [ ] **Verify `PROOF_MODE_POLICY=secure-only`** on the prover instance so
      clients cannot request `proof_mode=dev`.
- [ ] **Verify `ALLOW_INSECURE_PROVER_URL=0`** in wrangler.jsonc (already the
      default). The worker must communicate with the prover over HTTPS only.
- [ ] **Update `PROVER_BASE_URL`** in wrangler.jsonc from the placeholder
      `https://replace-with-your-prover.example.com` to the actual tunnel URL.
      Alternatively store it as a Wrangler secret.

---

## 2. Stellar Network Configuration

Switch all Stellar references from testnet to mainnet.

- [ ] **Add mainnet network to Stellar CLI:**
      ```bash
      stellar network add --global mainnet \
        --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm \
        --network-passphrase "Public Global Stellar Network ; September 2015"
      ```
      (Or use another mainnet RPC provider: Validation Cloud, Blockdaemon, etc.)
- [ ] **Choose a mainnet Soroban RPC provider** and confirm rate limits,
      availability SLAs, and pricing.
- [ ] **Add frontend environment variables** for mainnet. The client integration
      spec (`docs/games/asteroids/10-CLIENT-INTEGRATION-SPEC.md:106-112`)
      defines these — they need actual values:
      ```
      VITE_SMART_ACCOUNT_RPC_URL=<mainnet RPC>
      VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
      VITE_SMART_ACCOUNT_WASM_HASH=<mainnet account contract wasm hash>
      VITE_SMART_ACCOUNT_WEBAUTHN_VERIFIER_ADDRESS=<mainnet webauthn verifier contract ID>
      VITE_SMART_ACCOUNT_RELAYER_URL=https://channels.openzeppelin.com
      VITE_SMART_ACCOUNT_RELAYER_API_KEY=<mainnet relayer API key>
      VITE_SMART_ACCOUNT_RELAYER_PLUGIN_ID=<optional relayer plugin ID>
      VITE_SMART_ACCOUNT_RP_NAME="Stellar ZK"
      ```
- [ ] **Confirm the RISC Zero router contract exists on mainnet**. The testnet
      router is `CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD` and
      the Groth16 verifier is
      `CB54QOGYJJOSLNHRCHTSVGKJ3D5K6B5YO7DD6CRHRBCRNPF2VX2VCMV7`. These
      may differ on mainnet — verify with the RISC Zero / Stellar teams.
- [ ] **Update `deploy-and-test.sh`** (or create a separate mainnet deployment
      script) to use `NETWORK="mainnet"` and the mainnet router contract ID.

---

## 3. Score Token Deployment

Deploy the SCORE token on mainnet and configure minting authority.

- [ ] **Create the mainnet issuer account** with enough XLM for reserves.
      This account will issue the SCORE Stellar Asset Contract (SAC) token.
- [ ] **Deploy the SAC token** on mainnet. The testnet script
      (`deploy-and-test.sh:176-200`) creates an asset `SCORE:<issuer>` and wraps
      it as a Soroban SAC. Replicate this for mainnet:
      ```bash
      # Create the asset issuer
      stellar keys generate --global score-issuer --network mainnet --fund
      ISSUER_ADDR=$(stellar keys address score-issuer)

      # Deploy the SAC
      stellar contract asset deploy \
        --asset "SCORE:$ISSUER_ADDR" \
        --source score-issuer \
        --network mainnet
      ```
- [ ] **Record the mainnet TOKEN_ID** (SAC contract address) — you'll need it
      when deploying the score contract.
- [ ] **Transfer token admin to the score contract** after deploying it (see
      section 4). The score contract must be the token admin to call `mint()`.
      ```bash
      stellar contract invoke \
        --id $TOKEN_ID \
        --source score-issuer \
        --network mainnet \
        -- set_admin --new_admin $SCORE_CONTRACT_ID
      ```
- [ ] **Verify minting works** by submitting a real proof and confirming token
      balance increases for the player.
- [ ] **Consider token metadata**: name, symbol, decimals. SAC tokens inherit
      these from the classic asset. "SCORE" is 12 characters max, 0 decimals
      (since scores are integers).
- [ ] **Decide on token supply governance**: Is there a max supply? Should
      minting be uncapped or should the contract enforce a ceiling? Currently
      there is no cap — every valid proof mints `final_score` tokens.

---

## 4. Score Contract Deployment

Deploy the asteroids_score contract to mainnet.

- [ ] **Build the contract WASM** for production:
      ```bash
      cd stellar-asteroids-contract
      stellar contract build
      ```
      Verify the WASM at
      `target/wasm32v1-none/release/asteroids_score.wasm`.
- [ ] **Determine the correct `image_id`** for the production RISC0 guest
      program. This is the 32-byte hash that identifies the proving program.
      Get it from the prover build output or `methods/build.rs`. Currently:
      `755b655c3063e7684eaeb2073ba2a75d6b41e82b1577671bbf29600eca10d83d`
      (from `.testnet-state.env`). **Any change to the Rust guest code changes
      this value** — rebuild and re-verify.
- [ ] **Deploy the contract to mainnet:**
      ```bash
      stellar contract deploy \
        --wasm $WASM \
        --source <deployer> \
        --network mainnet \
        -- \
        --admin <admin-address> \
        --router_id <mainnet-risc0-router> \
        --image_id <image-id-hex> \
        --token_id <mainnet-token-id>
      ```
- [ ] **Record the mainnet SCORE_CONTRACT_ID**.
- [ ] **Transfer token admin** to the score contract (see section 3).
- [ ] **Verify contract state** after deployment:
      ```bash
      stellar contract invoke --id $CONTRACT_ID --network mainnet -- image_id
      stellar contract invoke --id $CONTRACT_ID --network mainnet -- router_id
      stellar contract invoke --id $CONTRACT_ID --network mainnet -- token_id
      ```
- [ ] **Set the contract admin key** to a secure multisig or cold-storage
      account. The admin can call `set_image_id()` and `set_admin()` — this
      key must be protected.
- [ ] **Store the mainnet state file** (contract IDs, deployer, image ID) in a
      `.mainnet-state.env` file (add to `.gitignore`).

---

## 5. Contract Hardening

Review the on-chain contract for production safety.

- [ ] **Add TTL extension logic**. The contract currently has NO `extend_ttl()`
      calls (`lib.rs`). On mainnet, instance storage, persistent entries
      (Claimed journal digests), and the contract code itself will expire if
      TTLs are not extended. At minimum:
      - Extend instance storage TTL on every `submit_score()` call
      - Extend persistent `Claimed(digest)` entries so replay protection doesn't
        silently expire
      - Set up an operational cron or script to periodically bump contract and
        code TTLs
- [ ] **Audit the `risc0_router.wasm` import**. The contract imports the router
      WASM at compile time (`lib.rs:8-10`). Verify this is the correct mainnet
      router interface and that the WASM in the repo matches what's deployed.
- [ ] **Consider adding a pause mechanism**. Currently there is no way to pause
      the contract in case of an exploit. An admin-controlled pause flag would
      allow halting `submit_score()` if needed.
- [ ] **Consider adding a `set_router_id()` function**. If the RISC Zero router
      is ever redeployed, there is currently no way to update the router address
      without redeploying the entire score contract.
- [ ] **Review error handling completeness**. Verify that all error paths
      revert cleanly and do not leave partial state.
- [ ] **Run the full test suite against real Groth16 proofs**, not just mock
      verifier proofs. The test fixtures in `test-fixtures/` include
      `proof-*-groth16.*` files — ensure these all pass.
- [ ] **Get a third-party audit** of the contract. Recommended auditors from
      the security docs: Veridise, Certora, or OpenZeppelin.

---

## 6. Client Integration (Wallet + On-Chain Submission)

The frontend currently stops at "proof succeeded" with no wallet or on-chain
submission. These are all missing and required for mainnet.

- [ ] **Implement passkey wallet integration** (SAK — Smart Account Kit).
      Modules needed: `wallet/connect.ts`, `wallet/session.ts`,
      `wallet/transaction.ts`.
- [ ] **Implement Soroban contract client** (`contract/client.ts`,
      `contract/config.ts`) wrapping `submit_score`, `is_claimed`, and getters.
- [ ] **Implement proof claim flow** (`proof/claim.ts`):
      1. Fetch proof result from worker API
      2. Extract seal + journal_raw from proof artifact
      3. Check `is_claimed()` to avoid wasted transactions
      4. Build and sign `submit_score` transaction
      5. Submit via relayer
      6. Return tx hash + minted score
- [ ] **Implement token balance and history display** (`chain/` module):
      token balance queries, `ScoreSubmitted` event history.
- [ ] **Choose and configure a relayer** (OpenZeppelin Channels or custom) for
      feeless or sponsored transactions.
- [ ] **Handle mainnet XLM requirements**: players need trustlines to the SCORE
      token. Decide if the relayer/sponsor covers this or if the user must have
      XLM.

---

## 7. Cloudflare Workers Production Config

Harden the Cloudflare Workers deployment for mainnet traffic.

- [ ] **Configure a custom domain** for the worker (e.g., `asteroids.example.com`)
      instead of the default `*.workers.dev` URL.
- [ ] **Add security headers** middleware in `worker/index.ts`:
      ```
      X-Content-Type-Options: nosniff
      X-Frame-Options: DENY
      Strict-Transport-Security: max-age=31536000; includeSubDomains
      ```
- [ ] **Verify R2 bucket is private**. Check the Cloudflare dashboard to ensure
      `stellar-zk-proof-artifacts` has no public access rules. All access should
      be through the Worker binding only.
- [ ] **Set R2 lifecycle rule** to auto-expire proof artifacts as a safety net.
      The DO already prunes at 24hr/200 jobs, but orphaned R2 objects from edge
      cases (crashes between R2 write and DO tracking) need a backstop:
      ```bash
      npx wrangler r2 bucket lifecycle add stellar-zk-proof-artifacts \
        --name expire-proof-jobs \
        --prefix proof-jobs/ \
        --expire-days 7
      ```
- [ ] **Consider rate limiting** at the Cloudflare edge (WAF rules or Rate
      Limiting rules) to prevent tape submission spam.
- [ ] **Review queue retry settings** in `wrangler.jsonc`. Current:
      `max_retries: 10`. Consider whether this is appropriate for mainnet.
- [ ] **Set up Cloudflare analytics and alerting** for error rates, latency,
      and queue depth.
- [ ] **Ensure Durable Object migration** `v1-proof-coordinator` has been
      deployed to the production Worker.

---

## 8. Prover Infrastructure

Production readiness for the Vast.ai prover server.

- [ ] **Pin `GIT_REF`** in the VASTAI script to a specific release tag or commit
      SHA, not `main`. This prevents accidental deployment of untested code.
- [ ] **Set up persistent storage** for the SQLite job store (`DATA_DIR`). If
      the Vast.ai instance is preempted, job history is lost. Consider using a
      persistent volume or external database.
- [ ] **Configure appropriate resource limits**:
      - `MAX_JOBS=64` (or higher for mainnet traffic)
      - `MAX_TAPE_BYTES=2097152` (2 MB, verify this covers max game length)
      - `MAX_FRAMES=18000` (5 minutes at 60 fps — confirm this is sufficient)
      - `RUNNING_JOB_TIMEOUT_SECS=1800` (30 min per proof — adjust per actual
        proving times)
- [ ] **Benchmark proving times** with real game tapes of various lengths.
      Document expected latency for short, medium, and long games.
- [ ] **Set up health monitoring** with automated alerts if `/health` returns
      unhealthy or the prover is unreachable.
- [ ] **Plan for prover redundancy**. Currently there's a single Vast.ai
      instance. If it goes down, all proving halts. Consider a hot standby or
      failover strategy.
- [ ] **Lock Rust and RISC Zero toolchain versions** in the VASTAI script.
      Currently pinned to `RUST_TOOLCHAIN_VERSION=1.93.0` but `rzup install`
      fetches latest. Pin specific versions.
- [ ] **Use `cargo build --locked`** to ensure the `Cargo.lock` is respected
      and dependencies don't drift.
- [ ] **Run the api-server under supervisord** for automatic restart on crash.
      The api-server intentionally aborts the process when a timed-out proof
      remains stuck (see `TIMED_OUT_PROOF_KILL_SECS`). A process supervisor
      ensures automatic recovery. **Vast.ai containers do not have systemd**
      (PID 1 is not init), so use supervisord instead. The compiled binary is
      self-contained (no `cargo` or RISC Zero CLI tools needed at runtime;
      CUDA libs are in system `ld` paths on Vast.ai). Deploy files:
      - `deploy/supervisord/risc0-asteroids-api.conf` (supervisord program)
      - `api-server/.env.example` (env config)
      ```bash
      # On the Vast.ai box via SSH:
      cd <your-clone>/risc0-asteroids-verifier

      # 1. Build first (the service runs the compiled binary, not cargo run)
      cargo build --locked --release -p api-server

      # 2. Install supervisord config and env file
      mkdir -p /etc/stellar-zk /var/lib/stellar-zk/prover
      cp deploy/supervisord/risc0-asteroids-api.conf /etc/supervisor/conf.d/
      cp api-server/.env.example /etc/stellar-zk/api-server.env

      # 3. IMPORTANT: Edit BOTH files to match your actual clone path.
      #    The VASTAI script defaults to /workspace/stellar-zk/ but a manual
      #    git clone uses /workspace/stellar-zk-claude/. Update command and
      #    directory in the .conf file accordingly.
      nano /etc/supervisor/conf.d/risc0-asteroids-api.conf
      nano /etc/stellar-zk/api-server.env   # set API_KEY and other config

      # 4. Load configs (supervisord is already running on Vast.ai images)
      supervisorctl reread && supervisorctl update
      supervisorctl status                              # verify running
      tail -f /var/lib/stellar-zk/prover/api-server.log
      ```

---

## 9. Determinism & Proof Integrity

Verify bit-for-bit determinism between the TypeScript game engine and the
Rust prover core.

- [ ] **Run cross-implementation parity tests**. Play back the same tape in
      both the TypeScript engine (`AsteroidsGame.ts`) and the Rust
      `asteroids-core` — final scores, RNG state, and checksums must match
      exactly.
- [ ] **Verify the image ID matches** between the compiled guest program and
      what's configured in the contract. If the guest code changes, the
      image ID changes, and old proofs will fail verification.
- [ ] **Test end-to-end with real Groth16 proofs** (not mock verifier):
      1. Play a game and capture the tape
      2. Submit to prover and get Groth16 proof
      3. Submit proof to testnet contract with Groth16 verifier
      4. Confirm token minting succeeds
- [ ] **Validate the `RULES_DIGEST` constant** (`0x4153_5433` / "AST3").
      This is baked into both the guest and contract. It serves as a versioning
      marker — if game rules change, bump this value.

---

## 10. Operational Readiness

Production operations, monitoring, and incident response.

- [ ] **Create a mainnet deployment runbook** documenting every step in order:
      1. Deploy token
      2. Deploy contract (with correct admin, router, image ID, token)
      3. Transfer token admin to contract
      4. Verify contract state
      5. Update frontend env vars
      6. Deploy Worker
      7. Start prover with production config
      8. End-to-end smoke test
- [ ] **Set up monitoring dashboards** covering:
      - Worker request rates, error rates, latency
      - Queue depth and retry counts
      - Prover health, GPU utilization, proving latency
      - R2 storage usage
      - Contract invocation success/failure rates (via Soroban RPC or explorer)
- [ ] **Define an incident response plan**:
      - What to do if the prover goes down
      - What to do if a contract bug is discovered (pause mechanism?)
      - What to do if the image ID needs rotation
      - Contact information for RISC Zero and Stellar teams
- [ ] **Document API key rotation procedure**: How to rotate `PROVER_API_KEY`
      across both the Vast.ai prover and Cloudflare Worker without downtime.
- [ ] **Plan for contract upgrades**: Since Soroban contracts are immutable once
      deployed, plan how to handle future versions. Options: deploy a new
      contract and update the frontend, or use an upgradable proxy pattern.
- [ ] **Set up automated contract TTL bumping** as a cron job or operational
      task. Without this, the contract and its storage will eventually expire
      on mainnet.
- [ ] **Back up all mainnet keys securely**: deployer key, admin key, token
      issuer key. Use hardware wallets or secure key management for the admin
      key since it controls `set_image_id()` and `set_admin()`.

---

## 11. Testing Before Launch

Final verification before flipping the switch.

- [ ] **Full testnet dry run** with the exact mainnet configuration (just
      pointing at testnet). Walk through every step of the deployment runbook.
- [ ] **End-to-end test on testnet**:
      1. Play a full game
      2. Submit tape to worker
      3. Worker dispatches to prover
      4. Prover generates Groth16 proof
      5. Frontend claims proof on-chain via wallet
      6. Contract verifies and mints tokens
      7. UI shows updated balance
- [ ] **Load test the worker** with concurrent tape submissions. Verify the
      single-flight queue correctly returns 429 for concurrent requests and
      that jobs complete reliably.
- [ ] **Test failure scenarios**:
      - Prover is unreachable — does the worker retry correctly?
      - Prover returns an error — does the job fail gracefully?
      - Duplicate tape submission — is replay protection enforced?
      - Invalid proof — does the contract reject it?
      - Token admin is not set — does minting fail with a clear error?
- [ ] **Security scan** of all deployed infrastructure:
      - Can the prover be accessed without the API key?
      - Can the R2 bucket be accessed publicly?
      - Are all dev-mode flags disabled?
      - Is HTTPS enforced end-to-end?
- [ ] **Verify no secrets are committed and `.example` files exist** for every
      env-var surface. Audit the repo for leaked keys, tokens, or credentials
      and ensure every place that consumes env variables has a checked-in
      example file so new developers know what to set. Current state:
      - `.gitignore` ignores `.dev.vars` (Cloudflare Worker secrets) — but
        **no `.dev.vars.example`** exists to document expected keys
        (`PROVER_API_KEY`, `PROVER_ACCESS_CLIENT_ID`, etc.)
      - `stellar-asteroids-contract/.gitignore` ignores `.testnet-state.env`
        — but **no `.testnet-state.env.example`** exists
      - `risc0-asteroids-verifier/api-server/.env.example` exists (good)
      - `risc0-asteroids-verifier/api-server/.env.example` exists (good)
      - Root `.gitignore` does **not** ignore `.env` — add a catch-all
        `.env*` pattern (excluding `.env.example`) as a safety net
      - `risc0-asteroids-verifier/.gitignore` does **not** exist — any
        `.env` dropped in the prover workspace would be committed
      - Run `git log --all --diff-filter=A -- '*.env' '*.vars' '*secret*'`
        to check history for accidentally committed secrets

---

## 12. Project Rename & Repository Migration

The project will be renamed and moved to a new GitHub URL before mainnet.
Directory names inside the repo may also change. Do this **before** deploying
anything to mainnet so all deployed config points to the final names.

- [ ] **Choose the new repo name and GitHub URL.** Update or redirect the
      current `kalepail/stellar-zk-claude` repo.
- [ ] **Rename / reorganize internal directories** as desired (e.g.
      `risc0-asteroids-verifier/` → shorter name). After renaming, grep the
      entire repo for stale path references — at minimum these files hardcode
      paths today:
      - `risc0-asteroids-verifier/VASTAI` — `REPO_URL`, `WORKDIR`,
        "Next steps" output (line 106)
      - `risc0-asteroids-verifier/deploy/supervisord/risc0-asteroids-api.conf`
        — `command`, `directory`
      - `risc0-asteroids-verifier/README.md` — clone URLs, paths
      - `MAINNET-CHECKLIST.md` — references throughout
- [ ] **Update `REPO_URL`** in the VASTAI script to the new GitHub URL.
- [ ] **Update `WORKDIR`** in the VASTAI script and supervisord conf paths to
      match the new directory layout.
- [ ] **Update Cloudflare Tunnel config** if the tunnel name or origin path
      references the old repo name.
- [ ] **Update any CI, GitHub Actions, or deployment scripts** that reference
      the old repo URL or directory structure.
- [ ] **Set up a GitHub redirect** from the old repo URL if others have linked
      to it (GitHub does this automatically for renames within the same owner).

---

## 13. Documentation & Legal

- [ ] **Update all testnet references** in docs/ to note mainnet equivalents.
- [ ] **Remove or `.gitignore` testnet state files**
      (`.testnet-state.env`, test deployer keys).
- [ ] **Document mainnet contract addresses** for users and integrators.
- [ ] **Review and update license files**. Currently the only project-owned
      LICENSE file is `risc0-asteroids-verifier/LICENSE` (Apache 2.0 with
      unfilled `[yyyy] [name of copyright owner]` boilerplate). Items to
      address:
      - Fill in the copyright year and owner in the existing Apache 2.0
        LICENSE in `risc0-asteroids-verifier/`
      - Add a root-level LICENSE for the overall project (frontend, worker,
        scripts, docs)
      - Add a LICENSE to `stellar-asteroids-contract/` (the Soroban contract
        has none)
      - Add `license` fields to `package.json` and the Rust `Cargo.toml`
        workspace files (both are currently missing)
      - Decide if all components should use the same license or if different
        parts warrant different licenses (e.g. MIT for frontend, Apache 2.0
        for prover)
- [ ] **Comprehensive documentation sweep.** Review and update all READMEs,
      inline code comments, doc specs, and operational notes for accuracy,
      staleness, and consistency with the final project name/URLs. Files to
      cover:
      - READMEs: `docs/README.md`, `risc0-asteroids-verifier/README.md`,
        `risc0-asteroids-verifier/api-server/README.md`,
        `stellar-asteroids-contract/README.md` (no root-level README exists
        — consider adding one)
      - Specs (30+ files): `docs/games/asteroids/` (game, verification,
        integer math, proving system, proof gateway, client integration,
        guest optimization specs) and `docs/zk/` (protocol foundations,
        proving systems, developer tools, security)
      - Operational docs: `MAINNET-CHECKLIST.md`,
        `stellar-asteroids-contract/codex-improvements.md`
      - Inline comments: code comments referencing testnet contract IDs,
        placeholder URLs, old repo names, or TODO/FIXME/HACK markers
      - Script help text: `VASTAI` "Next steps" output, deploy script
        banners, CLI `--help` strings
- [ ] **Review terms of service / disclaimer** for the game and token. Minting
      tokens based on game scores may have regulatory considerations depending
      on jurisdiction.
- [ ] **Add a privacy policy** if the app collects any user data (wallet
      addresses, game telemetry, etc.).

---

## Quick Reference: Key Secrets & Configuration

| Secret / Config | Where to Set | Notes |
|---|---|---|
| `API_KEY` (prover) | Prover `.env` on Vast.ai | Must be strong, random |
| `PROVER_API_KEY` (worker) | `wrangler secret put PROVER_API_KEY` | Must match prover `API_KEY` |
| `PROVER_BASE_URL` | `wrangler.jsonc` or `wrangler secret put` | Cloudflare Tunnel URL |
| `PROVER_ACCESS_CLIENT_ID` | `wrangler secret put` | Optional, Cloudflare Access |
| `PROVER_ACCESS_CLIENT_SECRET` | `wrangler secret put` | Optional, Cloudflare Access |
| Admin key (contract) | Secure cold storage / multisig | Controls image_id + admin transfer |
| Token issuer key | Secure storage | Only needed during initial setup |
| Mainnet deployer key | Secure storage | Only needed during deployment |

| Env Var | Required Value for Mainnet |
|---|---|
| `RISC0_DEV_MODE` | `0` |
| `PROOF_MODE_POLICY` | `secure-only` |
| `ALLOW_INSECURE_PROVER_URL` | `0` |
| `PROVER_RECEIPT_KIND` | `groth16` |
| `NETWORK` | `mainnet` (in deployment scripts) |
