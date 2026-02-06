# Client Integration Spec

## Goal

Define the client-side path that:
1. Connects a passkey wallet via passkey-kit.
2. Submits a completed game tape for ZK proving.
3. Claims the proven score on-chain via the Soroban contract.
4. Displays token balance and submission history.

## Current State

The game engine, tape capture, proof gateway, RISC0 prover, and Soroban
contract are all implemented. The client currently stops at "proof succeeded"
with no wallet, no on-chain submission, and no token display.

## User Flow

1. User opens app, creates or connects a passkey wallet.
2. User plays Asteroids; tape records every frame.
3. Game over → user submits tape to worker for proving.
4. UI shows proof pipeline status (queued → proving → done).
5. Proof succeeds → user claims score on-chain.
6. Contract verifies proof, mints SAC tokens to player.
7. UI shows updated balance and submission history.

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
client.submit_score({ player, seal, journal_raw }) → AssembledTransaction<u32>
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
  networkPassphrase: import.meta.env.VITE_STELLAR_NETWORK_PASSPHRASE,
  rpcUrl: import.meta.env.VITE_STELLAR_RPC_URL,
});
```

Read-only calls (like `is_claimed`) return results directly from simulation
without signing or sending. Mutating calls (like `submit_score`) return an
`AssembledTransaction` that must be signed and sent.

---

## Passkey Wallet

### Package

```
passkey-kit@0.12.0
```

`passkey-kit` provides three exports:
- `PasskeyKit` — client-side wallet creation, connection, and auth entry signing
- `PasskeyServer` — server-side relay submission and keyId→contractId lookup
- `SACClient` — Stellar Asset Contract helper for token queries

Note: passkey-kit docs reference "smart-account-kit" as a future successor, but
`passkey-kit` at 0.12.0 is what is currently published and in use.

### Client-Side Setup

```typescript
import { PasskeyKit } from 'passkey-kit';

const account = new PasskeyKit({
  rpcUrl: import.meta.env.VITE_STELLAR_RPC_URL,
  networkPassphrase: import.meta.env.VITE_STELLAR_NETWORK_PASSPHRASE,
  walletWasmHash: import.meta.env.VITE_WALLET_WASM_HASH,
});
```

### Wallet Creation (One-Time)

```typescript
const { keyId_base64, contractId, built } = await account.createWallet(
  'ZK Asteroids',  // relying party / app name
  'player'         // user display name
);
// `built` is a signed deploy transaction XDR
// Submit via Launchtube (user has no XLM yet)
// Store keyId_base64 in localStorage for reconnection
```

### Wallet Reconnection

```typescript
const { keyId_base64, contractId } = await account.connectWallet({
  getContractId: async (keyId) => {
    // Look up contractId from keyId via Mercury indexer or own DB
    return await server.getContractId({ keyId });
  },
});
```

### Signing Flow

`account.sign()` bridges the generated contract client and WebAuthn:

1. Takes the `built` Transaction XDR from an `AssembledTransaction`
2. Finds all `SorobanAuthorizationEntry` entries that need signing
3. Triggers the browser's WebAuthn API for each entry's preimage
4. Returns the transaction with signed auth entries injected

The smart wallet's `__check_auth` function validates the WebAuthn signature
on-chain during execution.

---

## Transaction Relay (Launchtube)

### What It Is

Launchtube is the SDF-maintained relay that accepts Soroban transactions and
submits them on-chain with **fee sponsorship**. Users do not need XLM.

### Endpoints

- Testnet: `https://testnet.launchtube.xyz`
- Mainnet: `https://launchtube.xyz`

### Authentication

Bearer JWT token. Testnet tokens available at `https://testnet.launchtube.xyz/gen`.

### Server-Side Integration

`PasskeyServer` wraps the Launchtube HTTP API:

```typescript
import { PasskeyServer } from 'passkey-kit';

const server = new PasskeyServer({
  rpcUrl: process.env.STELLAR_RPC_URL,
  launchtubeUrl: process.env.LAUNCHTUBE_URL,
  launchtubeJwt: process.env.LAUNCHTUBE_JWT,
  mercuryUrl: process.env.MERCURY_URL,    // for keyId → contractId lookup
  mercuryJwt: process.env.MERCURY_JWT,
});

// Submit a signed transaction
const result = await server.send(signedTxXdr);
```

Note: Launchtube is marked as legacy; the successor is the
[OpenZeppelin Relayer](https://docs.openzeppelin.com/relayer/stellar) (self-hosted).
Launchtube is still operational on both testnet and mainnet.

### Where Relay Lives

The relay endpoint requires a JWT secret, so it must run server-side. Options:
- A route on the existing Cloudflare Worker (`/api/send`)
- A separate serverless function
- A Cloudflare Worker Pages Function

The browser never calls Launchtube directly.

---

## Claim Flow (End-to-End)

### Step-by-Step

```
1. Fetch proof result from worker
   GET /api/proofs/jobs/{id}/result
   → { stored_at, prover_response: { result: { proof: { receipt, journal, ... } } } }

2. Extract seal + journal_raw from proof artifact
   seal = prover_response.result.proof.receipt  (exact path TBD — see open questions)
   journal_raw = pack 6 × u32 LE from journal fields

3. Check replay: await scoreContract.is_claimed({ journal_digest })
   If claimed → show "already claimed" message, stop

4. Build contract call
   const at = await scoreContract.submit_score({
     player: account.wallet,     // smart wallet contract address
     seal: Buffer.from(sealBytes),
     journal_raw: Buffer.from(journalBytes),
   });

5. Sign auth entries with passkey
   const signedTx = await account.sign(at.built!, { keyId });

6. Submit via relay
   const result = await sendToServer(signedTx);

7. Return tx hash + minted score
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

The token is a SAC. Use `SACClient` from passkey-kit or query via Stellar SDK:

```typescript
import { SACClient } from 'passkey-kit';

const sac = new SACClient({
  rpcUrl: import.meta.env.VITE_STELLAR_RPC_URL,
  networkPassphrase: import.meta.env.VITE_STELLAR_NETWORK_PASSPHRASE,
});

const tokenClient = sac.getSACClient(import.meta.env.VITE_TOKEN_CONTRACT_ID);
const balance = await tokenClient.balance({ id: walletContractId });
```

### History

Query `ScoreSubmitted` contract events via Soroban RPC `getEvents` or
Horizon transaction history. Each entry provides: score, player, journal digest,
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
- "Claim Score" button (disabled without wallet)
- Progress states: building → signing (WebAuthn prompt) → submitting → confirmed
- Tx hash with explorer link on success
- Error states: already claimed, verification failed, relay error

### History Panel
- Past submissions, most recent first
- Per entry: score, date, tx link
- Empty state when no history

### Game Panel Gating
- Game playable without wallet (tape capture still works)
- "Connect wallet to claim scores" prompt on game-over without wallet
- Proof submission to worker does not require wallet (only claiming does)

---

## State Management

React context + hooks (no external lib):
- `WalletProvider` — PasskeyKit instance, connection state, keyId, contractId
- `ProofProvider` — active proof job, polling, result cache
- `ChainProvider` — token balance, submission history

Each exposes a custom hook (`useWallet`, `useProof`, `useChain`).

---

## Configuration

### Vite Env Vars

```
VITE_STELLAR_RPC_URL          # Soroban RPC endpoint
VITE_STELLAR_NETWORK_PASSPHRASE  # "Test SDF Network ; September 2015"
VITE_SCORE_CONTRACT_ID        # Asteroids score contract address
VITE_TOKEN_CONTRACT_ID        # SAC token contract address
VITE_WALLET_WASM_HASH         # Smart wallet WASM hash (for passkey-kit)
VITE_EXPLORER_URL             # Stellar Expert or StellarChain base URL
```

### Server-Side Env Vars (Worker)

```
STELLAR_RPC_URL               # Soroban RPC
LAUNCHTUBE_URL                # https://testnet.launchtube.xyz
LAUNCHTUBE_JWT                # Launchtube auth token
MERCURY_URL                   # Mercury indexer (keyId → contractId)
MERCURY_JWT                   # Mercury auth token
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

# Passkey wallet creation, signing, relay, SAC queries
passkey-kit

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
    passkey.ts               # PasskeyKit instance + create/connect/sign
    relay.ts                 # Server call to submit signed TX via Launchtube
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

2. **Mercury indexer** — passkey-kit uses Mercury for keyId → contractId
   reverse lookup during `connectWallet()`. Do we need a Mercury account, or
   can we store the mapping ourselves (localStorage + optional backend)?

3. **Contract deployment** — Testnet addresses for:
   - Asteroids score contract
   - SAC token contract
   - RISC Zero router contract
   - Smart wallet WASM hash

4. **Relay hosting** — The Launchtube JWT must stay server-side. Add a
   `/api/send` route to the existing Cloudflare Worker, or deploy separately?

---

## Implementation Order

### Phase 1: Contract Bindings + Config
- Build contract WASM, generate TypeScript bindings
- Add `@stellar/stellar-sdk` and `vite-plugin-node-polyfills`
- Create `config.ts` and `.env.example`
- Verify: `is_claimed()` and read-only getters work against testnet

### Phase 2: Passkey Wallet
- Add `passkey-kit` dependency
- Build `wallet/passkey.ts` (PasskeyKit instance, create, connect)
- Build `WalletProvider` context
- Add `WalletHeader` component
- Gate: can create wallet, reconnect, see contract address

### Phase 3: Relay + Claim Flow
- Add relay route to worker (or separate function)
- Build `wallet/relay.ts` and `proof/claim.ts`
- Build `contract/journal.ts` (pack + digest)
- Build `ClaimPanel` component
- Gate: full flow from proof result → signed tx → on-chain mint → tx hash

### Phase 4: Balance & History
- Build `chain/balance.ts` using SACClient
- Build `chain/history.ts` using Soroban events
- Build `ChainProvider` context + `HistoryPanel` component
- Gate: balance updates after claim, browsable history

### Phase 5: Polish
- Error handling for all failure modes
- Loading/signing states (WebAuthn prompt indicator)
- Mobile responsive adjustments
- localStorage cleanup on disconnect
