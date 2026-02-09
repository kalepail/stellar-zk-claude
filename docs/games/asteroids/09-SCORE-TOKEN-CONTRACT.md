# Asteroids Score Token Contract

## Goal

Soroban contract that:
1. Accepts a RISC Zero proof payload for a replayed Asteroids tape.
2. Verifies it through the RISC Zero router contract on Stellar.
3. Extracts the proven score from committed journal bytes.
4. Mints score-denominated tokens to the player.

## On-Chain Components

- **RISC Zero Router** contract — verification dispatch
- **Groth16 verifier** contract — proof check (called by router)
- **Asteroids score** contract — application logic (`AsteroidsScoreContract`)
- **Score token** — Stellar Asset Contract (SAC) wrapping a classic asset

## Contract Storage

```rust
enum DataKey {
    Admin,                    // Address — contract admin
    RouterId,                 // Address — RISC0 router contract
    ImageId,                  // BytesN<32> — expected proving program identity
    TokenId,                  // Address — SAC token contract
    Claimed(BytesN<32>),      // () — replay-protection per journal digest
}
```

Admin, RouterId, ImageId, and TokenId are stored in **instance** storage.
Claimed entries are stored in **persistent** storage.

## Error Codes

```rust
enum ScoreError {
    InvalidJournalLength = 1, // journal_raw is not exactly 24 bytes
    InvalidRulesDigest = 2,   // rules_digest ≠ 0x4153_5432 ("AST2")
    JournalAlreadyClaimed = 3,// journal digest previously claimed
}
```

## Constructor

```rust
fn __constructor(env, admin: Address, router_id: Address, image_id: BytesN<32>, token_id: Address)
```

One-time setup. Stores all four config values in instance storage.

## Core Function

### `submit_score(player: Address, seal: Bytes, journal_raw: Bytes) -> Result<u32, ScoreError>`

Validation and execution in order:

1. `player.require_auth()`
2. `journal_raw.len() != 24` → `InvalidJournalLength`
3. Decode `rules_digest` from bytes `[20..24]` LE; must equal `0x4153_5432` → `InvalidRulesDigest`
4. `journal_digest = sha256(journal_raw)`
5. Check `Claimed(journal_digest)` not in persistent storage → `JournalAlreadyClaimed`
6. Load `router_id`, `image_id`, `token_id` from instance storage
7. Cross-contract call: `router.verify(seal, image_id, journal_digest)`
8. Decode `final_score` from bytes `[8..12]` LE
9. Store `Claimed(journal_digest)` in persistent storage
10. Mint `final_score` tokens to `player` via `StellarAssetClient`
11. Emit `ScoreSubmitted { player, score: final_score, journal_digest }`
12. Return `Ok(final_score)`

Note: `image_id` is **not** a parameter — it is read from storage. The contract
trusts its own stored image ID rather than accepting one from the caller.

## Read-Only Functions

- `is_claimed(journal_digest: BytesN<32>) -> bool` — replay check
- `image_id() -> BytesN<32>` — current verifier image ID
- `router_id() -> Address` — router contract address
- `token_id() -> Address` — token contract address

## Admin Functions

- `set_image_id(new_image_id: BytesN<32>)` — requires admin auth; version rotation
- `set_admin(new_admin: Address)` — requires admin auth; transfer admin role

## Journal Layout

24 bytes (6 × u32 LE):

| Offset | Field | Notes |
|--------|-------|-------|
| 0..4 | `seed` | Game RNG seed |
| 4..8 | `frame_count` | Total frames played |
| 8..12 | `final_score` | Score minted as tokens |
| 12..16 | `final_rng_state` | RNG state at game end |
| 16..20 | `tape_checksum` | CRC-32 of tape (unused by contract) |
| 20..24 | `rules_digest` | Must be `0x4153_5432` ("AST2") |

## Event

```rust
struct ScoreSubmitted {
    player: Address,
    score: u32,
    journal_digest: BytesN<32>,
}
```

## Replay and Fraud Controls

- **Journal digest replay lock** — each unique journal can only be claimed once.
- **Image ID pinning** — only proofs from the expected program are accepted.
- **Player auth** — `require_auth()` prevents unsigned third-party claims.
- **Rules digest check** — rejects journals from incompatible game versions.

## Token Model

The token is a **Stellar Asset Contract** (SAC) wrapping a classic Stellar
asset. The contract calls `StellarAssetClient::mint()` to mint tokens. The SAC
admin must be set to the score contract address (or a shared admin that
authorizes minting).

## Deployment Checklist

1. Deploy or identify the SAC token contract.
2. Deploy the Asteroids score contract with `__constructor(admin, router_id, image_id, token_id)`.
3. Set the SAC token admin to the score contract address.
4. Run end-to-end proof submission test and confirm mint behavior.

## Testnet References

Canonical testnet verifier/router references are tracked in
`docs/zk/09-GETTING-STARTED.md` to avoid drift across docs.

## Repository Path

`stellar-asteroids-contract/contracts/asteroids_score/src/lib.rs`
