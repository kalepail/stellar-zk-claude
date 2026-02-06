# Stellar ZK Overview

Last reviewed: February 2026.

## What Is Live
- Protocol 25 (X-Ray) is live on mainnet.
- BN254 host functions (CAP-0074) are live.
- Poseidon/Poseidon2 host support (CAP-0075) is live.
- BLS12-381 host support (CAP-0059) remains available.

## What This Enables
Stellar now supports practical on-chain verification for multiple proving paths:
- Groth16 (circuit-first, compact verification)
- RISC Zero (program-first, complex deterministic workloads)
- Noir/UltraHonk (active integration path, still constraint-sensitive)

## Practical Guidance
1. Use `03-PROVING-SYSTEMS.md` to pick your proving stack by workload.
2. Use `11-APPLICATION-VERIFICATION-PATTERNS.md` when designing deterministic
   replay/state-transition verification.
3. Use `10-SECURITY-BEST-PRACTICES.md` before production rollout.

## Key Links
- Protocol 25 announcement: <https://stellar.org/blog/developers/announcing-stellar-x-ray-protocol-25>
- Protocol upgrades page: <https://stellar.org/protocol-upgrades>
- Stellar ZK learn page: <https://stellar.org/learn/zero-knowledge-proof>
- CAP repo: <https://github.com/stellar/stellar-protocol/tree/master/core>
