# Asteroids Score Token Contract

## Goal

Soroban contract that:
1. Accepts a RISC Zero proof payload for a replayed Asteroids tape.
2. Verifies it through the RISC Zero router contract on Stellar.
3. Extracts the proven score from committed journal bytes.
4. Mints score-denominated tokens to the claimant.

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
    Best(Address, u32),       // u32 — best score per claimant per seed
}
```

Admin, RouterId, ImageId, and TokenId are stored in **instance** storage.
Claimed entries are stored in **persistent** storage.

## Error Codes

```rust
enum ScoreError {
    InvalidJournalLength = 1, // journal_raw is not exactly 24 bytes
    InvalidRulesDigest = 2,   // rules_digest ≠ 0x4153_5433 ("AST3")
    JournalAlreadyClaimed = 3,// journal digest previously claimed
    ZeroScoreNotAllowed = 4,  // final_score == 0
    ScoreNotImproved = 5,     // final_score <= previous_best for claimant+seed
}
```

## Constructor

```rust
fn __constructor(env, admin: Address, router_id: Address, image_id: BytesN<32>, token_id: Address)
```

One-time setup. Stores all four config values in instance storage.

## Core Function

### `submit_score(seal: Bytes, journal_raw: Bytes, claimant: Address) -> Result<u32, ScoreError>`

Validation and execution in order:

1. `journal_raw.len() != 24` → `InvalidJournalLength`
2. Decode `rules_digest` from bytes `[20..24]` LE; must equal `0x4153_5433` → `InvalidRulesDigest`
3. Decode `seed` from `[0..4]`, `final_score` from `[8..12]`; `final_score` must be `> 0` → `ZeroScoreNotAllowed`
4. `journal_digest = sha256(journal_raw)`
5. Check `Claimed(journal_digest)` not in persistent storage → `JournalAlreadyClaimed`
6. Load `previous_best = Best(claimant, seed)` default `0`; require `final_score > previous_best` → `ScoreNotImproved`
7. Compute `minted_delta = final_score - previous_best`
8. Load `router_id`, `image_id`, `token_id` from instance storage
9. Cross-contract call: `router.verify(seal, image_id, journal_digest)`
10. Store `Claimed(journal_digest)` and `Best(claimant, seed) = final_score`
11. Mint `minted_delta` tokens to `claimant` via `StellarAssetClient`
12. Emit `ScoreSubmitted { claimant, seed, previous_best, new_best: final_score, minted_delta, journal_digest }`
13. Return `Ok(final_score)`

Note: `image_id` is **not** a parameter — it is read from storage. The contract
trusts its own stored image ID rather than accepting one from the caller.

## Read-Only Functions

- `is_claimed(journal_digest: BytesN<32>) -> bool` — replay check
- `best_score(claimant: Address, seed: u32) -> u32` — claimant’s best score for seed
- `image_id() -> BytesN<32>` — current verifier image ID
- `router_id() -> Address` — router contract address
- `token_id() -> Address` — token contract address
- `rules_digest() -> u32` — expected journal rules digest (`0x4153_5433`)

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
| 20..24 | `rules_digest` | Must be `0x4153_5433` ("AST3") |

## Event

```rust
struct ScoreSubmitted {
    claimant: Address,
    seed: u32,
    previous_best: u32,
    new_best: u32,
    minted_delta: u32,
    journal_digest: BytesN<32>,
}
```

## Replay and Fraud Controls

- **Journal digest replay lock** — each unique journal can only be claimed once.
- **Image ID pinning** — only proofs from the expected program are accepted.
- **Per-claimant improvement policy** — only strictly higher score for `(claimant, seed)` is accepted.
- **Rules digest check** — rejects journals from incompatible game versions.

## Token Model

The token is a **Stellar Asset Contract** (SAC) wrapping a classic Stellar
asset. The contract calls `StellarAssetClient::mint()` to mint only the
improvement delta (`new_best - previous_best`) for a claimant+seed. The SAC
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
