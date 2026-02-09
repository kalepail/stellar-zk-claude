#![cfg(test)]

use crate::{
    AsteroidsScoreContract, AsteroidsScoreContractArgs, AsteroidsScoreContractClient, ScoreError,
};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, token::TokenClient, Address, Bytes, BytesN,
    Env,
};

// ---------------------------------------------------------------------------
// Mock router: always accepts verify
// ---------------------------------------------------------------------------
mod mock_router_ok {
    use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env};

    #[contract]
    pub struct MockRouter;

    #[contractimpl]
    impl MockRouter {
        pub fn verify(_env: Env, _seal: Bytes, _image_id: BytesN<32>, _journal: BytesN<32>) {
            // Always succeeds
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a valid 24-byte journal: seed=1, frame_count=100, final_score=42,
/// final_rng_state=99, tape_checksum=0xDEAD, rules_digest=0x41535432 ("AST2")
fn make_journal(env: &Env, final_score: u32) -> Bytes {
    let mut buf = [0u8; 24];
    // seed (0..4)
    buf[0..4].copy_from_slice(&1u32.to_le_bytes());
    // frame_count (4..8)
    buf[4..8].copy_from_slice(&100u32.to_le_bytes());
    // final_score (8..12)
    buf[8..12].copy_from_slice(&final_score.to_le_bytes());
    // final_rng_state (12..16)
    buf[12..16].copy_from_slice(&99u32.to_le_bytes());
    // tape_checksum (16..20)
    buf[16..20].copy_from_slice(&0xDEADu32.to_le_bytes());
    // rules_digest (20..24)
    buf[20..24].copy_from_slice(&0x4153_5432u32.to_le_bytes());
    Bytes::from_slice(env, &buf)
}

/// Build a journal with a wrong rules_digest.
fn make_journal_bad_rules(env: &Env) -> Bytes {
    let mut buf = [0u8; 24];
    buf[0..4].copy_from_slice(&1u32.to_le_bytes());
    buf[4..8].copy_from_slice(&100u32.to_le_bytes());
    buf[8..12].copy_from_slice(&42u32.to_le_bytes());
    buf[12..16].copy_from_slice(&99u32.to_le_bytes());
    buf[16..20].copy_from_slice(&0xDEADu32.to_le_bytes());
    buf[20..24].copy_from_slice(&0xBAAD_F00Du32.to_le_bytes()); // wrong
    Bytes::from_slice(env, &buf)
}

fn dummy_image_id(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xAA; 32])
}

fn dummy_seal(env: &Env) -> Bytes {
    Bytes::from_slice(env, &[0u8; 64])
}

/// Set up the environment with the score contract, mock router, and SAC token.
/// Returns (client, player, admin, token_address).
fn setup(env: &Env) -> (AsteroidsScoreContractClient<'_>, Address, Address, Address) {
    let admin = Address::generate(env);
    let player = Address::generate(env);

    // Register SAC token with admin as issuer
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();

    // Register mock router
    let router_addr = env.register(mock_router_ok::MockRouter, ());

    let image_id = dummy_image_id(env);

    // Register score contract with constructor
    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );

    // Transfer SAC mint authority to the score contract so it can mint
    let sac_admin = StellarAssetClient::new(env, &token_addr);
    sac_admin.set_admin(&contract_id);

    let client = AsteroidsScoreContractClient::new(env, &contract_id);
    (client, player, admin, token_addr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _player, _admin, token_addr) = setup(&env);

    assert_eq!(client.image_id(), dummy_image_id(&env));
    assert_eq!(client.token_id(), token_addr);
    assert_eq!(client.rules_digest(), 0x4153_5432);
    // router_id should be set (just check it doesn't panic)
    let _ = client.router_id();
}

#[test]
fn test_submit_score_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, token_addr) = setup(&env);
    let journal = make_journal(&env, 42);
    let seal = dummy_seal(&env);

    let score = client.submit_score(&player, &seal, &journal);
    assert_eq!(score, 42);

    // Check token balance
    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&player), 42);
}

#[test]
fn test_submit_score_duplicate_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, _token_addr) = setup(&env);
    let journal = make_journal(&env, 42);
    let seal = dummy_seal(&env);

    // First submission succeeds
    client.submit_score(&player, &seal, &journal);

    // Second submission with same journal should fail with JournalAlreadyClaimed
    let result = client.try_submit_score(&player, &seal, &journal);
    assert_eq!(result, Err(Ok(ScoreError::JournalAlreadyClaimed)));
}

#[test]
fn test_submit_score_requires_player_auth() {
    let env = Env::default();
    // Deliberately NOT calling env.mock_all_auths()

    let admin = Address::generate(&env);
    let player = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = dummy_image_id(&env);

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );

    // We need to mock auth for the SAC admin transfer but NOT for the player
    // So let's just skip the admin setup and test the auth failure directly
    let client = AsteroidsScoreContractClient::new(&env, &contract_id);
    let journal = make_journal(&env, 42);
    let seal = dummy_seal(&env);

    // Should fail because player hasn't authorized
    let result = client.try_submit_score(&player, &seal, &journal);
    assert!(result.is_err());
}

#[test]
fn test_submit_score_invalid_journal_length() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, _token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    // Too short
    let short_journal = Bytes::from_slice(&env, &[0u8; 20]);
    let result = client.try_submit_score(&player, &seal, &short_journal);
    assert_eq!(result, Err(Ok(ScoreError::InvalidJournalLength)));

    // Too long
    let long_journal = Bytes::from_slice(&env, &[0u8; 28]);
    let result = client.try_submit_score(&player, &seal, &long_journal);
    assert_eq!(result, Err(Ok(ScoreError::InvalidJournalLength)));
}

#[test]
fn test_submit_score_wrong_rules_digest() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, _token_addr) = setup(&env);
    let journal = make_journal_bad_rules(&env);
    let seal = dummy_seal(&env);

    let result = client.try_submit_score(&player, &seal, &journal);
    assert_eq!(result, Err(Ok(ScoreError::InvalidRulesDigest)));
}

#[test]
fn test_set_image_id_admin_only() {
    let env = Env::default();
    // Do NOT mock all auths — we want to test auth enforcement

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = dummy_image_id(&env);

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );
    let client = AsteroidsScoreContractClient::new(&env, &contract_id);

    let new_image_id = BytesN::from_array(&env, &[0xBB; 32]);

    // Without mock_all_auths, admin auth is not provided so this fails
    let result = client.try_set_image_id(&new_image_id);
    assert!(result.is_err());
}

#[test]
fn test_set_image_id_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _player, _admin, _token_addr) = setup(&env);

    let new_image_id = BytesN::from_array(&env, &[0xBB; 32]);
    client.set_image_id(&new_image_id);

    assert_eq!(client.image_id(), new_image_id);
}

#[test]
fn test_set_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _player, _admin, _token_addr) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);

    // Verify the new admin can set image_id (indirectly confirms admin was changed)
    let new_image_id = BytesN::from_array(&env, &[0xCC; 32]);
    client.set_image_id(&new_image_id);
    assert_eq!(client.image_id(), new_image_id);
}

#[test]
fn test_is_claimed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, _token_addr) = setup(&env);
    let journal = make_journal(&env, 42);
    let seal = dummy_seal(&env);

    // Compute the digest the same way the contract does
    let journal_digest: BytesN<32> = env.crypto().sha256(&journal).into();

    // Before submission
    assert!(!client.is_claimed(&journal_digest));

    // Submit
    client.submit_score(&player, &seal, &journal);

    // After submission
    assert!(client.is_claimed(&journal_digest));
}

#[test]
fn test_submit_score_different_journals() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, player, _admin, token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    // Submit first journal with score 10
    let journal1 = make_journal(&env, 10);
    let score1 = client.submit_score(&player, &seal, &journal1);
    assert_eq!(score1, 10);

    // Submit second journal with score 20 (different seed to get different digest)
    let mut buf = [0u8; 24];
    buf[0..4].copy_from_slice(&2u32.to_le_bytes()); // different seed
    buf[4..8].copy_from_slice(&100u32.to_le_bytes());
    buf[8..12].copy_from_slice(&20u32.to_le_bytes());
    buf[12..16].copy_from_slice(&99u32.to_le_bytes());
    buf[16..20].copy_from_slice(&0xDEADu32.to_le_bytes());
    buf[20..24].copy_from_slice(&0x4153_5432u32.to_le_bytes());
    let journal2 = Bytes::from_slice(&env, &buf);
    let score2 = client.submit_score(&player, &seal, &journal2);
    assert_eq!(score2, 20);

    // Total balance should be 30
    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&player), 30);
}

// ---------------------------------------------------------------------------
// Integration tests with real proof fixture data
// ---------------------------------------------------------------------------

/// Decode a hex char to its nibble value.
fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex char"),
    }
}

/// Decode hex string into Bytes.
fn hex_to_soroban_bytes(env: &Env, hex: &str) -> Bytes {
    let hex = hex.trim().as_bytes();
    let len = hex.len() / 2;
    let mut result = Bytes::new(env);
    for i in 0..len {
        let byte = (hex_nibble(hex[i * 2]) << 4) | hex_nibble(hex[i * 2 + 1]);
        result.push_back(byte);
    }
    result
}

fn parse_image_id(env: &Env, hex: &str) -> BytesN<32> {
    let id_bytes = hex_to_soroban_bytes(env, hex);
    let mut id_arr = [0u8; 32];
    for i in 0..32 {
        id_arr[i] = id_bytes.get(i as u32).unwrap();
    }
    BytesN::from_array(env, &id_arr)
}

/// Submit a real proof fixture through the contract and verify the result.
fn run_fixture_test(
    seal_hex: &str,
    journal_raw_hex: &str,
    image_id_hex: &str,
    expected_score: u32,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let player = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());

    let seal = hex_to_soroban_bytes(&env, seal_hex);
    let journal_raw = hex_to_soroban_bytes(&env, journal_raw_hex);
    let image_id = parse_image_id(&env, image_id_hex);

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );

    let sac_admin = StellarAssetClient::new(&env, &token_addr);
    sac_admin.set_admin(&contract_id);

    let client = AsteroidsScoreContractClient::new(&env, &contract_id);

    // Submit the real proof data (mock router accepts verify)
    let score = client.submit_score(&player, &seal, &journal_raw);
    assert_eq!(score, expected_score);

    // Verify token mint
    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&player), expected_score as i128);

    // Verify claimed
    let journal_digest: BytesN<32> = env.crypto().sha256(&journal_raw).into();
    assert!(client.is_claimed(&journal_digest));

    // Verify duplicate rejected
    let result = client.try_submit_score(&player, &seal, &journal_raw);
    assert_eq!(result, Err(Ok(ScoreError::JournalAlreadyClaimed)));
}

#[test]
fn test_fixture_short_tape_score_0() {
    run_fixture_test(
        include_str!("../../../../test-fixtures/proof-short-groth16.seal"),
        include_str!("../../../../test-fixtures/proof-short-groth16.journal_raw"),
        include_str!("../../../../test-fixtures/proof-short-groth16.image_id"),
        0, // 500 frames, no scoring
    );
}

#[test]
fn test_fixture_medium_tape_score_90() {
    run_fixture_test(
        include_str!("../../../../test-fixtures/proof-medium-groth16.seal"),
        include_str!("../../../../test-fixtures/proof-medium-groth16.journal_raw"),
        include_str!("../../../../test-fixtures/proof-medium-groth16.image_id"),
        90, // 3980 frames
    );
}

#[test]
fn test_fixture_real_game_score_1880() {
    run_fixture_test(
        include_str!("../../../../test-fixtures/proof-real-game-groth16.seal"),
        include_str!("../../../../test-fixtures/proof-real-game-groth16.journal_raw"),
        include_str!("../../../../test-fixtures/proof-real-game-groth16.image_id"),
        1880, // 6894 frames, real gameplay
    );
}

/// Submit all 3 fixtures to the same contract to verify cumulative token minting
/// and that different journal digests are independently tracked.
#[test]
fn test_fixture_all_three_cumulative() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let player = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());

    // All fixtures share the same image_id
    let image_id = parse_image_id(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.image_id"),
    );

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );

    let sac_admin = StellarAssetClient::new(&env, &token_addr);
    sac_admin.set_admin(&contract_id);

    let client = AsteroidsScoreContractClient::new(&env, &contract_id);
    let token = TokenClient::new(&env, &token_addr);

    // Submit short tape (score 0)
    let seal1 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.seal"),
    );
    let journal1 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.journal_raw"),
    );
    assert_eq!(client.submit_score(&player, &seal1, &journal1), 0);
    assert_eq!(token.balance(&player), 0);

    // Submit medium tape (score 90)
    let seal2 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-medium-groth16.seal"),
    );
    let journal2 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-medium-groth16.journal_raw"),
    );
    assert_eq!(client.submit_score(&player, &seal2, &journal2), 90);
    assert_eq!(token.balance(&player), 90);

    // Submit real game tape (score 1880)
    let seal3 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-real-game-groth16.seal"),
    );
    let journal3 = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-real-game-groth16.journal_raw"),
    );
    assert_eq!(client.submit_score(&player, &seal3, &journal3), 1880);
    assert_eq!(token.balance(&player), 90 + 1880);

    // All three should be claimed
    let d1: BytesN<32> = env.crypto().sha256(&journal1).into();
    let d2: BytesN<32> = env.crypto().sha256(&journal2).into();
    let d3: BytesN<32> = env.crypto().sha256(&journal3).into();
    assert!(client.is_claimed(&d1));
    assert!(client.is_claimed(&d2));
    assert!(client.is_claimed(&d3));
}
