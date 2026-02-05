# Developer Tools for ZK on Stellar

## Core Toolchain

### Soroban CLI
**Install**: `cargo install --locked stellar-cli --features opt`
**Purpose**: Compile, test, deploy, and invoke Soroban smart contracts
**Docs**: https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup

### Soroban Rust SDK
**Repository**: https://github.com/stellar/rs-soroban-sdk
**Purpose**: Foundation for all Soroban contract development including ZK verifiers

---

## ZK-Specific Tools

### circom2soroban

**Location**: Part of the soroban-privacy-pools repository
**Language**: Rust
**Purpose**: Converts Circom JSON outputs into Rust code embeddable in Soroban contracts

#### Usage

```bash
# Convert verification key
circom2soroban vk verification_key.json

# Convert proof
circom2soroban proof proof.json

# Convert public inputs
circom2soroban public public.json
```

#### What It Does
- Accepts verification keys, SNARK proofs, or public inputs in JSON format (from Circom/SnarkJS)
- Outputs Rust variables that can be pasted directly into contracts
- Also outputs byte conversion for dynamic contract invocation
- Works with BLS12-381 field elements
- Uses decimal string representation for readability, hex for contract integration

---

### coinutils CLI

**Location**: Part of the soroban-privacy-pools repository
**Purpose**: Manages the lifecycle of privacy pool coins

#### Commands

```bash
# Generate a new coin with cryptographic parameters
coinutils generate

# Prepare withdrawal (reconstructs Merkle tree, generates SNARK inputs)
coinutils withdraw --coin coin.json --state state.json

# Update association set
coinutils updateAssociation --depth 20 --labels labels.json
```

#### How It Works
- `generate`: Creates secret, nullifier, and Poseidon commitment over BLS12-381
- `withdraw`: Reconstructs Merkle tree from on-chain state, generates Merkle proofs
- `updateAssociation`: Manages ASP Merkle trees

---

### soroban-verifier-gen

**Repository**: https://github.com/mysteryon88/soroban-verifier-gen
**Crate**: https://lib.rs/crates/soroban-verifier-gen
**Purpose**: Automatically generates complete Soroban smart contracts for Groth16 proof verification

#### Usage

```bash
cargo install soroban-verifier-gen

# Generate a verifier contract from a circom/snarkjs verification key
soroban-verifier-gen --vk verification_key.json --output verifier_contract/
```

#### What It Does
- Takes a verification key in circom/snarkjs JSON format
- Generates a complete, deployable Soroban smart contract
- No deep cryptographic expertise required
- Released January 2026

---

### OpenZKTool (Xcapit Labs)

**Repository**: https://github.com/xcapit/openzktool
**Stars**: 13 | **Languages**: TypeScript, Rust
**Purpose**: Complete ZK-SNARK toolkit for Soroban using **Circom + Groth16 on BN254**

#### Features
- Multiple Circom circuits (KYC, range proof, compliance, solvency)
- Full BN254 pairing implementation in Rust (field arithmetic, Miller loop, final exponentiation)
- TypeScript SDK for frontend integration
- 49+ tests
- **Deployed testnet**: `CBPBVJJW5NMV4UVEDKSR6UO4DRBNWRQEMYKRYZI3CW6YK3O7HAZA43OI`
- **Key contributor**: fboiero (111 commits)

### zk-examples/zk-soroban-examples

**Repository**: https://github.com/zk-examples/zk-soroban-examples
**Purpose**: Multi-framework ZK examples for Stellar
- Covers: arkworks, circom, gnark, noname
- **Created**: January 26, 2026

---

## Circuit Development Tools

### Circom

**Docs**: https://docs.circom.io/getting-started/proving-circuits/
**Purpose**: DSL for describing ZK circuits

```circom
// Example: Simple hash proof
template HashProof() {
    signal input secret;
    signal input nullifier;
    signal output commitment;

    component hasher = Poseidon(2);
    hasher.inputs[0] <== secret;
    hasher.inputs[1] <== nullifier;
    commitment <== hasher.out;
}
```

### SnarkJS

**Purpose**: Proof generation, verification key generation, Powers-of-Tau ceremonies
**Used for**: Groth16 proofs on Stellar

```bash
# Compile circuit
circom main.circom --r1cs --wasm --sym

# Generate trusted setup
snarkjs groth16 setup circuit.r1cs pot_final.ptau circuit.zkey

# Generate proof
snarkjs groth16 prove circuit.zkey witness.wtns proof.json public.json

# Verify proof (off-chain test)
snarkjs groth16 verify verification_key.json public.json proof.json
```

### Noir (Aztec)

**Docs**: https://noir-lang.org/docs/
**Purpose**: Rust-like language for building ZK circuits, backend-agnostic

```noir
// Example: Simple proof
fn main(x: Field, y: pub Field) {
    assert(x != y);
}
```

### Nargo

**Purpose**: Noir compiler and package manager

```bash
# Create new Noir project
nargo new my_circuit

# Compile
nargo compile

# Generate proof
nargo prove

# Verify
nargo verify
```

---

## OpenZeppelin Stellar Ecosystem

OpenZeppelin has built a comprehensive developer tools ecosystem for Stellar/Soroban that goes far beyond basic security:

### Stellar Contracts Library
**URL**: https://github.com/OpenZeppelin/stellar-contracts
**Stars**: 75 | **Last Push**: February 2, 2026
**Purpose**: Production-grade Soroban contract library used by ZK projects (e.g., NethermindEth/stellar-risc0-verifier uses v0.6.0 for access control)

### Contract Wizard
**URL**: https://wizard.openzeppelin.com/stellar
**Purpose**: Generate and deploy secure, audited Stellar contracts. Select templates and options, then download as a single file, Rust package, or Scaffold Stellar Package.

### Builder UI
**URL**: https://builder.openzeppelin.com/
**Purpose**: Instantly generate a React UI for any Stellar contract. Takes you from prototype to production-ready frontends in seconds.

### MCP Server
**URL**: https://mcp.openzeppelin.com
**Purpose**: Extensions of the OpenZeppelin Contract Wizard that allow you to write and edit smart contracts with natural language prompting. Click on the Stellar Contracts card and follow instructions to install.

### Relayer
**Docs**: https://developers.stellar.org/docs/tools/openzeppelin-relayer
**Purpose**: Managed service for submitting smart contract transactions to Stellar. Abstracts away transaction building, signing, and submission (including fee handling and parallel processing).

### Soroban Security Detector SDK
**URL**: https://github.com/OpenZeppelin/soroban-security-detectors-sdk
**Stars**: 6
**Purpose**: Automated vulnerability detection for Soroban smart contracts

### Additional OpenZeppelin Stellar Tools
- **Monitor** (119 stars): https://github.com/OpenZeppelin/openzeppelin-monitor — on-chain activity monitoring
- **UI Builder** (31 stars): https://github.com/OpenZeppelin/ui-builder — frontend components
- **Stellar Upgrader CLI** (3 stars): https://github.com/OpenZeppelin/stellar-upgrader-cli — contract upgrades
- **Soroban Helpers** (1 star): https://github.com/OpenZeppelin/soroban-helpers — utility helpers
- **Role Manager** (1 star): https://github.com/OpenZeppelin/role-manager — access control

**Note**: `OpenZeppelin/openzeppelin-confidential-contracts` (143 stars) targets Ethereum FHE (Zama FHEVM), NOT Stellar.

---

## Wallet Development Tools

### Stellar Wallets Kit
**URL**: https://stellarwalletskit.dev/
**Purpose**: A kit to handle all Stellar wallets at once with a simple API without caring about individual configurations for each one of them.

### Stellar Wallet Playbook
**URL**: https://stellarplaybook.com/
**Purpose**: Design guide for building Stellar wallets. Helps navigate key design choices for building secure, user-friendly wallets ready for scale.

---

## Testing & Development Infrastructure

### Stellar Lab
**Docs**: https://developers.stellar.org/docs/tools/lab
**Purpose**: Web-based tool for development, experimenting, and testing, as well as exploring APIs developers use to interact with the Stellar network.

### Quickstart
**Docs**: https://developers.stellar.org/docs/tools/quickstart
**Purpose**: Docker container that runs a local Stellar network environment (node), allowing developers to run a local version of the Stellar network for development and testing.

### Soroban Fuzzing Framework
- Built-in fuzzing support for Soroban contracts
- Use `panic_with_error!` macro (not bare `panic!`) for proper fuzzer behavior
- Critical for ZK contract security testing

### OpenZeppelin Relayer
**Docs**: https://developers.stellar.org/meetings
**Purpose**: Manages Soroban transaction submission with automatic parallel processing and fee management

### Scaffold Stellar
**Repository**: https://github.com/AhaLabs/scaffold-stellar
**Docs**: https://developers.stellar.org/docs/tools/developer-tools/scaffold-stellar
**Purpose**: Developer toolkit for building dapps and smart contracts on Stellar. Provides CLI tools, reusable contract templates, a smart contract registry, and a modern frontend to go from idea to working full-stack dapp faster.

---

## Language & SDK Support

| Language | SDK | ZK Use |
|----------|-----|--------|
| Rust | soroban-sdk | Primary for all ZK contracts |
| Circom | - | Circuit description for Groth16 |
| Noir | nargo | Circuit description for UltraHonk |
| JavaScript | stellar-sdk, snarkjs | Proof generation, frontend |
| Python | - | Off-chain computation |

---

## Development Workflow

```
1. Design ZK circuit (Circom or Noir)
2. Compile circuit
3. Generate trusted setup (Groth16) or compile to prover (Noir/RISC Zero)
4. Generate test proofs off-chain
5. Convert proof artifacts for Soroban (circom2soroban or soroban-verifier-gen)
6. Write Soroban verifier contract (or use generated one)
7. Test locally with soroban-cli
8. Deploy to testnet
9. Test end-to-end
10. Deploy to mainnet
```
