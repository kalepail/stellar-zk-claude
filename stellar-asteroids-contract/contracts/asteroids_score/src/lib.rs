#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Bytes,
    BytesN, Env,
};

mod risc0_router {
    soroban_sdk::contractimport!(file = "risc0_router.wasm");
}

#[contracttype]
enum DataKey {
    Admin,
    RouterId,
    ImageId,
    TokenId,
    Claimed(BytesN<32>),
}

const RULES_DIGEST_V1: u32 = 0x4153_5431; // "AST1"

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ScoreError {
    InvalidJournalLength = 1,
    InvalidRulesDigest = 2,
    JournalAlreadyClaimed = 3,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreSubmitted {
    pub player: Address,
    pub score: u32,
    pub journal_digest: BytesN<32>,
}

#[contract]
pub struct AsteroidsScoreContract;

#[contractimpl]
impl AsteroidsScoreContract {
    pub fn __constructor(
        env: Env,
        admin: Address,
        router_id: Address,
        image_id: BytesN<32>,
        token_id: Address,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::RouterId, &router_id);
        env.storage().instance().set(&DataKey::ImageId, &image_id);
        env.storage().instance().set(&DataKey::TokenId, &token_id);
    }

    /// Verify a RISC Zero proof and mint score tokens to the player.
    ///
    /// - `player`: address receiving tokens (must authorize)
    /// - `seal`: variable-length proof seal bytes
    /// - `journal_raw`: 24-byte raw journal (6 × u32 LE)
    ///
    /// Returns the final score.
    pub fn submit_score(
        env: Env,
        player: Address,
        seal: Bytes,
        journal_raw: Bytes,
    ) -> Result<u32, ScoreError> {
        player.require_auth();

        // Journal must be exactly 24 bytes (6 × u32 LE)
        if journal_raw.len() != 24 {
            return Err(ScoreError::InvalidJournalLength);
        }

        // Decode rules_digest from bytes 20..24 and validate
        let rules_digest = read_u32_le(&journal_raw, 20);
        if rules_digest != RULES_DIGEST_V1 {
            return Err(ScoreError::InvalidRulesDigest);
        }

        // Compute journal digest (SHA-256 of raw journal bytes)
        let journal_digest: BytesN<32> = env.crypto().sha256(&journal_raw).into();

        // Replay protection: reject duplicate journal digests
        let claimed_key = DataKey::Claimed(journal_digest.clone());
        if env.storage().persistent().has(&claimed_key) {
            return Err(ScoreError::JournalAlreadyClaimed);
        }

        // Load config
        let router_id: Address = env.storage().instance().get(&DataKey::RouterId).unwrap();
        let image_id: BytesN<32> = env.storage().instance().get(&DataKey::ImageId).unwrap();
        let token_id: Address = env.storage().instance().get(&DataKey::TokenId).unwrap();

        // Cross-contract call to RISC Zero router to verify the proof
        let router_client = risc0_router::Client::new(&env, &router_id);
        router_client.verify(&seal, &image_id, &journal_digest);

        // Decode final_score from bytes 8..12
        let final_score = read_u32_le(&journal_raw, 8);

        // Mark journal as claimed
        env.storage().persistent().set(&claimed_key, &());

        // Mint score tokens to the player
        let token_client = token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&player, &(final_score as i128));

        // Emit event
        ScoreSubmitted {
            player,
            score: final_score,
            journal_digest,
        }
        .publish(&env);

        Ok(final_score)
    }

    /// Check whether a journal digest has already been claimed.
    pub fn is_claimed(env: Env, journal_digest: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Claimed(journal_digest))
    }

    /// Admin: update the image ID (for program upgrades).
    pub fn set_image_id(env: Env, new_image_id: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ImageId, &new_image_id);
    }

    /// Admin: transfer admin role.
    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Read the current image ID.
    pub fn image_id(env: Env) -> BytesN<32> {
        env.storage().instance().get(&DataKey::ImageId).unwrap()
    }

    /// Read the router address.
    pub fn router_id(env: Env) -> Address {
        env.storage().instance().get(&DataKey::RouterId).unwrap()
    }

    /// Read the token address.
    pub fn token_id(env: Env) -> Address {
        env.storage().instance().get(&DataKey::TokenId).unwrap()
    }
}

/// Read a u32 from bytes at the given offset in little-endian order.
fn read_u32_le(bytes: &Bytes, offset: u32) -> u32 {
    let b0 = bytes.get(offset).unwrap() as u32;
    let b1 = bytes.get(offset + 1).unwrap() as u32;
    let b2 = bytes.get(offset + 2).unwrap() as u32;
    let b3 = bytes.get(offset + 3).unwrap() as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

mod test;
