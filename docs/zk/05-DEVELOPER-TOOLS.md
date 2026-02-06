# Developer Tools for ZK on Stellar

Last reviewed: February 2026.

## Baseline Toolchain
- Rust + `wasm32-unknown-unknown`
- `stellar-cli`
- Soroban SDK (`rs-soroban-sdk`)

## Groth16 / Circom Path
### Core tools
- Circom
- SnarkJS
- `circom2soroban` (conversion workflow)
- `soroban-verifier-gen` (contract generation)

### Typical flow
1. Write circuit.
2. Compile and generate proof artifacts.
3. Convert artifacts for Soroban contract use.
4. Deploy verifier contract.
5. Run end-to-end proof verification tests.

## RISC Zero Path
### Core tools
- `rzup` / RISC Zero toolchain (includes `r0vm`, `risc0-groth16`)
- Host/guest Rust workspace pattern
- Optional proving marketplace integration (for scaling)

### Typical flow
1. Implement deterministic guest logic.
2. Prove via host runner.
3. Verify receipts in local and testnet environments.
4. Integrate verifier contract submission path.

### Profiling and debugging
- **Cycle profiling**: `RISC0_PPROF_OUT=./profile.pb RISC0_DEV_MODE=1` generates pprof files. View with `go tool pprof -http=127.0.0.1:8000 profile.pb`.
- **Cycle counts**: `RISC0_INFO=1` logs segment and cycle stats. Use `env::cycle_count()` in guest for fine-grained measurement.
- **Dev mode**: `RISC0_DEV_MODE=1` skips proving but runs the guest and reports accurate cycle counts. Use for fast iteration on optimization experiments.
- **GDB debugging**: `rzup install gdb`, then `r0vm --elf <guest>.bin --with-debugger`. Requires `debug = true` in guest profile.
- **Inline tracking**: `RISC0_PPROF_ENABLE_INLINE_FUNCTIONS=yes` for detailed flamegraphs showing inlined functions.

### Precompiles (accelerated crypto)
RISC Zero provides precompile circuits for cryptographic operations that run far faster than software:
- SHA-256: 68 cycles per 64-byte block
- 256-bit modular multiply: 10 cycles
- Patched crates available for: `sha2`, `k256`, `p256`, `curve25519-dalek`, `rsa`, `bls12_381`, `blst`, `crypto-bigint`
- Apply via `[patch.crates-io]` in Cargo.toml pointing to RISC Zero forks.

### Guest optimization quick wins
- Use `opt-level = "s"` and `codegen-units = 1` — smaller guest binary means fewer page faults (1,094-5,130 cycles each).
- Avoid floats (60-140 cycles emulated), HashMap (SipHash overhead), unnecessary serde.
- Keep guest `no_std` and single-threaded.
- See `risc0-asteroids-verifier/OPTIMIZATION.md` for full experiment matrix.

## Noir / UltraHonk Path
### Core tools
- Noir (`nargo`)
- Barretenberg backend
- Soroban UltraHonk verifier stack

### Typical flow
1. Build and test circuit.
2. Benchmark proving and verification cost at realistic input sizes.
3. Validate verifier contract size/budget fit.
4. Integrate end-to-end submission flow.

## Security and Quality Tools
- Fuzz/property testing for contract and verifier logic.
- Dependency audits (`cargo audit` and lockfile hygiene).
- Security analysis tooling from ecosystem providers (OpenZeppelin, audit firms).

## Rule of Thumb
Choose one proving path per product milestone. Mixing multiple stacks in early
versions usually increases complexity without immediate payoff.
