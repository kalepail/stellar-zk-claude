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
RISC0_DEV_MODE=1 cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape
```

Then run without dev mode for a real proof:

```bash
RISC0_DEV_MODE=0 cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape
```

Optional journal output:

```bash
cargo run -p host --release -- --tape ../test-fixtures/test-medium.tape --journal-out ./journal.json
```
