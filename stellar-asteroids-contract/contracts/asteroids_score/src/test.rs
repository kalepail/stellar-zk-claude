#![cfg(test)]

use crate::{
    AsteroidsScoreContract, AsteroidsScoreContractArgs, AsteroidsScoreContractClient, ScoreError,
};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, token::TokenClient, Address, Bytes, BytesN,
    Env,
};

const RULES_DIGEST_AST3: u32 = 0x4153_5433;

// ---------------------------------------------------------------------------
// Mock router: always accepts verify
// ---------------------------------------------------------------------------
mod mock_router_ok {
    use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env};

    #[contract]
    pub struct MockRouter;

    #[contractimpl]
    impl MockRouter {
        pub fn verify(_env: Env, _seal: Bytes, _image_id: BytesN<32>, _journal: BytesN<32>) {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_journal_24(seed: u32, final_score: u32, rules_digest: u32) -> [u8; 24] {
    let mut buf = [0u8; 24];
    buf[0..4].copy_from_slice(&seed.to_le_bytes());
    buf[4..8].copy_from_slice(&100u32.to_le_bytes());
    buf[8..12].copy_from_slice(&final_score.to_le_bytes());
    buf[12..16].copy_from_slice(&99u32.to_le_bytes());
    buf[16..20].copy_from_slice(&0xDEADu32.to_le_bytes());
    buf[20..24].copy_from_slice(&rules_digest.to_le_bytes());
    buf
}

fn claimant_strkey_bytes(claimant: &Address) -> Bytes {
    claimant.to_string().to_bytes()
}

fn make_journal_with_claimant(env: &Env, seed: u32, final_score: u32, claimant: &Address) -> Bytes {
    let mut journal =
        Bytes::from_slice(env, &base_journal_24(seed, final_score, RULES_DIGEST_AST3));
    let claimant_bytes = claimant_strkey_bytes(claimant);
    journal.extend_from_slice(&claimant_bytes.len().to_le_bytes());
    journal.append(&claimant_bytes);
    journal
}

fn append_claimant(_env: &Env, base_journal_24: &Bytes, claimant: &Address) -> Bytes {
    let mut out = base_journal_24.clone();
    let claimant_bytes = claimant_strkey_bytes(claimant);
    out.extend_from_slice(&claimant_bytes.len().to_le_bytes());
    out.append(&claimant_bytes);
    out
}

fn force_ast3_rules_digest(env: &Env, journal_raw_24: &Bytes) -> Bytes {
    let mut buf = [0u8; 24];
    for i in 0..24 {
        buf[i] = journal_raw_24.get(i as u32).unwrap();
    }
    buf[20..24].copy_from_slice(&RULES_DIGEST_AST3.to_le_bytes());
    Bytes::from_slice(env, &buf)
}

fn dummy_image_id(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xAA; 32])
}

fn dummy_seal(env: &Env) -> Bytes {
    Bytes::from_slice(env, &[0u8; 64])
}

fn setup(env: &Env) -> (AsteroidsScoreContractClient<'_>, Address, Address, Address) {
    let admin = Address::generate(env);
    let claimant = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = dummy_image_id(env);

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );

    let sac_admin = StellarAssetClient::new(env, &token_addr);
    sac_admin.set_admin(&contract_id);

    let client = AsteroidsScoreContractClient::new(env, &contract_id);
    (client, claimant, admin, token_addr)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _claimant, _admin, token_addr) = setup(&env);

    assert_eq!(client.image_id(), dummy_image_id(&env));
    assert_eq!(client.token_id(), token_addr);
    assert_eq!(client.rules_digest(), RULES_DIGEST_AST3);
    let _ = client.router_id();
}

#[test]
fn test_submit_score_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, token_addr) = setup(&env);
    let journal = make_journal_with_claimant(&env, 1, 42, &claimant);
    let seal = dummy_seal(&env);

    let score = client.submit_score(&seal, &journal);
    assert_eq!(score, 42);
    assert_eq!(client.best_score(&claimant, &1), 42);

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), 42);

    let digest: BytesN<32> = env.crypto().sha256(&journal).into();
    assert!(client.is_claimed(&digest));
}

#[test]
fn test_submit_score_duplicate_journal_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, _token_addr) = setup(&env);
    let journal = make_journal_with_claimant(&env, 7, 77, &claimant);
    let seal = dummy_seal(&env);

    client.submit_score(&seal, &journal);
    let result = client.try_submit_score(&seal, &journal);
    assert_eq!(result, Err(Ok(ScoreError::JournalAlreadyClaimed)));
}

#[test]
fn test_submit_score_not_improved_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    let journal_a = make_journal_with_claimant(&env, 9, 80, &claimant);
    client.submit_score(&seal, &journal_a);

    let journal_b = make_journal_with_claimant(&env, 9, 79, &claimant);
    let result = client.try_submit_score(&seal, &journal_b);
    assert_eq!(result, Err(Ok(ScoreError::ScoreNotImproved)));

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), 80);
    assert_eq!(client.best_score(&claimant, &9), 80);
}

#[test]
fn test_submit_score_improvement_mints_delta() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    let journal_a = make_journal_with_claimant(&env, 10, 10, &claimant);
    assert_eq!(client.submit_score(&seal, &journal_a), 10);

    let journal_b = make_journal_with_claimant(&env, 10, 25, &claimant);
    assert_eq!(client.submit_score(&seal, &journal_b), 25);
    assert_eq!(client.best_score(&claimant, &10), 25);

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), 25); // 10 + (25 - 10)
}

#[test]
fn test_submit_score_different_seeds_track_independently() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    assert_eq!(
        client.submit_score(&seal, &make_journal_with_claimant(&env, 1, 10, &claimant)),
        10
    );
    assert_eq!(
        client.submit_score(&seal, &make_journal_with_claimant(&env, 2, 20, &claimant)),
        20
    );

    assert_eq!(client.best_score(&claimant, &1), 10);
    assert_eq!(client.best_score(&claimant, &2), 20);

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), 30);
}

#[test]
fn test_submit_score_invalid_journal_length() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _claimant, _admin, _token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    let short_journal = Bytes::from_slice(&env, &[0u8; 20]);
    let result = client.try_submit_score(&seal, &short_journal);
    assert_eq!(result, Err(Ok(ScoreError::InvalidJournalLength)));

    // Base fields + claimant_len=4, but no claimant bytes present.
    let invalid_len = {
        let mut j = Bytes::from_slice(&env, &base_journal_24(1, 5, RULES_DIGEST_AST3));
        j.extend_from_slice(&4u32.to_le_bytes());
        j
    };
    let result = client.try_submit_score(&seal, &invalid_len);
    assert_eq!(result, Err(Ok(ScoreError::InvalidJournalLength)));
}

#[test]
fn test_submit_score_invalid_claimant_address_length() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _claimant, _admin, _token_addr) = setup(&env);
    let seal = dummy_seal(&env);

    let mut journal = Bytes::from_slice(&env, &base_journal_24(1, 5, RULES_DIGEST_AST3));
    journal.extend_from_slice(&0u32.to_le_bytes());

    let result = client.try_submit_score(&seal, &journal);
    assert_eq!(result, Err(Ok(ScoreError::InvalidClaimantAddressLength)));
}

#[test]
fn test_submit_score_wrong_rules_digest() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, _token_addr) = setup(&env);
    let seal = dummy_seal(&env);
    let mut bad = Bytes::from_slice(&env, &base_journal_24(1, 42, 0xBAAD_F00D));
    let claimant_bytes = claimant_strkey_bytes(&claimant);
    bad.extend_from_slice(&claimant_bytes.len().to_le_bytes());
    bad.append(&claimant_bytes);

    let result = client.try_submit_score(&seal, &bad);
    assert_eq!(result, Err(Ok(ScoreError::InvalidRulesDigest)));
}

#[test]
fn test_submit_score_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, claimant, _admin, token_addr) = setup(&env);
    let seal = dummy_seal(&env);
    let journal = make_journal_with_claimant(&env, 1, 0, &claimant);

    let result = client.try_submit_score(&seal, &journal);
    assert_eq!(result, Err(Ok(ScoreError::ZeroScoreNotAllowed)));

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), 0);
}

#[test]
fn test_set_image_id_admin_only() {
    let env = Env::default();

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
    let result = client.try_set_image_id(&new_image_id);
    assert!(result.is_err());
}

#[test]
fn test_set_image_id_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _claimant, _admin, _token_addr) = setup(&env);
    let new_image_id = BytesN::from_array(&env, &[0xBB; 32]);
    client.set_image_id(&new_image_id);
    assert_eq!(client.image_id(), new_image_id);
}

#[test]
fn test_set_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _claimant, _admin, _token_addr) = setup(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);

    let new_image_id = BytesN::from_array(&env, &[0xCC; 32]);
    client.set_image_id(&new_image_id);
    assert_eq!(client.image_id(), new_image_id);
}

// ---------------------------------------------------------------------------
// Integration tests with real proof fixture data
// ---------------------------------------------------------------------------

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex char"),
    }
}

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

fn run_fixture_test(
    seal_hex: &str,
    journal_raw_hex: &str,
    image_id_hex: &str,
    expected_score: u32,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let claimant = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = parse_image_id(&env, image_id_hex);

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );
    StellarAssetClient::new(&env, &token_addr).set_admin(&contract_id);
    let client = AsteroidsScoreContractClient::new(&env, &contract_id);

    let seal = hex_to_soroban_bytes(&env, seal_hex);
    let base_journal_24 =
        force_ast3_rules_digest(&env, &hex_to_soroban_bytes(&env, journal_raw_hex));
    let journal_raw = append_claimant(&env, &base_journal_24, &claimant);

    let score = client.submit_score(&seal, &journal_raw);
    assert_eq!(score, expected_score);

    let token = TokenClient::new(&env, &token_addr);
    assert_eq!(token.balance(&claimant), expected_score as i128);

    let journal_digest: BytesN<32> = env.crypto().sha256(&journal_raw).into();
    assert!(client.is_claimed(&journal_digest));

    let result = client.try_submit_score(&seal, &journal_raw);
    assert_eq!(result, Err(Ok(ScoreError::JournalAlreadyClaimed)));
}

#[test]
fn test_fixture_short_tape_score_0_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let claimant = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = parse_image_id(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.image_id"),
    );

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );
    StellarAssetClient::new(&env, &token_addr).set_admin(&contract_id);
    let client = AsteroidsScoreContractClient::new(&env, &contract_id);

    let seal = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.seal"),
    );
    let base_journal_24 = force_ast3_rules_digest(
        &env,
        &hex_to_soroban_bytes(
            &env,
            include_str!("../../../../test-fixtures/proof-short-groth16.journal_raw"),
        ),
    );
    let journal_raw = append_claimant(&env, &base_journal_24, &claimant);

    let result = client.try_submit_score(&seal, &journal_raw);
    assert_eq!(result, Err(Ok(ScoreError::ZeroScoreNotAllowed)));
    assert_eq!(TokenClient::new(&env, &token_addr).balance(&claimant), 0);

    let digest: BytesN<32> = env.crypto().sha256(&journal_raw).into();
    assert!(!client.is_claimed(&digest));
}

#[test]
fn test_fixture_medium_tape_score_90() {
    run_fixture_test(
        include_str!("../../../../test-fixtures/proof-medium-groth16.seal"),
        include_str!("../../../../test-fixtures/proof-medium-groth16.journal_raw"),
        include_str!("../../../../test-fixtures/proof-medium-groth16.image_id"),
        90,
    );
}

#[test]
fn test_fixture_real_game_score_32860() {
    run_fixture_test(
        include_str!("../../../../test-fixtures/proof-real-game-groth16.seal"),
        include_str!("../../../../test-fixtures/proof-real-game-groth16.journal_raw"),
        include_str!("../../../../test-fixtures/proof-real-game-groth16.image_id"),
        32860,
    );
}

#[test]
fn test_fixture_all_three_cumulative() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let claimant = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let router_addr = env.register(mock_router_ok::MockRouter, ());
    let image_id = parse_image_id(
        &env,
        include_str!("../../../../test-fixtures/proof-medium-groth16.image_id"),
    );

    let contract_id = env.register(
        AsteroidsScoreContract,
        AsteroidsScoreContractArgs::__constructor(&admin, &router_addr, &image_id, &token_addr),
    );
    StellarAssetClient::new(&env, &token_addr).set_admin(&contract_id);
    let client = AsteroidsScoreContractClient::new(&env, &contract_id);
    let token = TokenClient::new(&env, &token_addr);

    // short (score 0) rejected
    let short_seal = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-short-groth16.seal"),
    );
    let short_base = force_ast3_rules_digest(
        &env,
        &hex_to_soroban_bytes(
            &env,
            include_str!("../../../../test-fixtures/proof-short-groth16.journal_raw"),
        ),
    );
    let short_journal = append_claimant(&env, &short_base, &claimant);
    assert_eq!(
        client.try_submit_score(&short_seal, &short_journal),
        Err(Ok(ScoreError::ZeroScoreNotAllowed))
    );
    assert_eq!(token.balance(&claimant), 0);

    // medium (score 90)
    let medium_seal = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-medium-groth16.seal"),
    );
    let medium_base = force_ast3_rules_digest(
        &env,
        &hex_to_soroban_bytes(
            &env,
            include_str!("../../../../test-fixtures/proof-medium-groth16.journal_raw"),
        ),
    );
    let medium_journal = append_claimant(&env, &medium_base, &claimant);
    assert_eq!(client.submit_score(&medium_seal, &medium_journal), 90);
    assert_eq!(token.balance(&claimant), 90);

    // real game (score 32860, different seed)
    let real_seal = hex_to_soroban_bytes(
        &env,
        include_str!("../../../../test-fixtures/proof-real-game-groth16.seal"),
    );
    let real_base = force_ast3_rules_digest(
        &env,
        &hex_to_soroban_bytes(
            &env,
            include_str!("../../../../test-fixtures/proof-real-game-groth16.journal_raw"),
        ),
    );
    let real_journal = append_claimant(&env, &real_base, &claimant);
    assert_eq!(client.submit_score(&real_seal, &real_journal), 32860);
    assert_eq!(token.balance(&claimant), 90 + 32860);
}
