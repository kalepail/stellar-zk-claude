# Getting Started: Building ZK on Stellar

Last reviewed: February 2026.

## 1) Base Setup
```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Stellar CLI
cargo install --locked stellar-cli --features opt

# Testnet config
stellar network add --global testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"
stellar keys generate --global alice --network testnet --fund
```

## 2) Pick One Proving Path

### Path A: Groth16 (best first path for most teams)
1. Clone `stellar/soroban-examples`.
2. Start from `groth16_verifier` and `privacy-pools`.
3. Use Circom + SnarkJS for proof artifacts.
4. Convert artifacts with `circom2soroban` or generate verifier contracts via
   `soroban-verifier-gen`.

### Path B: RISC Zero (best for complex logic / replay proofs)
1. Set up RISC Zero toolchain (`rzup`).
2. Implement deterministic guest logic in Rust.
3. Prove and verify locally using host runner.
4. Integrate with Stellar verifier submission flow.

### Path C: Noir / UltraHonk (advanced path)
1. Install Noir (`noirup`, `nargo`).
2. Build/test small circuits first.
3. Benchmark proving and verification limits before scaling workloads.
4. Validate Soroban verifier budget and contract-size fit early.

## 3) End-to-End Minimum Milestone
- Produce one valid proof for a meaningful statement.
- Verify on testnet contract path.
- Add regression tests for malformed inputs and invalid proofs.
- Capture expected instruction/cost envelope before mainnet planning.

## 4) Testnet References (from current research docs)
- RISC Zero router: `CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD`
- RISC Zero Groth16 verifier: `CB54QOGYJJOSLNHRCHTSVGKJ3D5K6B5YO7DD6CRHRBCRNPF2VX2VCMV7`
- RISC Zero mock verifier: `CCKXGODVBNCGZZIKTU2DIPTXPVSLIG5Z67VYPAL4X5HVSED7VI4OD6A3`

Treat these as operational references and re-verify before production use.
