# ZK Asteroids Score Token — Full System Specification

## Goal

Build an end-to-end system where:

1. A player plays Asteroids in the browser, authenticated via **passkey** (no seed phrase)
2. The game tape is sent to a **Cloudflare Worker** backend, which enqueues it for proving on a **VAST AI GPU endpoint**
3. The Groth16 proof comes back through the queue to the client
4. The client submits the proof to a **Soroban smart contract** that verifies it on-chain via Nethermind's deployed **RISC Zero verifier**
5. On successful verification, the contract **mints the score as tokens** to the player's smart wallet
6. The **OpenZeppelin Relayer** pays all XLM transaction fees — the player never needs XLM

---

## System Architecture

```
 Browser (Client)                  Cloudflare                         VAST AI GPU
┌─────────────────────┐     ┌──────────────────────┐     ┌──────────────────────┐
│                     │     │                      │     │                      │
│  Asteroids Game     │     │  CF Worker (API)     │     │  RISC0 Prover        │
│  (TypeScript)       │     │    │                 │     │  API Server          │
│                     │     │    ▼                 │     │                      │
│  Records tape ──────┼────▶│  CF Queue (produce)  │     │  POST /api/jobs/     │
│                     │     │    │                 │     │  prove-tape/raw      │
│  SmartAccountKit    │     │    ▼                 │     │  ?receipt_kind=      │
│  (smart wallet)     │     │  CF Worker (consumer)│────▶│   groth16            │
│                     │     │    │                 │     │                      │
│                     │     │    │  poll job status │◀───│  GET /api/jobs/{id}  │
│  ◀────────────────────────│    │                 │     │                      │
│  receives proof     │     │    ▼                 │     └──────────────────────┘
│                     │     │  Return proof to     │
│  Signs auth entry   │     │  client via response │
│  with passkey ──────┼──┐  │  or WebSocket        │
│                     │  │  └──────────────────────┘
└─────────────────────┘  │
                         │
                         │   Stellar Network (Soroban)
                         │  ┌───────────────────────────────────────────────┐
                         │  │                                               │
                         │  │  ┌─────────────────────┐                     │
                         └──┼─▶│ OpenZeppelin Relayer │                     │
                            │  │ (pays XLM fees)      │                     │
                            │  └──────────┬──────────┘                     │
                            │             │                                 │
                            │             ▼                                 │
                            │  ┌─────────────────────┐   ┌──────────────┐  │
                            │  │ Asteroids Score      │──▶│ RISC Zero    │  │
                            │  │ Contract             │   │ Router       │  │
                            │  │                      │   │ (Nethermind) │  │
                            │  │ submit_score()       │   └──────┬───────┘  │
                            │  │   ├ sha256(journal)  │          │          │
                            │  │   ├ verify proof ────┼──────────┘          │
                            │  │   ├ decode score     │                     │
                            │  │   ├ check replay     │   ┌──────────────┐  │
                            │  │   └ mint tokens ─────┼──▶│ Score Token  │  │
                            │  │                      │   │ (SAC)        │  │
                            │  └─────────────────────┘   └──────────────┘  │
                            │                                               │
                            │  ┌─────────────────────┐                     │
                            │  │ Smart Account        │                     │
                            │  │ Factory (OZ)         │                     │
                            │  │ + WebAuthn Verifier  │                     │
                            │  └─────────────────────┘                     │
                            └───────────────────────────────────────────────┘
```

---

## Part 1: Player Authentication (Passkey Smart Wallets)

### Technology Stack

| Component | Implementation |
|-----------|---------------|
| Smart Wallet SDK | `smart-account-kit` (OZ Smart Accounts) |
| Auth Standard | WebAuthn / FIDO2 (secp256r1) |
| On-chain Verification | Stellar Protocol 21 native secp256r1 |
| Account Pattern | `CustomAccountInterface` with `__check_auth` |
| Factory | Deterministic account deployment via factory contract |
| Relay | OpenZeppelin Relayer (`@openzeppelin/relayer-plugin-channels`) |

### How It Works

1. **First visit**: Browser calls `SmartAccountKit.createWallet()` which:
   - Triggers WebAuthn `navigator.credentials.create()` — user confirms with biometric/PIN
   - Extracts secp256r1 public key from the credential
   - Deploys a smart account contract via the OZ factory (deterministic address from `keyId` salt)
   - Returns `contractId` (the player's on-chain address)

2. **Return visits**: `SmartAccountKit.connectWallet()` derives the `contractId` deterministically from the credential's `keyId`, or falls back to an indexer lookup.

3. **Signing transactions**: When the player submits a proof, the SDK calls `SmartAccountKit.sign()` which:
   - Iterates auth entries from transaction simulation
   - For each entry, computes the `SorobanAuthorization` preimage hash
   - Triggers WebAuthn `navigator.credentials.get()` — user confirms with biometric
   - Returns a compact 64-byte secp256r1 signature (low-S normalized)

### NPM Packages Required

```
smart-account-kit        # OZ Smart Account SDK (wallet creation, signing, relay)
@stellar/stellar-sdk     # Stellar/Soroban core
@openzeppelin/relayer-plugin-channels  # OZ Relayer integration
```

### Key Consideration: Contract Accounts Cannot Pay Fees

Smart wallet contracts (C-accounts) **cannot** serve as transaction sources — they cannot sign transaction envelopes or consume sequence numbers. This is why a relay is mandatory: the relay's G-account serves as the transaction source and pays fees, while the player's smart wallet only signs **auth entries** (not the transaction envelope itself).

---

## Part 2: Fee Sponsorship (Relay Infrastructure)

### Auth-Entry Signing Pattern (NOT Fee-Bump)

For Soroban contract invocations with smart wallets, the correct pattern is **auth-entry signing**, not fee-bump transactions. (There is a known Soroban fee-bump bug in the multidimensional fee model — fee-bumps are unreliable for Soroban txns.)

**Flow:**

1. Client builds a transaction invoking `submit_score()` on the Score Contract
2. Client simulates in **Recording Mode** → gets auth entries that need signatures
3. Client signs auth entries using passkey (via `SmartAccountKit.sign()`)
4. Client re-simulates in **Enforcing Mode** to validate signatures
5. Client sends the signed transaction XDR to the backend relay
6. Relay validates: transaction source is not an authorized address, auth entries don't reference the relay
7. Relay rebuilds the transaction with its own G-account as source
8. Relay simulates in Enforcing Mode, signs the transaction envelope, submits to Stellar

### OpenZeppelin Relayer

- Production-grade relay infrastructure
- Supports fee abstraction (user pays in USDC instead of XLM, or free)
- `FeeForwarder` contract for atomic fee collection
- `@openzeppelin/relayer-plugin-channels` npm package
- `SmartAccountServer.send()` integrates directly

### Backend Relay Endpoint

```typescript
// POST /api/send
async function sendTransaction(req) {
    const { xdr } = req.body;

    // SmartAccountServer handles relay submission via OZ Relayer
    const server = new SmartAccountServer({
        rpcUrl: RPC_URL,
        networkPassphrase: NETWORK_PASSPHRASE,
        relayerUrl: RELAYER_URL,
        relayerApiKey: RELAYER_API_KEY,
    });

    const result = await server.send(xdr);
    return { hash: result.hash };
}
```

---

## Part 3: Cloudflare Worker Queue (Proving Pipeline)

### Architecture: CF Workflows

Use **Cloudflare Workflows** for the proving pipeline. Workflows provide durable multi-step execution with automatic retries, sleep/polling, and state persistence — ideal for the long-running prove cycle (can take minutes to hours for Groth16).

```
Client POST /prove  →  CF Worker (API)  →  Workflow Instance
                                              │
                                    step 1: submit tape to VAST AI
                                              │
                                    step 2: poll for completion (sleep + retry)
                                              │
                                    step 3: extract proof components
                                              │
                                    step 4: return to client
```

### Workflow Definition

```typescript
import { WorkflowEntrypoint, WorkflowStep, WorkflowEvent } from 'cloudflare:workers';

export class ProveWorkflow extends WorkflowEntrypoint {
    async run(event: WorkflowEvent, step: WorkflowStep) {
        const { tapeBytes, playerAddress } = event.payload;

        // Step 1: Submit tape to VAST AI prover
        const jobId = await step.do('submit-tape', async () => {
            const response = await fetch(
                `${VAST_AI_URL}/api/jobs/prove-tape/raw?receipt_kind=groth16`,
                { method: 'POST', body: tapeBytes, headers: { 'X-API-Key': VAST_API_KEY } }
            );
            const { job_id } = await response.json();
            return job_id;
        });

        // Step 2: Poll for proof completion (with backoff)
        const proof = await step.do('poll-proof', { retries: { limit: 60, delay: '10 seconds' } },
            async () => {
                const response = await fetch(`${VAST_AI_URL}/api/jobs/${jobId}`);
                const job = await response.json();

                if (job.status === 'failed') throw new Error(`Proof failed: ${job.error}`);
                if (job.status !== 'succeeded') throw new Error('Still proving...'); // triggers retry

                return job.result.proof;
            }
        );

        // Step 3: Extract on-chain proof components
        const components = await step.do('extract-components', async () => {
            return {
                seal: extractSealBytes(proof.receipt),
                imageId: VERIFY_TAPE_ID,
                journalRaw: extractJournalBytes(proof.receipt),
                score: proof.journal.final_score,
            };
        });

        return components;
    }
}
```

### Client Polling

The client either:
- **Option A**: Polls a CF Worker endpoint (`GET /prove/{workflow_id}`) for status
- **Option B**: Connects via WebSocket to a Durable Object that gets notified when the workflow completes
- **Option C**: Uses `step.waitForEvent()` in the workflow and the client is notified via push

### Wrangler Config

```jsonc
{
    "workflows": [{
        "name": "prove-workflow",
        "binding": "PROVE_WORKFLOW",
        "class_name": "ProveWorkflow"
    }]
}
```

---

## Part 4: Soroban Score Contract

### Contracts Involved

| Contract | Role | Status |
|----------|------|--------|
| **RISC Zero Router** | Dispatches `verify()` by seal selector | Deployed (Nethermind) |
| **Groth16 Verifier** | BN254 pairing check on Groth16 proof | Deployed (Nethermind) |
| **Asteroids Score Contract** | Verifies proof, mints tokens | **To Build** |
| **Score Token (SAC)** | Standard Stellar asset for score tokens | **To Deploy** |
| **Smart Account Factory** | Creates passkey wallets | Deployed (OZ Smart Accounts) |
| **WebAuthn Verifier** | Verifies secp256r1 passkey signatures | Deployed (OZ Smart Accounts) |

### Deployed Addresses (Testnet)

| Contract | Address |
|----------|---------|
| RISC Zero Router | `CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD` |
| Groth16 Verifier | `CB54QOGYJJOSLNHRCHTSVGKJ3D5K6B5YO7DD6CRHRBCRNPF2VX2VCMV7` |
| Mock Verifier | `CCKXGODVBNCGZZIKTU2DIPTXPVSLIG5Z67VYPAL4X5HVSED7VI4OD6A3` |

### RISC Zero Verifier Interface

From `NethermindEth/stellar-risc0-verifier` — the **exact** function signature:

```rust
fn verify(
    env: Env,
    seal: Bytes,            // Variable-length: 4-byte selector + 256-byte Groth16 proof (260 bytes)
    image_id: BytesN<32>,   // SHA-256 of guest program ELF
    journal: BytesN<32>,    // SHA-256(raw_journal_bytes) — NOT the raw journal itself
) -> Result<(), VerifierError>;
```

**Critical**: `journal` parameter is `SHA-256(raw_journal_bytes)`. The raw journal must be hashed before calling `verify()`.

### Seal Format (260 bytes)

```
[4 bytes: selector]  [64 bytes: G1 point A]  [128 bytes: G2 point B]  [64 bytes: G1 point C]
```

### Score Contract — Storage

```rust
#[contracttype]
pub enum DataKey {
    Admin,                   // Address
    TokenId,                 // Address — score token SAC
    RouterId,                // Address — RISC Zero router contract
    ImageId,                 // BytesN<32> — expected guest program image ID
    Claimed(BytesN<32>),     // journal_digest → bool (prevents replay)
}
```

### Score Contract — Functions

#### `initialize`

```rust
pub fn initialize(
    env: Env,
    admin: Address,
    token: Address,
    router: Address,
    image_id: BytesN<32>,
)
```

Sets admin, token, router, and expected image ID. Called once.

#### `submit_score`

```rust
pub fn submit_score(
    env: Env,
    player: Address,
    seal: Bytes,
    image_id: BytesN<32>,
    journal_raw: Bytes,
) -> u32
```

**Steps:**

1. `player.require_auth()` — player's smart wallet signs via passkey
2. Verify `image_id` matches stored value (only our Asteroids guest program accepted)
3. `let journal_digest: BytesN<32> = env.crypto().sha256(&journal_raw).into()`
4. Check `DataKey::Claimed(journal_digest)` doesn't exist (replay protection)
5. Call RISC Zero verifier:
   ```rust
   let router_client = RiscZeroVerifierRouterClient::new(&env, &router_addr);
   router_client.verify(&seal, &image_id, &journal_digest);
   // Panics on failure → entire transaction rolls back
   ```
6. Decode `final_score` from `journal_raw` (bytes 8..12, little-endian u32)
7. Store `DataKey::Claimed(journal_digest) = true`
8. Mint tokens:
   ```rust
   let token_client = token::StellarAssetClient::new(&env, &token_addr);
   token_client.mint(&player, &(score as i128));
   // Auto-authorized because this contract IS the token admin (direct caller)
   ```
9. Emit event, return score

#### Read-only Functions

- `get_image_id(env) -> BytesN<32>`
- `is_claimed(env, journal_digest: BytesN<32>) -> bool`
- `set_image_id(env, new_image_id: BytesN<32>)` — admin-only

### Journal Decoding

The raw journal (24 bytes) from `env::commit(&VerificationJournal)`:

```
Offset  Size  Field
0       4     seed (u32 LE)
4       4     frame_count (u32 LE)
8       4     final_score (u32 LE)     ← mint this many tokens
12      4     final_rng_state (u32 LE)
16      4     tape_checksum (u32 LE)
20      4     rules_digest (u32 LE)
```

**Caveat**: RISC Zero's `env::commit()` serialization format needs empirical verification. Run a test proof, hex-dump `receipt.journal.bytes`, and confirm offsets before hardcoding.

### Token Setup

**Approach: Classic Stellar Asset wrapped as SAC**

1. Create classic asset `ZKAST` with an issuer keypair
2. `stellar contract asset deploy --asset ZKAST:GISSUER...` → SAC contract ID
3. Deploy Asteroids Score Contract
4. `set_admin` on the SAC to transfer admin to the Score Contract
5. `initialize` the Score Contract with SAC address

**Token Config:**
- Decimals: **0** (scores are whole numbers)
- Symbol: `ZKAST` (≤12 chars)
- Amount type: `i128`. With 0 decimals, `mint(&player, &score_as_i128)` mints exactly `score` tokens

---

## Part 5: End-to-End Flow

### Player Onboarding (First Visit)

```
1. Player opens game in browser
2. Game calls SmartAccountKit.createWallet()
   → Browser shows "Create passkey" prompt (biometric/PIN)
   → WebAuthn credential created (secp256r1 keypair on device)
   → OZ factory contract deploys smart wallet (deterministic address)
3. Player's smart wallet address stored in browser + backend DB
4. Player can immediately play — no XLM needed
```

### Gameplay → Proof → Tokens

```
1. Player plays Asteroids, game records input tape
2. Game over → client sends tape to CF Worker:
   POST https://worker.example.com/prove
   Body: raw tape bytes

3. CF Worker creates a Workflow instance:
   - Submits tape to VAST AI: POST /api/jobs/prove-tape/raw?receipt_kind=groth16
   - Polls for completion (retries with backoff)
   - Extracts: seal (260 bytes), image_id (32 bytes), journal_raw (24 bytes)
   - Returns proof components to client

4. Client receives proof components

5. Client builds Soroban transaction:
   - Invokes score_contract.submit_score(player, seal, image_id, journal_raw)
   - Simulates in Recording Mode → gets auth entries
   - Signs auth entries with passkey (WebAuthn assertion, biometric confirm)
   - Re-simulates in Enforcing Mode to validate

6. Client sends signed transaction XDR to backend:
   POST https://worker.example.com/api/send
   Body: { xdr: "base64..." }

7. Backend relay (via SmartAccountServer.send() / OZ Relayer):
   - Validates transaction
   - Rebuilds with relay's G-account as source
   - Submits to Stellar network
   - Relay pays all XLM fees

8. On-chain execution:
   - Soroban host calls smart wallet's __check_auth
   - WebAuthn verifier validates passkey signature
   - Score Contract verifies RISC Zero proof via router
   - Score Contract mints ZKAST tokens to player
   - Score Contract emits event

9. Client polls transaction status, shows "Score verified! +{score} ZKAST"
```

---

## Part 6: Security Considerations

### Replay Protection

Each proof is uniquely identified by `SHA-256(journal_raw)`. The journal contains `seed + frame_count + final_score + final_rng_state + tape_checksum + rules_digest`. Any distinct gameplay produces a distinct journal. The contract stores claimed digests in persistent storage.

### Image ID Pinning

Only proofs from our specific Asteroids guest program (`VERIFY_TAPE_ID`) are accepted. A valid proof from a different guest program is rejected.

### Frontrunning

The proof doesn't bind to a player address. If proof data leaks before submission, someone else could claim it. Mitigations:
- `player.require_auth()` means the original player must sign
- The passkey signature is embedded in the auth entry before reaching the relay
- For stronger binding: future guest program version could commit player address into the journal

### Smart Wallet Security

- `__check_auth` validates secp256r1 signature against stored public key
- Context rules can restrict which contracts/functions the passkey can invoke
- Policies can enforce rate limits (e.g., max 1 proof submission per minute)
- Passkey private key never leaves the user's device (TPM/Secure Enclave)

### Relay Safety

- Relay always validates: transaction source ≠ any authorized address
- Relay simulates in Enforcing Mode before signing (catches failures before paying fees)
- Auth entries have ledger-based expiration (anti-replay)
- Rate limiting at relay level prevents fee exhaustion attacks

### Score Overflow

Score is `u32` (max ~4.29B). Minting as `i128` cannot overflow.

---

## Part 7: Deployment Steps

### 1. Deploy Smart Wallet Infrastructure

If not already deployed:
- Smart Account Factory contract
- WebAuthn secp256r1 Verifier contract
- (These may already be deployed by OZ Smart Accounts on testnet)

### 2. Create and Deploy Score Token

```bash
stellar keys generate --global zkast-issuer --network testnet --fund

stellar contract asset deploy \
  --asset ZKAST:$(stellar keys address zkast-issuer) \
  --source zkast-issuer \
  --network testnet
# → Note the SAC contract ID
```

### 3. Build and Deploy Score Contract

```bash
cd contracts/asteroids-score
cargo build --target wasm32-unknown-unknown --release

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/asteroids_score.wasm \
  --source zkast-issuer \
  --network testnet
# → Note the Score Contract ID
```

### 4. Initialize Score Contract

```bash
stellar contract invoke --id <SCORE_CONTRACT> --source zkast-issuer --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --token <SAC_CONTRACT_ID> \
  --router CCYKHXM3LO5CC6X26GFOLZGPXWI3P2LWXY3EGG7JTTM5BQ3ISETDQ3DD \
  --image_id <VERIFY_TAPE_ID_HEX>
```

### 5. Transfer SAC Admin to Score Contract

```bash
stellar contract invoke --id <SAC_CONTRACT_ID> --source zkast-issuer --network testnet \
  -- set_admin \
  --new_admin <SCORE_CONTRACT_ADDRESS>
```

### 6. Deploy Cloudflare Worker + Workflow

```bash
cd workers/prove-worker
npx wrangler deploy
```

### 7. Configure Relay

- Configure OpenZeppelin Relayer with API key
- Set environment variables in the CF Worker

### 8. End-to-End Test

Play a game → submit tape → receive Groth16 proof → submit to contract → verify tokens minted.

---

## Open Questions

### 1. Journal Byte Offsets

RISC Zero `env::commit()` serialization format needs empirical verification. Run a test proof, hex-dump `receipt.journal.bytes`, confirm the byte layout matches the assumed LE u32 packing.

### 2. `risc0-interface` Crate Availability

The Nethermind `risc0-interface` crate may not be on crates.io. Options:
- Git dependency: `risc0-interface = { git = "https://github.com/NethermindEth/stellar-risc0-verifier" }`
- Copy the interface types locally (only need the `verify` trait + types)

### 3. Receipt → Seal Byte Extraction

The API server returns a `risc0_zkvm::Receipt` serialized as JSON. Need to confirm how to extract the raw 260-byte Groth16 seal from the JSON structure. Options:
- Add a `/soroban` endpoint to the API server that returns pre-extracted components
- Parse the JSON receipt client-side to extract `receipt.inner.groth16().seal`

### 4. Workflow vs Queue

Cloudflare Workflows are recommended over raw Queues for this use case because:
- Proving can take minutes to hours (Groth16)
- Workflows support `step.sleep()` and retry with backoff natively
- State persists across steps automatically
- But: Workflows are still in beta. Alternative: Queue + Durable Object for polling state.

### 5. Testnet Verifier Readiness

Verify the testnet router and Groth16 verifier are operational by submitting a test proof before building the full contract.

---

## File Structure

```
contracts/
  asteroids-score/
    Cargo.toml
    src/
      lib.rs          # Score contract (verify proof + mint)
      test.rs         # Tests with mock verifier

workers/
  prove-worker/
    src/
      index.ts        # CF Worker API (POST /prove, GET /prove/{id}, POST /api/send)
      workflow.ts      # ProveWorkflow class
    wrangler.jsonc

frontend/
  src/
    wallet.ts         # SmartAccountKit integration
    prove.ts          # Tape submission + proof polling
    submit.ts         # Build + sign + relay Soroban transaction
```

---

## References

- [Nethermind stellar-risc0-verifier](https://github.com/NethermindEth/stellar-risc0-verifier)
- [OpenZeppelin Stellar Contracts — Smart Account](https://docs.openzeppelin.com/stellar-contracts/accounts/smart-account)
- [Stellar Smart Wallets Guide](https://developers.stellar.org/docs/build/guides/contract-accounts/smart-wallets)
- [smart-account-kit](https://github.com/kalepail/smart-account-kit)
- [OZ Relayer Fee Abstraction](https://docs.openzeppelin.com/stellar-contracts/fee-abstraction)
- [Signing Soroban Invocations](https://developers.stellar.org/docs/build/guides/transactions/signing-soroban-invocations)
- [Cloudflare Workflows](https://developers.cloudflare.com/workflows/)
- [Soroban Authorization](https://developers.stellar.org/docs/learn/fundamentals/contract-development/authorization)
- [Fee-Bump Bug Context](https://stellar.org/blog/developers/fee-bump-bug-disclosure)
