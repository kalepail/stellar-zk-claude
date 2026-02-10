# Codex Improvements

Date: 2026-02-07
Scope: Stellar Asteroids contract, RISC0 verifier stack, proof/test scripts, and testnet/live-prover operations.

## Baseline Audit Outcome

What passed:
- `stellar-asteroids-contract`: `cargo test --workspace --all-targets` (15/15 tests passed).
- Testnet end-to-end integration: `stellar-asteroids-contract/scripts/deploy-and-test.sh --proof-mode all` (24/24 checks passed).
- Real tape replay checks: `scripts/verify-tape.ts` for `test-short`, `test-medium`, `test-real-game` all passed.
- Verifier core: `cargo +1.93.0 test --workspace --all-targets --no-default-features` in `risc0-asteroids-verifier` passed.
- Live prover (Vast.ai URL): successful proving for short/medium/real tapes and full benchmark matrix.
- Live API stress test: `scripts/stress-test-api.sh https://risc0-kalien.stellar.buzz --delay 1` passed (40/40).
- Fresh non-fixture E2E proved + settled on testnet:
  - generated tape `/tmp/codex-fresh-1200.tape` (score 20),
  - generated Groth16 proof from live prover,
  - submitted on-chain to `CCVCYDNFYUT4EXL2GFXWL555XN5RISQ55PN5CSWPFF2GAMDRXBJXI4QI`,
  - claim marked true,
  - token balance increased by 20 (to `18330`),
  - duplicate re-submit rejected (non-zero exit), no extra mint.

What failed or needs hardening:
- Script CLI drift in router verify scripts (critical operational bug).
- Image-ID consistency risk between local guest build and live/testnet fixtures.
- No TTL extension logic in score contract (mainnet durability risk).
- Local scripts default to CUDA builds and fail on machines without `nvcc`.
- Cleanliness gates (`clippy`, `fmt`, JS lint/format) are not fully green.

## Priority Plan

### P0: Fix Operationally Broken Script Paths

1. Fix router verify arg name in proof scripts.
- Files:
  - `stellar-asteroids-contract/scripts/verify-proofs.sh`
  - `stellar-asteroids-contract/scripts/regenerate-proofs.sh`
- Problem:
  - Uses `verify --journal_digest ...` but current CLI expects `verify --journal ...`.
  - Verified by failure output and manual corrected invocation success.
- Change:
  - Replace `--journal_digest` with `--journal`.
- Acceptance:
  - `./scripts/verify-proofs.sh` passes 3/3.
  - `./scripts/regenerate-proofs.sh <prover-url>` verifies all regenerated fixtures on-chain.

2. Add script compatibility smoke checks to CI.
- Add a lightweight CI script that runs:
  - `stellar contract invoke ... verify --help` sanity parse check,
  - one fixture verification command in dry/run mode.
- Acceptance:
  - CI fails on future CLI arg drift before release.

### P0: Enforce Image ID Consistency End-to-End

3. Add image-id consistency gate across build/prover/fixtures.
- Problem:
  - Live fixture/prover image ID currently observed as `4298acea...`.
  - Local compiled method ID observed as `755b655c...` in generated `methods.rs` output.
  - If not synchronized, valid proofs from one environment fail on contracts pinned to the other.
- Change:
  - Add a check script that compares:
    - local `host::image_id_hex()` (or build output),
    - `test-fixtures/*.image_id`,
    - prover `/health` `image_id`,
    - deployment state file image id.
  - Fail fast on mismatch unless explicitly overridden.
- Acceptance:
  - A single canonical image ID is enforced per release branch.

### P1: Mainnet Contract Safety Hardening

4. Add TTL extension strategy for instance + persistent keys.
- File: `stellar-asteroids-contract/contracts/asteroids_score/src/lib.rs`
- Problem:
  - Contract writes instance/persistent storage but never extends TTL.
- Change:
  - In `submit_score`, extend instance TTL and claimed key TTL.
  - Add operational script for periodic contract/code TTL bump.
- Acceptance:
  - Storage survives expected inactivity windows without replay-protection expiry surprises.

5. Add controlled emergency mechanisms.
- Add:
  - pause flag (`set_paused`) checked in `submit_score`,
  - optional `set_router_id` admin method (if operationally required).
- Acceptance:
  - Can halt claims during incident response.
  - Router migration is possible without redeploying everything.

### P1: Prover Deployment Hardening

6. Pin full prover toolchain versions in `VASTAI`.
- File: `risc0-asteroids-verifier/VASTAI`
- Problem:
  - `rzup install` is unpinned and can drift.
- Change:
  - Pin explicit RISC0/rzup versions and record in release doc.
- Acceptance:
  - Reproducible prover builds across new instances.

7. Require API auth in production.
- Ensure `API_KEY` is non-empty in production env and worker side secret matches.
- Acceptance:
  - `/api/*` rejects unauthenticated requests in production.

### P2: Cross-Platform Dev Reliability

8. Add CPU fallback path for local scripts.
- Files:
  - `scripts/verify-tape-risc0.ts`
  - `scripts/bench-risc0.sh`
- Problem:
  - Current defaults compile CUDA-enabled host and fail without `nvcc`.
- Change:
  - Add explicit `--cpu` mode (`--no-default-features`) and auto-detect fallback when CUDA toolchain absent.
- Acceptance:
  - Scripts run on standard dev machines without CUDA.

### P2: Cleanliness and Static Quality Gates

9. Make lint/format/clippy green (or codify intentional exceptions).
- Rust:
  - Fix clippy warnings seen with `-D warnings`:
    - `stellar-asteroids-contract/contracts/asteroids_score/src/test.rs` loop style,
    - `risc0-asteroids-verifier/host/src/lib.rs` `manual_is_multiple_of`,
    - `risc0-asteroids-verifier/api-server/src/main.rs` `wrong_self_convention`.
  - Ensure rustfmt clean for both Rust workspaces.
- TS/Worker:
  - Resolve current `bun run check` warnings and `worker/constants.ts` format issue.
- Acceptance:
  - `cargo clippy ... -D warnings` and `cargo fmt --check` pass for targeted workspaces.
  - `bun run check` passes.

## Suggested Verification Matrix After Fixes

Run all of the following on every release candidate:
- Contract unit tests: `cd stellar-asteroids-contract && cargo test --workspace --all-targets`
- Contract integration/testnet:
  - `./scripts/verify-proofs.sh`
  - `./scripts/deploy-and-test.sh --proof-mode all`
- Verifier:
  - `cd risc0-asteroids-verifier && cargo +1.93.0 test --workspace --all-targets --no-default-features`
- Live prover:
  - `bash scripts/bench-prover-api.sh <prover-url>`
  - `bash scripts/stress-test-api.sh <prover-url> --delay 1`
- Fresh non-fixture E2E:
  - generate tape,
  - generate live Groth16 proof,
  - submit to testnet contract,
  - assert claimed + balance delta + duplicate rejection.
