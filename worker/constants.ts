export const COORDINATOR_OBJECT_NAME = "global-proof-coordinator";
export const TESTNET_NETWORK_PASSPHRASE = "Test SDF Network ; September 2015";
export const DEFAULT_BINDINGS_RPC_URL = "https://soroban-testnet.stellar.org";
export const OPENZEPPELIN_CHANNELS_HOSTNAME = "channels.openzeppelin.com";
export const DEFAULT_RELAYER_REQUEST_TIMEOUT_MS = 30_000;

export const TAPE_MAGIC = 0x5a4b5450;
export const TAPE_VERSION = 2;
export const TAPE_HEADER_SIZE = 16;
export const TAPE_FOOTER_SIZE = 12;

export const DEFAULT_MAX_TAPE_BYTES = 2 * 1024 * 1024;
export const DEFAULT_POLL_INTERVAL_MS = 3_000;
export const MIN_PROVER_POLL_INTERVAL_MS = 500;
// Target: typical proofs ~5 min; accept up to 10 min before timing out.
// This is primarily a safety bound if someone configures an overly large poll budget.
export const DEFAULT_POLL_TIMEOUT_MS = 11 * 60_000;
export const MIN_PROVER_POLL_TIMEOUT_MS = 5_000;
export const DEFAULT_PROVER_REQUEST_TIMEOUT_MS = 30_000;
export const DEFAULT_POLL_BUDGET_MS = 45_000;
export const DEFAULT_PROVER_HEALTH_CACHE_MS = 30_000;
export const OPPORTUNISTIC_POLL_STALE_MS = 5_000;
// End-to-end job lifetime cap in the gateway (includes queue + polling + storage).
// Keep slightly above the prover timeout to allow status propagation + artifact writes.
export const DEFAULT_MAX_JOB_WALL_TIME_MS = 11 * 60_000; // 11 minutes
export const DEFAULT_MAX_COMPLETED_JOBS = 200;
export const DEFAULT_COMPLETED_JOB_RETENTION_MS = 24 * 60 * 60_000; // 24 hours
export const MAX_PROVER_RECOVERY_ATTEMPTS = 1;

export const DEFAULT_SEGMENT_LIMIT_PO2 = 21;

// With a ~10 minute proving SLA, a 5 minute retry backoff just wastes time.
// Keep retries snappy so transient network/prover issues either recover quickly or fail fast.
export const MAX_RETRY_DELAY_SECONDS = 60;

// Must match the max_retries value in wrangler.jsonc queue consumer config.
// After this many delivery attempts (attempts >= MAX_QUEUE_RETRIES), the job is marked
// as permanently failed rather than retried again.
export const MAX_QUEUE_RETRIES = 10;
export const EXPECTED_RULES_TAG = 3; // "AST3"
export const EXPECTED_RULES_DIGEST = 0x41535433; // "AST3"
export const EXPECTED_RULESET = "AST3";

export const RETRYABLE_JOB_ERROR_CODES = new Set([
  "server_restarted",
  "proof_error",
  "internal_error",
]);

export const ACTIVE_JOB_KEY = "active_job_id";
export const JOB_KEY_PREFIX = "job:";
export const PROFILE_KEY_PREFIX = "profile:";
export const LEADERBOARD_EVENT_KEY_PREFIX = "lb:event:";
export const LEADERBOARD_INGESTION_STATE_KEY = "lb:ingestion-state";
