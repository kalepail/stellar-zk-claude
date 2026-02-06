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
export const DEFAULT_MAX_QUEUE_ATTEMPTS = 180;

export const MAX_RETRY_DELAY_SECONDS = 300;

export const ACTIVE_JOB_KEY = "active_job_id";
export const JOB_KEY_PREFIX = "job:";
