// Frontend + browser runtime defaults and polling intervals.
// Keep operational literals here so they are easy to audit.

export const TESTNET_NETWORK_PASSPHRASE = "Test SDF Network ; September 2015";
export const PUBLIC_NETWORK_PASSPHRASE = "Public Global Stellar Network ; September 2015";
export const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";
export const DEFAULT_ACCOUNT_WASM_HASH =
  "a12e8fa9621efd20315753bd4007d974390e31fbcb4a7ddc4dd0a0dec728bf2e";
export const DEFAULT_WEBAUTHN_VERIFIER_ADDRESS =
  "CBSHV66WG7UV6FQVUTB67P3DZUEJ2KJ5X6JKQH5MFRAAFNFJUAJVXJYV";
export const OPENZEPPELIN_CHANNELS_MAINNET_URL = "https://channels.openzeppelin.com";
export const OPENZEPPELIN_CHANNELS_TESTNET_URL = "https://channels.openzeppelin.com/testnet";
export const DEFAULT_RP_NAME = "Stellar ZK";
export const DEFAULT_SMART_WALLET_USER_NAME = "Player";
export const SMART_WALLET_APP_NAME = "Stellar ZK Asteroids";

export const PROOF_STATUS_INITIAL_POLL_DELAY_MS = 1_200;
export const PROOF_STATUS_POLL_INTERVAL_MS = 3_000;
export const PROOF_STATUS_ERROR_POLL_INTERVAL_MS = 5_000;
export const GATEWAY_HEALTH_INITIAL_POLL_DELAY_MS = 300;
export const GATEWAY_HEALTH_POLL_INTERVAL_MS = 15_000;

export const API_TIMEOUT_SUBMIT_PROOF_MS = 30_000;
export const API_TIMEOUT_GET_PROOF_MS = 10_000;
export const API_TIMEOUT_CANCEL_PROOF_MS = 10_000;
export const API_TIMEOUT_GET_ARTIFACT_MS = 15_000;
export const API_TIMEOUT_GET_GATEWAY_HEALTH_MS = 10_000;
