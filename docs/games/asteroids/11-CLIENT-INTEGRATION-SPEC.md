# Client Integration Spec

## Goal

Define the client-side path that:
1. Connects a smart-account wallet via smart-account-kit.
2. Submits a completed game tape for ZK proving.
3. Relays the proven score on-chain (through the worker claim relay).
4. Displays token balance and submission history.

## Current State

The game engine, tape capture, proof gateway, RISC0 prover, and Soroban
contract are implemented. The current frontend uses wallet-backed proof
submission and surfaces claim relay status from the worker.

## User Flow

1. User opens app, creates or connects a smart-account wallet.
2. User plays Asteroids; tape records every frame.
3. Game over → user submits tape to worker for proving with `x-claimant-address`.
4. UI shows proof pipeline status (queued → proving → done).
5. Proof succeeds → worker enqueues claim relay job automatically.
6. Claim relay submits `submit_score(seal, journal_raw, claimant)` on-chain.
7. UI shows claim status (`queued/submitting/retrying/succeeded/failed`) and tx hash when available.

---

## Contract Bindings

### Generation

Use the Stellar CLI to generate typed TypeScript bindings from the compiled
WASM. No deployed contract or network access is required.

```bash
stellar contract bindings typescript \
  --wasm ./stellar-asteroids-contract/target/wasm32v1-none/release/asteroids_score.wasm \
  --output-dir ./packages/asteroids-score-client \
  --overwrite

cd ./packages/asteroids-score-client && npm install && npm run build && cd ../..
```

Add as a local dependency:
```bash
npm add file:./packages/asteroids-score-client
```

### What Gets Generated

```
packages/asteroids-score-client/
  src/
    index.ts     # Barrel re-exports
    client.ts    # Typed Client class extending @stellar/stellar-sdk contract.Client
    types.ts     # TypeScript types for ScoreError, DataKey, etc.
  package.json   # Peer-depends on @stellar/stellar-sdk
```

The generated `Client` has typed methods matching each contract function:

```typescript
client.submit_score({ seal, journal_raw, claimant }) → AssembledTransaction<u32>
client.is_claimed({ journal_digest })              → AssembledTransaction<boolean>
client.image_id()                                  → AssembledTransaction<Buffer>
client.router_id()                                 → AssembledTransaction<string>
client.token_id()                                  → AssembledTransaction<string>
```

### Client Instantiation

```typescript
import { Client } from 'asteroids-score-client';

const scoreContract = new Client({
  contractId: import.meta.env.VITE_SCORE_CONTRACT_ID,
  networkPassphrase: import.meta.env.VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE,
  rpcUrl: import.meta.env.VITE_SMART_ACCOUNT_RPC_URL,
});
```

Read-only calls (like `is_claimed`) return results directly from simulation
without signing or sending. Mutating calls (like `submit_score`) return an
`AssembledTransaction` that must be signed and sent.

---

## Smart Account Wallet

### Package

```
smart-account-kit
```

`smart-account-kit` is the wallet SDK used by the app for:
- passkey-backed smart wallet creation
- reconnecting existing wallet sessions
- signing and submitting Soroban transactions

### Client-Side Setup

```typescript
import { SmartAccountKit, IndexedDBStorage } from 'smart-account-kit';

const kit = new SmartAccountKit({
  rpcUrl: import.meta.env.VITE_SMART_ACCOUNT_RPC_URL,
  networkPassphrase: import.meta.env.VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE,
  accountWasmHash: import.meta.env.VITE_SMART_ACCOUNT_WASM_HASH,
  webauthnVerifierAddress: import.meta.env.VITE_SMART_ACCOUNT_WEBAUTHN_VERIFIER_ADDRESS,
  relayerUrl: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_URL,
  storage: new IndexedDBStorage(),
  rpName: import.meta.env.VITE_SMART_ACCOUNT_RP_NAME ?? 'Stellar ZK',
});
```

### Wallet Creation (One-Time)

```typescript
const creation = await kit.createWallet('Stellar ZK Asteroids', 'player', {
  autoSubmit: false,
});

// Submit signed deployment XDR through relayer
await submitDeploymentXdr(creation.signedTransaction);

// Bind the newly deployed wallet as the active session
const session = await kit.connectWallet({
  contractId: creation.contractId,
  credentialId: creation.credentialId,
});

if (!session) {
  throw new Error('wallet deployed, but failed to restore connected session');
}
```

### Wallet Reconnection

```typescript
// Silent restore from local persisted session
const restored = await kit.connectWallet();

// Prompt user with passkey chooser when needed
const prompted = await kit.connectWallet({ prompt: true });
```

### Signing Flow

`smart-account-kit` supports both manual and one-shot submission flows:

```typescript
// Manual: sign auth entries on an assembled transaction
const signed = await kit.sign(assembledTx);

// Recommended: sign + re-simulate + submit
const txResult = await kit.signAndSubmit(assembledTx);
```

The smart wallet's `__check_auth` function validates WebAuthn signatures on-chain.

---

## Transaction Relay (OpenZeppelin Channels)

### What It Is

OpenZeppelin Channels is the relayer used for sponsored Soroban transaction
submission.

### Endpoints

- Testnet: `https://channels.openzeppelin.com/testnet`
- Mainnet: `https://channels.openzeppelin.com`

### Authentication

API key. Generation endpoints:
- Testnet: `https://channels.openzeppelin.com/testnet/gen`
- Mainnet: `https://channels.openzeppelin.com/gen`

### Client-Side Integration

For the managed Channels service, use `baseUrl` + `apiKey`:

```typescript
import { ChannelsClient } from '@openzeppelin/relayer-plugin-channels';

const client = new ChannelsClient({
  baseUrl: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_URL!,
  apiKey: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_API_KEY!,
});

const result = await client.submitTransaction({ xdr: signedTransaction });
```

If using a self-hosted OpenZeppelin Relayer plugin, include `pluginId`:

```typescript
const client = new ChannelsClient({
  baseUrl: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_URL!,
  apiKey: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_API_KEY!,
  pluginId: import.meta.env.VITE_SMART_ACCOUNT_RELAYER_PLUGIN_ID!,
});
```

### Where Relay Logic Lives

Primary path: transaction submission is delegated to the worker claim relay.
The browser submits proof jobs; the worker performs on-chain claim relay after
proof success.

The browser should never receive privileged backend relay secrets.

---

## Claim Flow (End-to-End)

### Step-by-Step

```
1. Submit proof job to worker
   POST /api/proofs/jobs (raw tape body + x-claimant-address)

2. Poll proof job
   GET /api/proofs/jobs/{id}
   → includes proof state + claim state

3. On proof success
   Worker enqueues claim job and sends claimant + journal + proof artifact to claim relay.

4. Track claim relay status in UI
   claim.status: queued | submitting | retrying | succeeded | failed
   claim.txHash: set when available

5. Optional fallback path (manual claim)
   If claim relay fails terminally, UI can surface claim.fallbackPayload and use it
   to submit `submit_score({ seal, journal_raw, claimant })` from an operator tool.
```

### Journal Packing

Pack 24 bytes from the proof journal fields (all u32 LE):

```typescript
function packJournal(journal: ProofJournal): Uint8Array {
  const buf = new ArrayBuffer(24);
  const view = new DataView(buf);
  view.setUint32(0, journal.seed, true);
  view.setUint32(4, journal.frame_count, true);
  view.setUint32(8, journal.final_score, true);
  view.setUint32(12, journal.final_rng_state, true);
  view.setUint32(16, journal.tape_checksum, true);
  view.setUint32(20, journal.rules_digest, true);
  return new Uint8Array(buf);
}
```

---

## Token Balance & History

### Balance

The token is a SAC. Query it via generated token bindings or direct Stellar SDK
contract calls (frontend or worker backend). `smart-account-kit` does not expose
a dedicated SAC balance helper.

```typescript
const balance = await tokenClient.balance({ id: walletContractId });
```

### History

Query `ScoreSubmitted` contract events via Soroban RPC `getEvents` or
Horizon transaction history. Each entry provides: score, claimant, journal digest,
ledger timestamp, and tx hash.

---

## UI Components

### Wallet Header
- Connect / disconnect button
- Truncated contract address when connected
- Network indicator (testnet badge)
- Token balance (refreshed after claim)

### Claim Panel
- Appears after proof succeeds
- Shows relay lifecycle (`queued/submitting/retrying/succeeded/failed`)
- Shows tx hash with explorer link on success
- Shows relay error details + fallback payload action when relay fails

### History Panel
- Past submissions, most recent first
- Per entry: score, date, tx link
- Empty state when no history

### Game Panel Gating
- Game playable without wallet (tape capture still works)
- "Connect wallet to claim scores" prompt on game-over without wallet
- Proof submission to worker requires wallet (claimant address is mandatory)

---

## State Management

React context + hooks (no external lib):
- `WalletProvider` — SmartAccountKit instance, connection state, credentialId, contractId
- `ProofProvider` — active proof job, polling, result cache
- `ChainProvider` — token balance, submission history

Each exposes a custom hook (`useWallet`, `useProof`, `useChain`).

---

## Configuration

### Vite Env Vars

```
VITE_SMART_ACCOUNT_RPC_URL          # Soroban RPC endpoint
VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE  # "Test SDF Network ; September 2015"
VITE_SMART_ACCOUNT_WASM_HASH        # Smart account WASM hash
VITE_SMART_ACCOUNT_WEBAUTHN_VERIFIER_ADDRESS  # WebAuthn verifier contract ID
VITE_SMART_ACCOUNT_RELAYER_URL      # Channels endpoint (testnet/mainnet)
VITE_SMART_ACCOUNT_RELAYER_API_KEY  # API key for managed Channels
VITE_SMART_ACCOUNT_RELAYER_PLUGIN_ID  # Optional, self-hosted OZ Relayer only
VITE_SMART_ACCOUNT_RP_NAME          # Relying party name shown in passkey UX
VITE_SCORE_CONTRACT_ID        # Asteroids score contract address
VITE_TOKEN_CONTRACT_ID        # SAC token contract address
VITE_EXPLORER_URL             # Stellar Expert or StellarChain base URL
```

### Server-Side Env Vars (Worker)

```
RELAYER_URL                   # Channels or proxy endpoint
RELAYER_API_KEY               # Keep as worker secret when proxying tx submission
RELAYER_PLUGIN_ID             # Optional, self-hosted OZ Relayer plugin id
```

### Vite Config

The Stellar SDK uses `Buffer` and other Node APIs. Add polyfills:

```typescript
// vite.config.ts
import { nodePolyfills } from 'vite-plugin-node-polyfills';

export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({ include: ['buffer', 'stream', 'util'] }),
  ],
});
```

---

## Dependencies

```
# Stellar SDK (peer dep of generated bindings)
@stellar/stellar-sdk

# Smart account wallet creation, signing, and session management
smart-account-kit

# Optional direct Channels client integration
@openzeppelin/relayer-plugin-channels

# Generated typed contract client (local)
file:./packages/asteroids-score-client

# Vite polyfills for Stellar SDK
vite-plugin-node-polyfills
```

---

## File Structure

```
packages/
  asteroids-score-client/     # Generated contract bindings (npm run build)
src/
  wallet/
    smartAccount.ts          # SmartAccountKit instance + create/connect/sign
    relay.ts                 # Optional server call to submit signed TX via Channels
  contract/
    score.ts                 # Generated Client instance, configured
    journal.ts               # packJournal(), computeJournalDigest()
  proof/
    api.ts                   # (exists) Worker API client
    claim.ts                 # Proof-to-chain orchestration
  chain/
    balance.ts               # SAC token balance queries
    history.ts               # ScoreSubmitted event queries
  providers/
    WalletProvider.tsx        # Wallet context + hook
    ProofProvider.tsx         # Proof job context + hook
    ChainProvider.tsx         # Balance + history context + hook
  components/
    AsteroidsCanvas.tsx       # (exists)
    WalletHeader.tsx          # Connect button, address, balance
    ClaimPanel.tsx            # Claim flow UI
    HistoryPanel.tsx          # Score submission history
  config.ts                  # Centralized env var access
  App.tsx                    # (modify) Wire providers, add panels
```

---

## Open Questions

1. **Seal extraction** — The worker stores the full prover response in R2 at
   `prover_response.result.proof.receipt` (typed as `unknown` in worker types).
   Need to inspect a real groth16 proof result to determine the exact path to
   the raw seal bytes and confirm the format (hex string, base64, or nested
   object). This is the primary blocker before implementing the claim flow.

2. **Claim submission path** — Should claim transactions submit directly from
   frontend via `kit.signAndSubmit()`, or should frontend only sign and send
   the signed payload to worker for backend submission?

3. **Contract deployment** — Testnet addresses for:
   - Asteroids score contract
   - SAC token contract
   - RISC Zero router contract
   - Smart wallet WASM hash

4. **Relay hosting** — If we proxy Channels through worker, add `/api/send` to
   existing Cloudflare Worker, or deploy a separate relay façade?

---

## Implementation Order

### Phase 1: Contract Bindings + Config
- Build contract WASM, generate TypeScript bindings
- Add `@stellar/stellar-sdk` and `vite-plugin-node-polyfills`
- Create `config.ts` and `.env.example`
- Verify: `is_claimed()` and read-only getters work against testnet

### Phase 2: Smart Account Wallet
- Add `smart-account-kit` dependency
- Build `wallet/smartAccount.ts` (SmartAccountKit instance, create, connect)
- Build `WalletProvider` context
- Add `WalletHeader` component
- Gate: can create wallet, reconnect, see contract address

### Phase 3: Relay + Claim Flow
- Add Channels relay route to worker (or separate function)
- Build `wallet/relay.ts` and `proof/claim.ts`
- Build `contract/journal.ts` (pack + digest)
- Build `ClaimPanel` component
- Gate: full flow from proof result → signed tx → on-chain mint → tx hash

### Phase 4: Balance & History
- Build `chain/balance.ts` using token contract client
- Build `chain/history.ts` using Soroban events
- Build `ChainProvider` context + `HistoryPanel` component
- Gate: balance updates after claim, browsable history

### Phase 5: Polish
- Error handling for all failure modes
- Loading/signing states (WebAuthn prompt indicator)
- Mobile responsive adjustments
- localStorage cleanup on disconnect
