# Getting Started: Building ZK Applications on Stellar

## Prerequisites

### Required Software

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Add WebAssembly target
rustup target add wasm32-unknown-unknown

# 3. Install Stellar CLI
cargo install --locked stellar-cli --features opt

# 4. Configure for testnet
stellar network add --global testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"

# 5. Create a test identity
stellar keys generate --global alice --network testnet --fund
```

### For Circom/Groth16 Development

```bash
# Install Circom compiler
npm install -g circom

# Install SnarkJS
npm install -g snarkjs

# Install Node.js (>= 18)
# https://nodejs.org/
```

### For Noir Development

```bash
# Install Nargo (Noir compiler)
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup
```

---

## Quick Start: Verify Your First ZK Proof on Stellar

### Option 1: Clone the Official Examples

```bash
# Clone Soroban examples
git clone https://github.com/stellar/soroban-examples
cd soroban-examples

# Look at ZK-relevant examples
ls -la groth16_verifier/
ls -la privacy-pools/
ls -la bls_signature/
```

### Option 2: Minimal Groth16 Verifier

The `groth16_verifier` example in soroban-examples demonstrates the simplest ZK verification:

```bash
cd soroban-examples/groth16_verifier

# Build
cargo build --target wasm32-unknown-unknown --release

# Test
cargo test

# Deploy to testnet
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/groth16_verifier.wasm \
  --source alice \
  --network testnet
```

---

## Path 1: Groth16 with Circom (Recommended Starting Point)

### Step 1: Write a Circom Circuit

```circom
// circuits/simple.circom
pragma circom 2.0.0;

include "node_modules/circomlib/circuits/poseidon.circom";

template SimpleProof() {
    // Private inputs
    signal input secret;
    signal input nullifier;

    // Public output
    signal output commitment;

    // Compute Poseidon hash
    component hasher = Poseidon(2);
    hasher.inputs[0] <== secret;
    hasher.inputs[1] <== nullifier;

    commitment <== hasher.out;
}

component main = SimpleProof();
```

### Step 2: Compile and Generate Proof

```bash
# Compile circuit
circom circuits/simple.circom --r1cs --wasm --sym -o build/

# Generate Powers of Tau (or use existing ceremony)
snarkjs powersoftau new bn128 12 pot12_0000.ptau
snarkjs powersoftau contribute pot12_0000.ptau pot12_0001.ptau
snarkjs powersoftau prepare phase2 pot12_0001.ptau pot12_final.ptau

# Generate verification key
snarkjs groth16 setup build/simple.r1cs pot12_final.ptau circuit.zkey
snarkjs zkey export verificationkey circuit.zkey verification_key.json

# Create input
echo '{"secret": "123", "nullifier": "456"}' > input.json

# Generate witness
node build/simple_js/generate_witness.js build/simple_js/simple.wasm input.json witness.wtns

# Generate proof
snarkjs groth16 prove circuit.zkey witness.wtns proof.json public.json
```

### Step 3: Convert for Soroban

```bash
# Use circom2soroban (from privacy-pools repo)
circom2soroban vk verification_key.json
circom2soroban proof proof.json
circom2soroban public public.json
```

### Step 4: Deploy Verifier Contract

Use the generated Rust code in your Soroban contract, or use `soroban-verifier-gen`:

```bash
cargo install soroban-verifier-gen
soroban-verifier-gen --vk verification_key.json --output my_verifier/
cd my_verifier
cargo build --target wasm32-unknown-unknown --release
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/my_verifier.wasm \
  --source alice --network testnet
```

---

## Path 2: RISC Zero zkVM

### Step 1: Install RISC Zero

```bash
# Install rzup (RISC Zero toolchain manager)
curl -L https://risczero.com/install | bash
rzup
```

### Step 2: Write a Guest Program

```rust
// guest/src/main.rs
use risc0_zkvm::guest::env;

fn main() {
    // Read private input
    let secret: u64 = env::read();

    // Perform computation
    let result = secret * secret;

    // Commit public output
    env::commit(&result);
}
```

### Step 3: Generate Proof

```rust
// host/src/main.rs
use risc0_zkvm::{default_prover, ExecutorEnv};

fn main() {
    let env = ExecutorEnv::builder()
        .write(&42u64)
        .unwrap()
        .build()
        .unwrap();

    let prover = default_prover();
    let receipt = prover.prove(env, GUEST_ELF).unwrap();

    // receipt.journal contains the public output
    // receipt can be verified on-chain via Soroban's RISC Zero verifier
}
```

### Step 4: Verify on Stellar

Submit the receipt (seal + journal) to the RISC Zero verifier contract deployed on Soroban by Nethermind.

---

## Path 3: Noir / UltraHonk

### Step 1: Create a Noir Project

```bash
nargo new my_zk_app
cd my_zk_app
```

### Step 2: Write a Noir Circuit

```noir
// src/main.nr
fn main(x: Field, y: pub Field) {
    // Prove we know x such that x * x == y
    assert(x * x == y);
}
```

### Step 3: Compile and Prove

```bash
# Edit Prover.toml with inputs
echo 'x = "7"' > Prover.toml
echo 'y = "49"' >> Prover.toml

# Compile
nargo compile

# Prove
nargo prove

# Verify locally
nargo verify
```

### Step 4: Deploy to Soroban

Use the UltraHonk Soroban verifier (see https://github.com/orgs/noir-lang/discussions/8509).

---

## Key Soroban Contract Patterns for ZK

### Calling the Groth16 Verifier

```rust
use soroban_sdk::{contract, contractimpl, Env, Bytes};

#[contract]
pub struct MyZkApp;

#[contractimpl]
impl MyZkApp {
    pub fn verify_and_act(env: Env, proof_bytes: Bytes, pub_signals_bytes: Bytes) -> bool {
        // Call the groth16 verifier contract
        let verifier_client = groth16_verifier::Client::new(&env, &verifier_id);

        // Verify the proof
        let is_valid = verifier_client.verify_proof(
            &vk,           // verification key
            &proof_bytes,  // the proof
            &pub_signals,  // public signals
        );

        if !is_valid {
            panic!("Invalid ZK proof");
        }

        // Proof is valid - execute business logic
        true
    }
}
```

### Using BN254 Host Functions (Protocol 25)

```rust
use soroban_sdk::{contract, contractimpl, Env, Bytes};

#[contractimpl]
impl MyContract {
    pub fn verify_bn254(env: Env) {
        // Point addition on BN254 G1
        let result = env.crypto().bn254_g1_add(&point_a, &point_b);

        // Scalar multiplication
        let result = env.crypto().bn254_g1_mul(&point, &scalar);

        // Multi-pairing check (core of SNARK verification)
        let valid = env.crypto().bn254_multi_pairing_check(&points_g1, &points_g2);
    }
}
```

---

## Resources

| Resource | URL |
|----------|-----|
| Soroban Getting Started | https://developers.stellar.org/docs/build/smart-contracts/getting-started |
| Soroban Examples | https://github.com/stellar/soroban-examples |
| Stellar Developer Discord | https://discord.com/invite/stellardev |
| Stellar ZK Discussion #1500 | https://github.com/orgs/stellar/discussions/1500 |
| Privacy Pools Blog Post | https://stellar.org/blog/ecosystem/prototyping-privacy-pools-on-stellar |
