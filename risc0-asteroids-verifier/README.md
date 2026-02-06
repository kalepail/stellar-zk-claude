# RISC0 Asteroids Verifier (Initial Codex Build)

This workspace is the first RISC Zero circuit/host implementation for Asteroids tape verification.

## Scope

- Shared deterministic core (`asteroids-core`) used by both host and guest.
- Tape parsing + CRC validation (`ZKTP` format from `src/game/tape.ts`).
- Deterministic replay simulation over fixed-point integer state.
- Guest proof of replay correctness.
- Host-side proving + receipt verification + journal decode.

## Workspace Layout

- `asteroids-core/` shared verifier logic (`no_std` compatible).
- `methods/guest/` RISC0 guest entrypoint.
- `host/` proving runner CLI.

## Toolchain

This workspace targets RISC Zero `3.0.5` (`cargo-risczero` and `r0vm`).

## Run Tests

```bash
cargo test -p asteroids-verifier-core
```

## Generate A Proof

```bash
cd risc0-asteroids-verifier
RISC0_DEV_MODE=1 cargo run -p host --release -- --allow-dev-mode --tape ../test-fixtures/test-medium.tape
```

Then run without dev mode for a real proof:

```bash
RISC0_DEV_MODE=0 cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape
```

By default, the host refuses to run when `RISC0_DEV_MODE=1` unless you pass
`--allow-dev-mode`. This prevents accidentally accepting fake dev receipts in
security-critical flows.

The host defaults to `--segment-limit-po2 19` (good memory/perf stability for this workload).

Optional journal output:

```bash
cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape --journal-out ./journal.json
```

Optional performance tuning knobs:

```bash
cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape --receipt-kind composite --segment-limit-po2 20
```

## Coverage + Performance Harness

From repo root:

```bash
bash scripts/bench-risc0.sh
```

`bench-risc0.sh` also defaults to `--segment-limit-po2 19` and can be overridden via `--segment-limit-po2 <n>`.

The script writes artifacts to:

- `risc0-asteroids-verifier/benchmarks/runs/<timestamp>/summary.md`
- `risc0-asteroids-verifier/benchmarks/runs/<timestamp>/metrics.csv`
- per-run logs / pprof artifacts in the same directory.

Run with regression checks enabled:

```bash
bash scripts/bench-risc0.sh --check
```

Include secure medium proving (slow):

```bash
bash scripts/bench-risc0.sh --full --check
```

Thresholds are defined in:

- `risc0-asteroids-verifier/benchmarks/thresholds.env`
