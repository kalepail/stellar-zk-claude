# Asteroids Score Token Contract Spec

## Goal
Define the Soroban contract path that:
1. Accepts a valid proof payload for a replayed Asteroids tape.
2. Verifies it through the RISC Zero verifier route on Stellar.
3. Extracts the proven score from committed journal bytes.
4. Mints score-denominated tokens to the player.

## On-Chain Components
- RISC Zero Router contract (verification dispatch)
- Groth16 verifier contract (proof check)
- Asteroids score contract (application logic)
- Score token contract (SAC or equivalent)

## Contract Storage
- `Admin`
- `TokenId`
- `RouterId`
- `ImageId` (expected proving program identity)
- `Claimed(journal_digest)` replay-protection map

## Core Functions
### `initialize(admin, token, router, image_id)`
One-time setup for routing and trust anchors.

### `submit_score(player, seal, image_id, journal_raw) -> u32`
Required checks in order:
1. `player.require_auth()`
2. `image_id` matches stored expected image ID
3. `journal_digest = sha256(journal_raw)`
4. `journal_digest` not already claimed
5. call router `verify(seal, image_id, journal_digest)`
6. decode `final_score` from journal bytes
7. mark digest as claimed
8. mint score tokens to `player`
9. emit submission event

### `is_claimed(journal_digest) -> bool`
Replay-check utility.

### `set_image_id(new_image_id)`
Admin-only version rotation hook.

## Proof Payload Requirements
- Receipt kind must be on-chain-verifiable (Groth16 path).
- Seal format must match verifier expectations.
- Journal digest supplied to verifier must be hash of raw journal bytes.

## Replay and Fraud Controls
- Journal digest replay lock prevents duplicate claims.
- Image ID pinning prevents proofs from unauthorized programs.
- Player auth prevents unsigned third-party claims.

## Journal Contract
Journal fields are expected to include at minimum:
- seed
- frame_count
- final_score
- final_rng_state
- tape_checksum
- rules_digest/version marker

Decode offsets must be confirmed against actual serialized journal bytes from the
active prover/toolchain version.

## Deployment Checklist
1. Deploy/identify score token contract.
2. Deploy Asteroids score contract.
3. Initialize contract with router + image ID + token.
4. Transfer token admin control to score contract (if required by token model).
5. Run end-to-end proof submission test and confirm mint behavior.

## Testnet References (Current Research Set)
Canonical testnet verifier/router references are tracked in
`docs/zk/09-GETTING-STARTED.md` to avoid drift across docs. Re-validate those
addresses before production rollout.
