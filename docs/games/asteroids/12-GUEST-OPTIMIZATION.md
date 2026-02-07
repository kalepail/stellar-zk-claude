# Guest Optimization Guide

References:
- https://dev.risczero.com/api/zkvm/optimization
- https://dev.risczero.com/api/zkvm/profiling
- https://dev.risczero.com/api/zkvm/precompiles
- https://dev.risczero.com/api/zkvm/benchmarks

## Current Profile

```toml
# Cargo.toml (workspace root)
[profile.dev]
opt-level = 3

[profile.release]
debug = 1
lto = true
```

## What We Already Get Right

- **Integer-only math** — No floats anywhere. Floats cost 60-140 cycles in the zkVM vs 1-2 for integer ops. Our Q12.4/Q8.8/BAM system avoids this entirely.
- **No HashMap** — Would pull in SipHash randomization. We don't need maps at all.
- **No serde in the hot path** — Guest reads raw slices via `env::read_slice()`, commits a 24-byte struct. No deserialization overhead.
- **`no_std` guest** — Minimal binary, no std bloat.
- **No async/threading** — zkVM is single-threaded. Async constructs only add overhead.
- **Trig via lookup tables** — `sin_bam`/`cos_bam` are array lookups (1 cycle) not computed trig.
- **No division in hot path** — All division replaced with shifts or reciprocal multiply.

## zkVM Cost Model (Key Differences from CPUs)

Understanding why certain optimizations matter:

| Operation | zkVM Cycles | Physical CPU |
|-----------|-------------|-------------|
| Add, sub, mul, load, store | 1 | 1-5 |
| Bitwise, div, remainder, shift-right | 2 | 1-40 |
| Float (emulated) | 60-140 | 1-5 |
| Memory page-in | 1,094-5,130 | N/A |
| Memory page-out | 1,094-5,130 | N/A |
| Unaligned 32-bit read | 12 | ~1 |
| Sequential byte iteration | ~1.35/byte | varies |

**Key insight:** Paging dominates. Each 1 KB page fault costs ~1,130 cycles on average. Smaller guest binary = fewer pages = fewer page faults = dramatically lower cycle count. This is why compiler size optimizations (`opt-level = "s"`) can outperform speed optimizations (`opt-level = 3`).

## Experiment Matrix

| # | lto | opt-level | codegen-units | Hypothesis |
|---|-----|-----------|---------------|------------|
| 0 | true | 3 | default | **Current baseline** |
| 1 | "thin" | 3 | 1 | Thin LTO sometimes faster than fat |
| 2 | true | 2 | 1 | Reduced opt may shrink binary |
| 3 | true | "s" | 1 | **Most promising** — size-opt reduces paging |
| 4 | true | "z" | 1 | Aggressive size opt (may over-optimize) |
| 5 | "thin" | "s" | 1 | Thin LTO + size-opt combo |

## How to Run A/B Tests Locally

### Quick method: dev mode (no proving, measures cycle counts only)

Dev mode skips cryptographic proving but still executes the guest and reports
accurate cycle counts. This is the fastest way to compare experiments.

```bash
cd risc0-asteroids-verifier

# 1. Edit Cargo.toml with experiment settings (see matrix above)

# 2. Build
cargo build --release -p host

# 3. Run with dev mode + info logging to get cycle counts
RISC0_DEV_MODE=1 RISC0_INFO=1 cargo run --release -p host -- \
  --tape ../test-fixtures/test-short.tape \
  --allow-dev-mode

# The output will show:
#   Total cycles:  <N>
#   User cycles:   <N>
#   Paging cycles: <N>
```

Compare `total_cycles` and `user_cycles` across experiments. The ratio of
`paging_cycles / total_cycles` tells you how much binary size matters.

### Full method: actual proving (measures wall time + cycles)

```bash
# Without dev mode — does real STARK proving
RISC0_INFO=1 time cargo run --release -p host -- \
  --tape ../test-fixtures/test-short.tape \
  --receipt-kind composite \
  --segment-limit-po2 19
```

### With profiling flamegraph (find hotspots)

```bash
# Requires Go installed (for pprof viewer)
RISC0_PPROF_OUT=./profile.pb RISC0_DEV_MODE=1 cargo run --release -p host -- \
  --tape ../test-fixtures/test-short.tape \
  --allow-dev-mode

# View flamegraph in browser
go tool pprof -http=127.0.0.1:8000 profile.pb
```

Set `RISC0_PPROF_ENABLE_INLINE_FUNCTIONS=yes` for more detail at the cost of
profiling overhead.

### A/B test script

A reusable script is checked in at `benchmarks/run_ab_test.sh`. It takes an optional tape path argument:

```bash
cd risc0-asteroids-verifier

# Short tape (default)
./benchmarks/run_ab_test.sh

# Medium tape
./benchmarks/run_ab_test.sh ../test-fixtures/test-medium.tape

# Any tape
./benchmarks/run_ab_test.sh /path/to/your.tape
```

The script patches `Cargo.toml` for each experiment, builds, runs in dev mode,
and restores the original on exit (including on Ctrl-C). Results are saved to
`benchmark_results.txt`.

## Test Results (Feb 2026)

Compiler profile changes had **zero effect** on guest cycle counts. The workspace
`[profile.release]` settings only affect the host binary, not the guest ELF
compiled by the risc0 build system.

### Short tape (500 frames, 528 bytes)

All 6 experiments identical:
```
total=524,288  user=402,525  paging=23,400  segments=1
```
- Paging is 4.5% of total — guest binary is already compact.
- Total = 2^19 (single segment, rounded to segment boundary).

### Medium tape (~3,900 bytes)

All 6 experiments identical:
```
total=6,324,224  user=5,801,454  paging=154,369  segments=13
```
- Paging is 2.4% of total — even lower ratio at scale.
- User cycles dominate (92%) — pure simulation work.

### Conclusion

The guest binary is small enough that compiler flags don't change its page
footprint. Optimization levers for proving time are:
1. **GPU acceleration** (CUDA on Vast.ai) — parallelizes segment proving.
2. **`segment_limit_po2` tuning** — more smaller segments = more GPU parallelism.
3. **Algorithmic changes** in the guest (but the sim is already tight integer math).

## Precompiles

RISC Zero provides hardware-accelerated precompiles for cryptographic ops.
Our guest doesn't use any crypto, so precompiles don't apply. For reference:

- **SHA-256**: 68 cycles per 64-byte block (vs hundreds for software)
- **256-bit modular multiply**: 10 cycles
- **Patched crates**: sha2, k256, p256, curve25519-dalek, rsa, bls12_381, etc.

If we ever add on-chain commitment hashing inside the guest, use the SHA-256
precompile via the patched `sha2` crate.

## Recursion Pipeline (How Receipt Types Work)

Understanding this helps reason about proving time:

```
Execution → Segments → SegmentReceipts (composite)
                            ↓ lift
                       SuccinctReceipts
                            ↓ join (pairwise)
                       Single SuccinctReceipt (~200 KB)
                            ↓ identity_p254
                            ↓ groth16 wrap
                       Groth16Receipt (~300 bytes, on-chain verifiable)
```

- **Composite**: Raw segment proofs. Fastest to generate, largest output.
- **Succinct**: Compressed via lift+join. ~200 KB. Constant-size regardless of computation.
- **Groth16**: SNARK-wrapped. ~300 bytes. Required for Stellar on-chain verification.

We use **groth16** because the Stellar verifier contracts (NethermindEth/stellar-risc0-verifier)
only accept Groth16 receipts.

## Debugging

### GDB for guest programs

```bash
# Install RISC-V GDB
rzup install gdb

# Run guest with debugger attached
r0vm --elf <guest>.bin --with-debugger

# In separate terminal, run the gdb command printed by r0vm
# Then: break main, continue, bt, etc.
```

Requires `debug = true` in guest Cargo profile for symbol info.

## Threshold Gates

Current gates from `benchmarks/thresholds.env`:

| Metric | Short tape | Medium tape |
|--------|-----------|-------------|
| Dev mode max real time | 2.0s | 2.0s |
| Secure mode max real time | 35.0s | 500.0s |
| Secure mode max RSS | 6.5 GB | 11.5 GB |
| Dev mode max total cycles | 700,000 | 7,000,000 |
