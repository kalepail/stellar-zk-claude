# Privacy Pools on Stellar

## Overview

Privacy Pools are a privacy-preserving protocol that enable users to deposit funds into a shared pool and later withdraw equivalent amounts while breaking the cryptographic link between deposits and withdrawals. The Stellar implementation incorporates **Association Set Providers (ASPs)** for compliance.

**Blog Post**: https://stellar.org/blog/ecosystem/prototyping-privacy-pools-on-stellar
**Repository**: https://github.com/ymcrcat/soroban-privacy-pools
**Official Example**: https://github.com/stellar/soroban-examples/tree/main/privacy-pools

---

## How It Works

### Deposit Flow

1. User generates two random values: **secret (s)** and **nullifier (n)**
2. Compute commitment: `c = Poseidon(s, n)` over BLS12-381 field elements
3. Send commitment to the Privacy Pool smart contract
4. Contract inserts commitment into a **Merkle tree** and stores the deposited tokens

### Withdrawal Flow

1. User proves via SNARK that they know `s` and `n` whose hash appears in the Merkle tree
2. The proof does NOT reveal `s` or `n` themselves
3. User provides a **nullifier hash** `H(n)` to prevent double-spending
4. The SNARK guarantees the nullifier hash matches the original deposit
5. Contract checks nullifier hasn't been used before
6. Tokens are released to the withdrawer

### Association Sets (Compliance Layer)

Association Set Providers (ASPs) define compliance criteria:
- Users can selectively associate with different compliance standards
- Withdrawal requires proving membership in an approved association set
- Enables privacy while maintaining regulatory compliance
- Regulators can verify compliance without seeing transaction details

---

## Cryptographic Scheme

| Component | Implementation |
|-----------|----------------|
| Proof System | Groth16 |
| Curve | BLS12-381 |
| Hash Function | Poseidon |
| Circuit Language | Circom |
| Proof Generator | SnarkJS |
| Verification Cost | ~40M Soroban instructions |

---

## Architecture

```
                    Off-chain                          On-chain (Soroban)
              +------------------+               +---------------------+
              |   Circom Circuit |               |  Privacy Pool       |
              |   (main.circom)  |               |  Contract           |
              +--------+---------+               |  - deposit()        |
                       |                         |  - withdraw()       |
              +--------v---------+               |  - updateAssoc()    |
              |   SnarkJS        |               +----------+----------+
              |   (prove)        |                          |
              +--------+---------+               +----------v----------+
                       |                         |  Groth16 Verifier   |
              +--------v---------+               |  Contract           |
              | circom2soroban   |               |  - verify_proof()   |
              | (convert outputs)|               +---------------------+
              +------------------+
```

---

## Key Files in the Implementation

### Circom Circuit (main.circom)
- Proves a coin was previously deposited into the mixer
- Takes inputs: coin parameters (s, n), Merkle root, sibling path
- Outputs: nullifier hash
- **Note**: Recipient address binding for frontrunning protection is listed as future work

### Smart Contract (contract/src/lib.rs)
- `deposit()` - Accepts commitments and tokens
- `withdraw()` - Verifies ZK proof, checks nullifier, releases tokens
- Interacts with separate Groth16 verifier contract
- Manages incremental Merkle tree

### circom2soroban Tool
- Converts Circom JSON outputs to Rust code for Soroban
- Handles verification keys, proofs, and public inputs
- Works with BLS12-381 field elements

### coinutils CLI
- `generate` - Creates coins with cryptographic parameters
- `withdraw` - Reconstructs Merkle tree, generates SNARK inputs
- `updateAssociation` - Manages ASP Merkle trees

---

## Running the Demo

From the official `soroban-examples/privacy-pools` directory:

```bash
# Build contracts
cd privacy-pools
make build

# Deploy groth16_verifier contract
stellar contract deploy --wasm ../groth16_verifier/target/wasm32-unknown-unknown/release/groth16_verifier.wasm \
  --source alice --network testnet

# Deploy privacy-pools contract
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/privacy_pools.wasm \
  --source alice --network testnet

# Generate a coin
cargo run --bin coinutils -- generate

# Deposit the coin
stellar contract invoke --id <privacy_pool_contract> --source alice --network testnet \
  -- deposit --commitment <hex_commitment> --amount 1000000

# Generate withdrawal proof using snarkjs
snarkjs groth16 prove circuits/circuit.zkey circuits/witness.wtns proof.json public.json

# Convert proof for Soroban
cargo run --bin circom2soroban -- proof proof.json
cargo run --bin circom2soroban -- public public.json

# Withdraw
stellar contract invoke --id <privacy_pool_contract> --source alice --network testnet \
  -- withdraw --proof-bytes <proof> --pub-signals-bytes <signals>
```

---

## Inspiration: 0xbow Privacy Pools (Ethereum)

The Stellar implementation is directly inspired by:
- **Repository**: https://github.com/0xbow-io/privacy-pools-core
- **Paper**: Privacy Pools by Vitalik Buterin et al.
- **Reference**: https://papers.ssrn.com/sol3/papers.cfm?abstract_id=4563364

---

## Nethermind 3-Phase Privacy Roadmap

Revealed by Nick Dimitriou at Meridian 2025 ("Private Transactions for Stellar" - https://www.youtube.com/watch?v=JFgDnMBTwAE):

### Phase 1: Privacy Pools with Association Sets (Current)
- Core privacy pool functionality with deposit/withdraw
- Association Set Providers (ASPs) for compliance
- Groth16 + BLS12-381 proof system
- Users prove membership in approved association sets

### Phase 2: View Keys
- **Partial view keys**: Allow specific parties to see transaction details
- **Full view keys**: Complete transaction visibility for auditors
- Enables regulatory compliance without breaking privacy for other users
- Auditors can verify transaction legitimacy without pool-wide visibility

### Phase 3: Compliant In-Pool Transfers
- Transfer value within the privacy pool without withdraw/re-deposit
- Maintain compliance through association set membership
- Significantly reduces gas costs for repeated private transactions
- Enables practical day-to-day private payment flows

---

## Future Work (per SDF blog)

1. **Frontrunning protection** - Include recipient address in ZK proof
2. **Rolling roots** - Store last N roots to allow withdrawals during new deposits
3. **Rage-quit** - Allow public withdrawal if not approved via association set
4. **SnarkJS extension** - Generate Soroban-compatible verification code directly
