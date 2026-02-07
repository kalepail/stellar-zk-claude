export const COORDINATOR_OBJECT_NAME = "global-proof-coordinator";

export const TAPE_MAGIC = 0x5a4b5450;
export const TAPE_VERSION = 1;
export const TAPE_HEADER_SIZE = 16;
export const TAPE_FOOTER_SIZE = 12;

export const DEFAULT_MAX_TAPE_BYTES = 2 * 1024 * 1024;
export const DEFAULT_POLL_INTERVAL_MS = 3_000;
export const DEFAULT_POLL_TIMEOUT_MS = 15 * 60_000;
export const DEFAULT_PROVER_REQUEST_TIMEOUT_MS = 30_000;
export const DEFAULT_POLL_BUDGET_MS = 45_000;
export const DEFAULT_MAX_JOB_WALL_TIME_MS = 60 * 60_000; // 1 hour
export const DEFAULT_MAX_COMPLETED_JOBS = 200;
export const DEFAULT_COMPLETED_JOB_RETENTION_MS = 24 * 60 * 60_000; // 24 hours

export const MAX_RETRY_DELAY_SECONDS = 300;

// Must match the max_retries value in wrangler.jsonc queue consumer config.
// After this many delivery attempts (attempts >= MAX_QUEUE_RETRIES), the job is marked
// as permanently failed rather than retried again.
export const MAX_QUEUE_RETRIES = 10;

export const RETRYABLE_JOB_ERROR_CODES = new Set([
  "server_restarted",
  "internal_error",
]);

export const ACTIVE_JOB_KEY = "active_job_id";
export const JOB_KEY_PREFIX = "job:";
