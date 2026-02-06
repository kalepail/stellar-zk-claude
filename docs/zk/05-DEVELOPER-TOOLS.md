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
- `rzup` / RISC Zero toolchain
- Host/guest Rust workspace pattern
- Optional proving marketplace integration (for scaling)

### Typical flow
1. Implement deterministic guest logic.
2. Prove via host runner.
3. Verify receipts in local and testnet environments.
4. Integrate verifier contract submission path.

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
