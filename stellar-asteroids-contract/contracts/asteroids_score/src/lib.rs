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
    Best(Address, u32),
}

const RULES_DIGEST: u32 = 0x4153_5433; // "AST3"
const JOURNAL_BASE_LEN: u32 = 24; // 6 x u32 (seed..rules_digest)
const CLAIMANT_LEN_OFFSET: u32 = JOURNAL_BASE_LEN;
const CLAIMANT_BYTES_OFFSET: u32 = JOURNAL_BASE_LEN + 4;
const MAX_CLAIMANT_ADDR_LEN: u32 = 128;
const INSTANCE_TTL_THRESHOLD: u32 = 120_960; // 14 days
const INSTANCE_TTL_BUMP: u32 = 172_800; // 20 days

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ScoreError {
    InvalidJournalLength = 1,
    InvalidRulesDigest = 2,
    JournalAlreadyClaimed = 3,
    ZeroScoreNotAllowed = 4,
    InvalidClaimantAddressLength = 5,
    ScoreNotImproved = 6,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreSubmitted {
    pub claimant: Address,
    pub seed: u32,
    pub previous_best: u32,
    pub new_best: u32,
    pub minted_delta: u32,
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
        extend_instance_ttl(&env);
    }

    /// Verify a RISC Zero proof and mint score tokens to the claimant address
    /// embedded in the journal.
    ///
    /// - `seal`: variable-length proof seal bytes
    /// - `journal_raw`: raw journal bytes:
    ///   - 24-byte base (6 × u32 LE)
    ///   - u32 claimant_address_len (LE)
    ///   - claimant_address bytes (Stellar strkey, ASCII)
    ///
    /// Returns the claimant's new best score for this seed.
    pub fn submit_score(env: Env, seal: Bytes, journal_raw: Bytes) -> Result<u32, ScoreError> {
        extend_instance_ttl(&env);

        // Journal must contain base fields and claimant length.
        if journal_raw.len() < CLAIMANT_BYTES_OFFSET {
            return Err(ScoreError::InvalidJournalLength);
        }

        let claimant_len = read_u32_le(&journal_raw, CLAIMANT_LEN_OFFSET);
        if claimant_len == 0 || claimant_len > MAX_CLAIMANT_ADDR_LEN {
            return Err(ScoreError::InvalidClaimantAddressLength);
        }
        if journal_raw.len() != CLAIMANT_BYTES_OFFSET + claimant_len {
            return Err(ScoreError::InvalidJournalLength);
        }

        let claimant_bytes =
            journal_raw.slice(CLAIMANT_BYTES_OFFSET..(CLAIMANT_BYTES_OFFSET + claimant_len));
        let claimant = Address::from_string_bytes(&claimant_bytes);

        // Decode seed and score.
        let seed = read_u32_le(&journal_raw, 0);
        let final_score = read_u32_le(&journal_raw, 8);

        // Decode rules_digest from bytes 20..24 and validate
        let rules_digest = read_u32_le(&journal_raw, 20);
        if rules_digest != RULES_DIGEST {
            return Err(ScoreError::InvalidRulesDigest);
        }

        // Enforce non-zero minting.
        if final_score == 0 {
            return Err(ScoreError::ZeroScoreNotAllowed);
        }

        // Compute journal digest (SHA-256 of raw journal bytes)
        let journal_digest: BytesN<32> = env.crypto().sha256(&journal_raw).into();

        // Replay protection: reject duplicate journal digests
        let claimed_key = DataKey::Claimed(journal_digest.clone());
        if env.storage().persistent().has(&claimed_key) {
            return Err(ScoreError::JournalAlreadyClaimed);
        }

        // Per-claimant per-seed best score policy.
        let best_key = DataKey::Best(claimant.clone(), seed);
        let previous_best = env.storage().persistent().get(&best_key).unwrap_or(0u32);
        if final_score <= previous_best {
            return Err(ScoreError::ScoreNotImproved);
        }
        let minted_delta = final_score - previous_best;

        // Load config
        let router_id: Address = env.storage().instance().get(&DataKey::RouterId).unwrap();
        let image_id: BytesN<32> = env.storage().instance().get(&DataKey::ImageId).unwrap();
        let token_id: Address = env.storage().instance().get(&DataKey::TokenId).unwrap();

        // Cross-contract call to RISC Zero router to verify the proof
        let router_client = risc0_router::Client::new(&env, &router_id);
        router_client.verify(&seal, &image_id, &journal_digest);

        // Mark journal as claimed
        env.storage().persistent().set(&claimed_key, &());
        env.storage().persistent().set(&best_key, &final_score);

        // Mint only the improvement delta to the claimant.
        let token_client = token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&claimant, &(minted_delta as i128));

        // Emit event
        ScoreSubmitted {
            claimant,
            seed,
            previous_best,
            new_best: final_score,
            minted_delta,
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

    /// Read a claimant's best score for a seed.
    pub fn best_score(env: Env, claimant: Address, seed: u32) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::Best(claimant, seed))
            .unwrap_or(0u32)
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

    /// Read the expected rules digest.
    pub fn rules_digest(_env: Env) -> u32 {
        RULES_DIGEST
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

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

mod test;
