import type { WorkerEnv } from "./env";
import type {
  LeaderboardEventRecord,
  LeaderboardIngestionState,
  LeaderboardWindow,
  PlayerProfileRecord,
} from "./types";

const TEN_MINUTES_MS = 10 * 60 * 1000;
const ONE_DAY_MS = 24 * 60 * 60 * 1000;

const schemaInitByDb = new WeakMap<D1Database, Promise<void>>();

export interface LeaderboardProfileCredentialRecord {
  claimantAddress: string;
  credentialId: string;
  publicKey: string;
  counter: number;
  transports: string[] | null;
  createdAt: string;
  updatedAt: string;
}

export interface LeaderboardProfileAuthChallengeRecord {
  challengeId: string;
  claimantAddress: string;
  credentialId: string;
  challenge: string;
  expectedOrigin: string;
  expectedRpId: string;
  createdAt: string;
  expiresAt: string;
  usedAt: string | null;
}

function getDb(env: WorkerEnv): D1Database {
  if (!env.LEADERBOARD_DB) {
    throw new Error("LEADERBOARD_DB binding is not configured");
  }
  return env.LEADERBOARD_DB;
}

function weakHashHex(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function eventRowHash(event: LeaderboardEventRecord): string {
  return weakHashHex(
    JSON.stringify([
      event.eventId,
      event.claimantAddress,
      event.seed >>> 0,
      event.frameCount === null ? null : event.frameCount >>> 0,
      event.finalScore >>> 0,
      event.finalRngState === null ? null : event.finalRngState >>> 0,
      event.tapeChecksum === null ? null : event.tapeChecksum >>> 0,
      event.rulesDigest === null ? null : event.rulesDigest >>> 0,
      event.previousBest >>> 0,
      event.newBest >>> 0,
      event.mintedDelta >>> 0,
      event.journalDigest ?? "",
      event.txHash ?? "",
      event.eventIndex ?? -1,
      event.ledger ?? -1,
      event.closedAt,
      event.source,
      event.ingestedAt,
    ]),
  );
}

function toNumber(value: unknown, fallback: number): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number.parseInt(value, 10);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return fallback;
}

function toNullableString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function toNullableU32(value: unknown): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  const parsed = toNumber(value, Number.NaN);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return null;
  }
  return Math.trunc(parsed) >>> 0;
}

function parseOptionalJsonStringArray(value: unknown): string[] | null {
  if (typeof value !== "string" || value.length === 0) {
    return null;
  }

  try {
    const parsed = JSON.parse(value);
    if (!Array.isArray(parsed)) {
      return null;
    }
    return parsed.filter((entry): entry is string => typeof entry === "string");
  } catch {
    return null;
  }
}

function normalizeProfileCredentialRow(
  row: Record<string, unknown> | null,
): LeaderboardProfileCredentialRecord | null {
  if (!row) {
    return null;
  }

  const claimantAddress = toNullableString(row.claimant_address);
  const credentialId = toNullableString(row.credential_id);
  const publicKey = toNullableString(row.public_key);
  const createdAt = toNullableString(row.created_at);
  const updatedAt = toNullableString(row.updated_at);
  if (!claimantAddress || !credentialId || !publicKey || !createdAt || !updatedAt) {
    return null;
  }

  return {
    claimantAddress,
    credentialId,
    publicKey,
    counter: toNumber(row.counter, 0),
    transports: parseOptionalJsonStringArray(row.transports),
    createdAt,
    updatedAt,
  };
}

function normalizeProfileAuthChallengeRow(
  row: Record<string, unknown> | null,
): LeaderboardProfileAuthChallengeRecord | null {
  if (!row) {
    return null;
  }

  const challengeId = toNullableString(row.challenge_id);
  const claimantAddress = toNullableString(row.claimant_address);
  const credentialId = toNullableString(row.credential_id);
  const challenge = toNullableString(row.challenge);
  const expectedOrigin = toNullableString(row.expected_origin);
  const expectedRpId = toNullableString(row.expected_rp_id);
  const createdAt = toNullableString(row.created_at);
  const expiresAt = toNullableString(row.expires_at);
  if (
    !challengeId ||
    !claimantAddress ||
    !credentialId ||
    !challenge ||
    !expectedOrigin ||
    !expectedRpId ||
    !createdAt ||
    !expiresAt
  ) {
    return null;
  }

  return {
    challengeId,
    claimantAddress,
    credentialId,
    challenge,
    expectedOrigin,
    expectedRpId,
    createdAt,
    expiresAt,
    usedAt: toNullableString(row.used_at),
  };
}

function getWindowCutoffIso(window: LeaderboardWindow, nowMs: number): string | null {
  if (window === "10m") {
    return new Date(nowMs - TEN_MINUTES_MS).toISOString();
  }
  if (window === "day") {
    return new Date(nowMs - ONE_DAY_MS).toISOString();
  }
  return null;
}

function getWindowRange(
  window: LeaderboardWindow,
  nowMs: number,
): { startAt: string | null; endAt: string } {
  const cutoffIso = getWindowCutoffIso(window, nowMs);
  return {
    startAt: cutoffIso,
    endAt: new Date(nowMs).toISOString(),
  };
}

async function getTableColumnNames(db: D1Database, table: string): Promise<Set<string>> {
  const rows = await db.prepare(`PRAGMA table_info(${table})`).all<Record<string, unknown>>();
  const names = new Set<string>();
  for (const row of rows.results ?? []) {
    if (typeof row.name === "string" && row.name.length > 0) {
      names.add(row.name);
    }
  }
  return names;
}

async function ensureLeaderboardEventColumns(db: D1Database): Promise<void> {
  const columns = await getTableColumnNames(db, "leaderboard_events");
  if (columns.size === 0) {
    return;
  }

  const alterStatements: string[] = [];
  if (!columns.has("frame_count")) {
    alterStatements.push("ALTER TABLE leaderboard_events ADD COLUMN frame_count INTEGER");
  }
  if (!columns.has("final_score")) {
    alterStatements.push("ALTER TABLE leaderboard_events ADD COLUMN final_score INTEGER");
  }
  if (!columns.has("final_rng_state")) {
    alterStatements.push("ALTER TABLE leaderboard_events ADD COLUMN final_rng_state INTEGER");
  }
  if (!columns.has("tape_checksum")) {
    alterStatements.push("ALTER TABLE leaderboard_events ADD COLUMN tape_checksum INTEGER");
  }
  if (!columns.has("rules_digest")) {
    alterStatements.push("ALTER TABLE leaderboard_events ADD COLUMN rules_digest INTEGER");
  }

  /* eslint-disable no-await-in-loop */
  for (const statement of alterStatements) {
    await db.prepare(statement).run();
  }
  /* eslint-enable no-await-in-loop */
}

async function ensureSchema(env: WorkerEnv): Promise<void> {
  const db = getDb(env);
  let schemaInitPromise = schemaInitByDb.get(db);
  if (!schemaInitPromise) {
    schemaInitPromise = (async () => {
      const schemaStatements = [
        `CREATE TABLE IF NOT EXISTS leaderboard_events (
          event_id TEXT PRIMARY KEY,
          claimant_address TEXT NOT NULL,
          seed INTEGER NOT NULL,
          frame_count INTEGER,
          final_score INTEGER,
          final_rng_state INTEGER,
          tape_checksum INTEGER,
          rules_digest INTEGER,
          previous_best INTEGER NOT NULL,
          new_best INTEGER NOT NULL,
          minted_delta INTEGER NOT NULL,
          journal_digest TEXT,
          tx_hash TEXT,
          event_index INTEGER,
          ledger INTEGER,
          closed_at TEXT NOT NULL,
          source TEXT NOT NULL,
          ingested_at TEXT NOT NULL,
          row_hash TEXT NOT NULL
        )`,
        `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_closed_at
          ON leaderboard_events(closed_at DESC)`,
        `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_claimant_closed_at
          ON leaderboard_events(claimant_address, closed_at DESC)`,
        `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_claimant_best
          ON leaderboard_events(claimant_address, new_best DESC, closed_at ASC, event_id ASC)`,
        `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_window_rank
          ON leaderboard_events(closed_at DESC, new_best DESC, claimant_address, event_id ASC)`,
        `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_seed_closed_at
          ON leaderboard_events(seed, closed_at DESC)`,
        `CREATE TABLE IF NOT EXISTS leaderboard_profiles (
          claimant_address TEXT PRIMARY KEY,
          username TEXT,
          link_url TEXT,
          updated_at TEXT NOT NULL
        )`,
        `CREATE TABLE IF NOT EXISTS leaderboard_profile_credentials (
          credential_id TEXT PRIMARY KEY,
          claimant_address TEXT NOT NULL,
          public_key TEXT NOT NULL,
          counter INTEGER NOT NULL DEFAULT 0,
          transports TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        )`,
        `CREATE INDEX IF NOT EXISTS idx_lb_profile_credentials_claimant
          ON leaderboard_profile_credentials(claimant_address)`,
        `CREATE TABLE IF NOT EXISTS leaderboard_profile_auth_challenges (
          challenge_id TEXT PRIMARY KEY,
          claimant_address TEXT NOT NULL,
          credential_id TEXT NOT NULL,
          challenge TEXT NOT NULL,
          expected_origin TEXT NOT NULL,
          expected_rp_id TEXT NOT NULL,
          created_at TEXT NOT NULL,
          expires_at TEXT NOT NULL,
          used_at TEXT
        )`,
        `CREATE INDEX IF NOT EXISTS idx_lb_profile_auth_challenges_lookup
          ON leaderboard_profile_auth_challenges(claimant_address, credential_id, created_at DESC)`,
        `CREATE TABLE IF NOT EXISTS leaderboard_ingestion_state (
          singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
          provider TEXT NOT NULL,
          source_mode TEXT NOT NULL,
          cursor TEXT,
          highest_ledger INTEGER,
          last_synced_at TEXT,
          last_backfill_at TEXT,
          total_events INTEGER NOT NULL DEFAULT 0,
          last_error TEXT
        )`,
      ];

      // D1 local dev can fail on multi-statement exec; run DDL one-by-one.
      /* eslint-disable no-await-in-loop */
      for (const statement of schemaStatements) {
        await db.prepare(statement).run();
      }
      await ensureLeaderboardEventColumns(db);
      await db
        .prepare(
          `CREATE INDEX IF NOT EXISTS idx_leaderboard_events_rules_digest
            ON leaderboard_events(rules_digest)`,
        )
        .run();
      /* eslint-enable no-await-in-loop */
    })().catch((error) => {
      schemaInitByDb.delete(db);
      throw error;
    });
    schemaInitByDb.set(db, schemaInitPromise);
  }

  await schemaInitPromise;
}

function normalizeIngestionStateRow(
  row: Record<string, unknown> | null,
): LeaderboardIngestionState {
  if (!row) {
    return {
      provider: "galexie",
      sourceMode: "datalake",
      cursor: null,
      highestLedger: null,
      lastSyncedAt: null,
      lastBackfillAt: null,
      totalEvents: 0,
      lastError: null,
    };
  }

  const provider = row.provider === "rpc" ? "rpc" : "galexie";
  const sourceMode =
    row.source_mode === "rpc" || row.source_mode === "events_api" || row.source_mode === "datalake"
      ? row.source_mode
      : provider === "rpc"
        ? "rpc"
        : "datalake";

  return {
    provider,
    sourceMode,
    cursor: toNullableString(row.cursor),
    highestLedger:
      typeof row.highest_ledger === "number" && Number.isFinite(row.highest_ledger)
        ? Math.trunc(row.highest_ledger)
        : null,
    lastSyncedAt: toNullableString(row.last_synced_at),
    lastBackfillAt: toNullableString(row.last_backfill_at),
    totalEvents: toNumber(row.total_events, 0),
    lastError: toNullableString(row.last_error),
  };
}

async function getExistingEventHashes(
  env: WorkerEnv,
  eventIds: string[],
): Promise<Map<string, string>> {
  await ensureSchema(env);
  const db = getDb(env);
  const out = new Map<string, string>();

  const chunkSize = 200;
  const chunks: string[][] = [];
  for (let index = 0; index < eventIds.length; index += chunkSize) {
    const chunk = eventIds.slice(index, index + chunkSize);
    if (chunk.length > 0) {
      chunks.push(chunk);
    }
  }

  const pages = await Promise.all(
    chunks.map((chunk) => {
      const placeholders = chunk.map(() => "?").join(",");
      return db
        .prepare(
          `SELECT event_id, row_hash FROM leaderboard_events WHERE event_id IN (${placeholders})`,
        )
        .bind(...chunk)
        .all<{ event_id: string; row_hash: string }>();
    }),
  );

  for (const page of pages) {
    for (const row of page.results ?? []) {
      if (row.event_id) {
        out.set(row.event_id, row.row_hash);
      }
    }
  }

  return out;
}

export async function getLeaderboardIngestionState(
  env: WorkerEnv,
): Promise<LeaderboardIngestionState> {
  await ensureSchema(env);
  const db = getDb(env);
  const row = await db
    .prepare(
      `SELECT provider, source_mode, cursor, highest_ledger, last_synced_at, last_backfill_at, total_events, last_error
       FROM leaderboard_ingestion_state
       WHERE singleton_id = 1`,
    )
    .first<Record<string, unknown>>();
  return normalizeIngestionStateRow(row ?? null);
}

export async function countLeaderboardEvents(env: WorkerEnv): Promise<number> {
  await ensureSchema(env);
  const db = getDb(env);
  const row = await db
    .prepare("SELECT COUNT(*) AS total FROM leaderboard_events")
    .first<Record<string, unknown>>();
  return toNumber(row?.total, 0);
}

export async function setLeaderboardIngestionState(
  env: WorkerEnv,
  state: LeaderboardIngestionState,
): Promise<void> {
  await ensureSchema(env);
  const db = getDb(env);

  await db
    .prepare(
      `INSERT INTO leaderboard_ingestion_state (
          singleton_id, provider, source_mode, cursor, highest_ledger, last_synced_at, last_backfill_at, total_events, last_error
        )
        VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(singleton_id) DO UPDATE SET
          provider = excluded.provider,
          source_mode = excluded.source_mode,
          cursor = excluded.cursor,
          highest_ledger = excluded.highest_ledger,
          last_synced_at = excluded.last_synced_at,
          last_backfill_at = excluded.last_backfill_at,
          total_events = excluded.total_events,
          last_error = excluded.last_error`,
    )
    .bind(
      state.provider,
      state.sourceMode,
      state.cursor,
      state.highestLedger,
      state.lastSyncedAt,
      state.lastBackfillAt,
      state.totalEvents,
      state.lastError,
    )
    .run();
}

export async function upsertLeaderboardEvents(
  env: WorkerEnv,
  events: LeaderboardEventRecord[],
): Promise<{ inserted: number; updated: number }> {
  await ensureSchema(env);
  const db = getDb(env);

  if (events.length === 0) {
    return { inserted: 0, updated: 0 };
  }

  const deduped = new Map<string, LeaderboardEventRecord>();
  for (const event of events) {
    deduped.set(event.eventId, event);
  }
  const normalized = Array.from(deduped.values());
  const existingHashes = await getExistingEventHashes(
    env,
    normalized.map((event) => event.eventId),
  );

  const upsert = db.prepare(
    `INSERT INTO leaderboard_events (
      event_id, claimant_address, seed, frame_count, final_score, final_rng_state, tape_checksum, rules_digest,
      previous_best, new_best, minted_delta, journal_digest,
      tx_hash, event_index, ledger, closed_at, source, ingested_at, row_hash
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(event_id) DO UPDATE SET
      claimant_address = excluded.claimant_address,
      seed = excluded.seed,
      frame_count = excluded.frame_count,
      final_score = excluded.final_score,
      final_rng_state = excluded.final_rng_state,
      tape_checksum = excluded.tape_checksum,
      rules_digest = excluded.rules_digest,
      previous_best = excluded.previous_best,
      new_best = excluded.new_best,
      minted_delta = excluded.minted_delta,
      journal_digest = excluded.journal_digest,
      tx_hash = excluded.tx_hash,
      event_index = excluded.event_index,
      ledger = excluded.ledger,
      closed_at = excluded.closed_at,
      source = excluded.source,
      ingested_at = excluded.ingested_at,
      row_hash = excluded.row_hash`,
  );

  let inserted = 0;
  let updated = 0;
  const statements: D1PreparedStatement[] = [];
  for (const event of normalized) {
    const frameCount = toNullableU32(event.frameCount);
    const finalScore = toNullableU32(event.finalScore);
    if (finalScore === null) {
      continue;
    }
    const finalRngState = toNullableU32(event.finalRngState);
    const tapeChecksum = toNullableU32(event.tapeChecksum);
    const rulesDigest = toNullableU32(event.rulesDigest);
    const normalizedEvent: LeaderboardEventRecord = {
      ...event,
      frameCount,
      finalScore,
      finalRngState,
      tapeChecksum,
      rulesDigest,
    };

    const rowHash = eventRowHash(normalizedEvent);
    const existingHash = existingHashes.get(event.eventId);
    if (!existingHash) {
      inserted += 1;
    } else if (existingHash !== rowHash) {
      updated += 1;
    } else {
      continue;
    }

    statements.push(
      upsert.bind(
        event.eventId,
        event.claimantAddress,
        event.seed >>> 0,
        frameCount,
        finalScore >>> 0,
        finalRngState,
        tapeChecksum,
        rulesDigest,
        event.previousBest >>> 0,
        event.newBest >>> 0,
        event.mintedDelta >>> 0,
        event.journalDigest,
        event.txHash,
        event.eventIndex,
        event.ledger,
        event.closedAt,
        event.source,
        event.ingestedAt,
        rowHash,
      ),
    );
  }

  if (statements.length > 0) {
    await db.batch(statements);
  }

  return { inserted, updated };
}

export async function upsertLeaderboardProfile(
  env: WorkerEnv,
  claimantAddress: string,
  updates: { username: string | null; linkUrl: string | null; updatedAt?: string | null },
): Promise<PlayerProfileRecord> {
  await ensureSchema(env);
  const db = getDb(env);
  const updatedAt =
    typeof updates.updatedAt === "string" && updates.updatedAt.length > 0
      ? updates.updatedAt
      : new Date().toISOString();

  await db
    .prepare(
      `INSERT INTO leaderboard_profiles (claimant_address, username, link_url, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(claimant_address) DO UPDATE SET
          username = excluded.username,
          link_url = excluded.link_url,
          updated_at = excluded.updated_at`,
    )
    .bind(claimantAddress, updates.username, updates.linkUrl, updatedAt)
    .run();

  return {
    claimantAddress,
    username: updates.username,
    linkUrl: updates.linkUrl,
    updatedAt,
  };
}

export async function upsertLeaderboardProfiles(
  env: WorkerEnv,
  profiles: PlayerProfileRecord[],
): Promise<number> {
  await ensureSchema(env);
  const db = getDb(env);
  if (profiles.length === 0) {
    return 0;
  }

  const upsert = db.prepare(
    `INSERT INTO leaderboard_profiles (claimant_address, username, link_url, updated_at)
      VALUES (?, ?, ?, ?)
      ON CONFLICT(claimant_address) DO UPDATE SET
        username = excluded.username,
        link_url = excluded.link_url,
        updated_at = excluded.updated_at`,
  );

  const statements: D1PreparedStatement[] = profiles.map((profile) =>
    upsert.bind(
      profile.claimantAddress,
      profile.username,
      profile.linkUrl,
      profile.updatedAt && profile.updatedAt.length > 0
        ? profile.updatedAt
        : new Date().toISOString(),
    ),
  );
  await db.batch(statements);
  return statements.length;
}

export async function getLeaderboardProfileCredential(
  env: WorkerEnv,
  credentialId: string,
): Promise<LeaderboardProfileCredentialRecord | null> {
  await ensureSchema(env);
  const db = getDb(env);
  const row = await db
    .prepare(
      `SELECT
        credential_id,
        claimant_address,
        public_key,
        counter,
        transports,
        created_at,
        updated_at
       FROM leaderboard_profile_credentials
       WHERE credential_id = ?
       LIMIT 1`,
    )
    .bind(credentialId)
    .first<Record<string, unknown>>();
  return normalizeProfileCredentialRow(row ?? null);
}

export async function upsertLeaderboardProfileCredential(
  env: WorkerEnv,
  input: {
    claimantAddress: string;
    credentialId: string;
    publicKey: string;
    transports?: string[] | null;
  },
): Promise<LeaderboardProfileCredentialRecord> {
  await ensureSchema(env);
  const db = getDb(env);
  const existing = await getLeaderboardProfileCredential(env, input.credentialId);
  const nowIso = new Date().toISOString();
  const transportsJson =
    input.transports && input.transports.length > 0 ? JSON.stringify(input.transports) : null;

  if (existing) {
    if (existing.claimantAddress !== input.claimantAddress) {
      throw new Error("credential is already bound to another claimant address");
    }
    if (existing.publicKey !== input.publicKey) {
      throw new Error("credential public key mismatch");
    }

    await db
      .prepare(
        `UPDATE leaderboard_profile_credentials
         SET transports = ?, updated_at = ?
         WHERE credential_id = ?`,
      )
      .bind(transportsJson ?? JSON.stringify(existing.transports ?? []), nowIso, input.credentialId)
      .run();
    return {
      ...existing,
      transports: input.transports ?? existing.transports,
      updatedAt: nowIso,
    };
  }

  await db
    .prepare(
      `INSERT INTO leaderboard_profile_credentials (
        credential_id,
        claimant_address,
        public_key,
        counter,
        transports,
        created_at,
        updated_at
      ) VALUES (?, ?, ?, 0, ?, ?, ?)`,
    )
    .bind(
      input.credentialId,
      input.claimantAddress,
      input.publicKey,
      transportsJson,
      nowIso,
      nowIso,
    )
    .run();

  return {
    claimantAddress: input.claimantAddress,
    credentialId: input.credentialId,
    publicKey: input.publicKey,
    counter: 0,
    transports: input.transports ?? null,
    createdAt: nowIso,
    updatedAt: nowIso,
  };
}

export async function updateLeaderboardProfileCredentialCounter(
  env: WorkerEnv,
  credentialId: string,
  counter: number,
): Promise<void> {
  await ensureSchema(env);
  const db = getDb(env);
  await db
    .prepare(
      `UPDATE leaderboard_profile_credentials
       SET counter = ?, updated_at = ?
       WHERE credential_id = ?`,
    )
    .bind(Math.max(0, Math.trunc(counter)), new Date().toISOString(), credentialId)
    .run();
}

export async function createLeaderboardProfileAuthChallenge(
  env: WorkerEnv,
  input: {
    challengeId: string;
    claimantAddress: string;
    credentialId: string;
    challenge: string;
    expectedOrigin: string;
    expectedRpId: string;
    expiresAt: string;
  },
): Promise<LeaderboardProfileAuthChallengeRecord> {
  await ensureSchema(env);
  const db = getDb(env);
  const nowIso = new Date().toISOString();
  await db
    .prepare(
      `INSERT INTO leaderboard_profile_auth_challenges (
        challenge_id,
        claimant_address,
        credential_id,
        challenge,
        expected_origin,
        expected_rp_id,
        created_at,
        expires_at,
        used_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)`,
    )
    .bind(
      input.challengeId,
      input.claimantAddress,
      input.credentialId,
      input.challenge,
      input.expectedOrigin,
      input.expectedRpId,
      nowIso,
      input.expiresAt,
    )
    .run();

  return {
    challengeId: input.challengeId,
    claimantAddress: input.claimantAddress,
    credentialId: input.credentialId,
    challenge: input.challenge,
    expectedOrigin: input.expectedOrigin,
    expectedRpId: input.expectedRpId,
    createdAt: nowIso,
    expiresAt: input.expiresAt,
    usedAt: null,
  };
}

export async function getLeaderboardProfileAuthChallenge(
  env: WorkerEnv,
  challengeId: string,
): Promise<LeaderboardProfileAuthChallengeRecord | null> {
  await ensureSchema(env);
  const db = getDb(env);
  const row = await db
    .prepare(
      `SELECT
        challenge_id,
        claimant_address,
        credential_id,
        challenge,
        expected_origin,
        expected_rp_id,
        created_at,
        expires_at,
        used_at
       FROM leaderboard_profile_auth_challenges
       WHERE challenge_id = ?
       LIMIT 1`,
    )
    .bind(challengeId)
    .first<Record<string, unknown>>();
  return normalizeProfileAuthChallengeRow(row ?? null);
}

export async function markLeaderboardProfileAuthChallengeUsed(
  env: WorkerEnv,
  challengeId: string,
): Promise<boolean> {
  await ensureSchema(env);
  const db = getDb(env);
  const result = await db
    .prepare(
      `UPDATE leaderboard_profile_auth_challenges
       SET used_at = ?
       WHERE challenge_id = ? AND used_at IS NULL`,
    )
    .bind(new Date().toISOString(), challengeId)
    .run();
  return Boolean((result.meta?.changes ?? 0) > 0);
}

export async function purgeExpiredLeaderboardProfileAuthChallenges(
  env: WorkerEnv,
  nowIso = new Date().toISOString(),
): Promise<void> {
  await ensureSchema(env);
  const db = getDb(env);
  await db
    .prepare(
      `DELETE FROM leaderboard_profile_auth_challenges
       WHERE expires_at < ? OR used_at IS NOT NULL`,
    )
    .bind(nowIso)
    .run();
}

function windowWhereClause(window: LeaderboardWindow): string {
  return window === "all" ? "" : "WHERE closed_at >= ?";
}

function windowParams(window: LeaderboardWindow, nowMs: number): unknown[] {
  const cutoff = getWindowCutoffIso(window, nowMs);
  return cutoff ? [cutoff] : [];
}

function rankedQueryCteSql(whereClause: string): string {
  return `WITH filtered AS (
    SELECT
      event_id,
      claimant_address,
      seed,
      frame_count,
      final_score,
      final_rng_state,
      tape_checksum,
      rules_digest,
      new_best,
      minted_delta,
      tx_hash,
      closed_at
    FROM leaderboard_events
    ${whereClause}
  ),
  best_per_claimant AS (
    SELECT *,
      ROW_NUMBER() OVER (
        PARTITION BY claimant_address
        ORDER BY new_best DESC, closed_at ASC, event_id ASC
      ) AS claimant_rank
    FROM filtered
  ),
  ranked AS (
    SELECT
      event_id,
      claimant_address,
      seed,
      frame_count,
      final_score,
      final_rng_state,
      tape_checksum,
      rules_digest,
      new_best,
      minted_delta,
      tx_hash,
      closed_at,
      ROW_NUMBER() OVER (ORDER BY new_best DESC, closed_at ASC, event_id ASC) AS rank
    FROM best_per_claimant
    WHERE claimant_rank = 1
  )`;
}

function mapRankedEntry(row: Record<string, unknown>): {
  rank: number;
  jobId: string;
  claimantAddress: string;
  score: number;
  mintedDelta: number;
  seed: number;
  frameCount: number | null;
  finalRngState: number | null;
  tapeChecksum: number | null;
  rulesDigest: number | null;
  completedAt: string;
  claimStatus: "succeeded";
  claimTxHash: string | null;
  profile: PlayerProfileRecord | null;
} {
  const profileUpdatedAt = toNullableString(row.profile_updated_at);
  const profileUsername = toNullableString(row.profile_username);
  const profileLinkUrl = toNullableString(row.profile_link_url);

  return {
    rank: toNumber(row.rank, 0),
    jobId: String(row.job_id),
    claimantAddress: String(row.claimant_address),
    score: toNumber(row.score, 0),
    mintedDelta: toNumber(row.minted_delta, 0),
    seed: toNumber(row.seed, 0) >>> 0,
    frameCount: toNullableU32(row.frame_count),
    finalRngState: toNullableU32(row.final_rng_state),
    tapeChecksum: toNullableU32(row.tape_checksum),
    rulesDigest: toNullableU32(row.rules_digest),
    completedAt: String(row.completed_at),
    claimStatus: "succeeded",
    claimTxHash: toNullableString(row.claim_tx_hash),
    profile:
      profileUpdatedAt || profileUsername || profileLinkUrl
        ? {
            claimantAddress: String(row.claimant_address),
            username: profileUsername,
            linkUrl: profileLinkUrl,
            updatedAt: profileUpdatedAt ?? new Date(0).toISOString(),
          }
        : null,
  };
}

export async function getLeaderboardPage(
  env: WorkerEnv,
  options: {
    window: LeaderboardWindow;
    limit: number;
    offset: number;
    claimantAddress: string | null;
    nowMs?: number;
  },
): Promise<{
  window: LeaderboardWindow;
  generatedAt: string;
  windowRange: { startAt: string | null; endAt: string };
  totalPlayers: number;
  limit: number;
  offset: number;
  nextOffset: number | null;
  entries: ReturnType<typeof mapRankedEntry>[];
  me: ReturnType<typeof mapRankedEntry> | null;
}> {
  await ensureSchema(env);
  const db = getDb(env);
  const nowMs = options.nowMs ?? Date.now();
  const whereClause = windowWhereClause(options.window);
  const whereParams = windowParams(options.window, nowMs);

  const totalRow = await db
    .prepare(
      `${rankedQueryCteSql(whereClause)}
       SELECT COUNT(*) AS total
       FROM ranked`,
    )
    .bind(...whereParams)
    .first<Record<string, unknown>>();

  const totalPlayers = toNumber(totalRow?.total, 0);

  const entriesRows = await db
    .prepare(
      `${rankedQueryCteSql(whereClause)}
       SELECT
         ranked.rank AS rank,
         ranked.event_id AS job_id,
         ranked.claimant_address AS claimant_address,
         ranked.new_best AS score,
         ranked.minted_delta AS minted_delta,
         ranked.seed AS seed,
         ranked.frame_count AS frame_count,
         ranked.final_rng_state AS final_rng_state,
         ranked.tape_checksum AS tape_checksum,
         ranked.rules_digest AS rules_digest,
         ranked.closed_at AS completed_at,
         ranked.tx_hash AS claim_tx_hash,
         p.username AS profile_username,
         p.link_url AS profile_link_url,
         p.updated_at AS profile_updated_at
       FROM ranked
       LEFT JOIN leaderboard_profiles AS p
         ON p.claimant_address = ranked.claimant_address
       ORDER BY ranked.rank ASC
       LIMIT ? OFFSET ?`,
    )
    .bind(...whereParams, options.limit, options.offset)
    .all<Record<string, unknown>>();

  const entries = (entriesRows.results ?? []).map(mapRankedEntry);

  let me: ReturnType<typeof mapRankedEntry> | null = null;
  if (options.claimantAddress) {
    const meRow = await db
      .prepare(
        `${rankedQueryCteSql(whereClause)}
         SELECT
           ranked.rank AS rank,
           ranked.event_id AS job_id,
           ranked.claimant_address AS claimant_address,
           ranked.new_best AS score,
           ranked.minted_delta AS minted_delta,
           ranked.seed AS seed,
           ranked.frame_count AS frame_count,
           ranked.final_rng_state AS final_rng_state,
           ranked.tape_checksum AS tape_checksum,
           ranked.rules_digest AS rules_digest,
           ranked.closed_at AS completed_at,
           ranked.tx_hash AS claim_tx_hash,
           p.username AS profile_username,
           p.link_url AS profile_link_url,
           p.updated_at AS profile_updated_at
         FROM ranked
         LEFT JOIN leaderboard_profiles AS p
           ON p.claimant_address = ranked.claimant_address
         WHERE ranked.claimant_address = ?
         LIMIT 1`,
      )
      .bind(...whereParams, options.claimantAddress)
      .first<Record<string, unknown>>();
    me = meRow ? mapRankedEntry(meRow) : null;
  }

  const nextOffset =
    options.offset + options.limit < totalPlayers ? options.offset + options.limit : null;

  return {
    window: options.window,
    generatedAt: new Date(nowMs).toISOString(),
    windowRange: getWindowRange(options.window, nowMs),
    totalPlayers,
    limit: options.limit,
    offset: options.offset,
    nextOffset,
    entries,
    me,
  };
}

async function getClaimantRankForWindow(
  env: WorkerEnv,
  claimantAddress: string,
  window: LeaderboardWindow,
  nowMs: number,
): Promise<number | null> {
  await ensureSchema(env);
  const db = getDb(env);
  const whereClause = windowWhereClause(window);
  const whereParams = windowParams(window, nowMs);

  const row = await db
    .prepare(
      `${rankedQueryCteSql(whereClause)}
       SELECT rank
       FROM ranked
       WHERE claimant_address = ?
       LIMIT 1`,
    )
    .bind(...whereParams, claimantAddress)
    .first<Record<string, unknown>>();

  if (!row) {
    return null;
  }
  return toNumber(row.rank, 0) || null;
}

export async function getLeaderboardPlayer(
  env: WorkerEnv,
  claimantAddress: string,
): Promise<{
  profile: PlayerProfileRecord | null;
  stats: {
    totalRuns: number;
    bestScore: number;
    totalMinted: number;
    lastPlayedAt: string | null;
  };
  ranks: {
    tenMin: number | null;
    day: number | null;
    all: number | null;
  };
  recentRuns: Array<{
    jobId: string;
    claimantAddress: string;
    score: number;
    mintedDelta: number;
    seed: number;
    frameCount: number | null;
    finalRngState: number | null;
    tapeChecksum: number | null;
    rulesDigest: number | null;
    completedAt: string;
    claimStatus: "succeeded";
    claimTxHash: string | null;
  }>;
}> {
  await ensureSchema(env);
  const db = getDb(env);
  const nowMs = Date.now();

  const [profileRow, statsRow, recentRows, rank10m, rankDay, rankAll] = await Promise.all([
    db
      .prepare(
        `SELECT claimant_address, username, link_url, updated_at
         FROM leaderboard_profiles
         WHERE claimant_address = ?
         LIMIT 1`,
      )
      .bind(claimantAddress)
      .first<Record<string, unknown>>(),
    db
      .prepare(
        `SELECT
           COUNT(*) AS total_runs,
           COALESCE(MAX(new_best), 0) AS best_score,
           COALESCE(SUM(minted_delta), 0) AS total_minted,
           MAX(closed_at) AS last_played_at
         FROM leaderboard_events
         WHERE claimant_address = ?`,
      )
      .bind(claimantAddress)
      .first<Record<string, unknown>>(),
    db
      .prepare(
        `SELECT
           event_id AS job_id,
           claimant_address,
           new_best AS score,
           minted_delta AS minted_delta,
           seed,
           frame_count AS frame_count,
           final_rng_state AS final_rng_state,
           tape_checksum AS tape_checksum,
           rules_digest AS rules_digest,
           closed_at AS completed_at,
           tx_hash AS claim_tx_hash
         FROM leaderboard_events
         WHERE claimant_address = ?
         ORDER BY closed_at DESC, new_best DESC, event_id ASC
         LIMIT 25`,
      )
      .bind(claimantAddress)
      .all<Record<string, unknown>>(),
    getClaimantRankForWindow(env, claimantAddress, "10m", nowMs),
    getClaimantRankForWindow(env, claimantAddress, "day", nowMs),
    getClaimantRankForWindow(env, claimantAddress, "all", nowMs),
  ]);

  const profile = profileRow
    ? {
        claimantAddress,
        username: toNullableString(profileRow.username),
        linkUrl: toNullableString(profileRow.link_url),
        updatedAt: String(profileRow.updated_at ?? new Date(0).toISOString()),
      }
    : null;

  const recentRuns = (recentRows.results ?? []).map((row) => ({
    jobId: String(row.job_id),
    claimantAddress: String(row.claimant_address),
    score: toNumber(row.score, 0),
    mintedDelta: toNumber(row.minted_delta, 0),
    seed: toNumber(row.seed, 0) >>> 0,
    frameCount: toNullableU32(row.frame_count),
    finalRngState: toNullableU32(row.final_rng_state),
    tapeChecksum: toNullableU32(row.tape_checksum),
    rulesDigest: toNullableU32(row.rules_digest),
    completedAt: String(row.completed_at),
    claimStatus: "succeeded" as const,
    claimTxHash: toNullableString(row.claim_tx_hash),
  }));

  return {
    profile,
    stats: {
      totalRuns: toNumber(statsRow?.total_runs, 0),
      bestScore: toNumber(statsRow?.best_score, 0),
      totalMinted: toNumber(statsRow?.total_minted, 0),
      lastPlayedAt: toNullableString(statsRow?.last_played_at),
    },
    ranks: {
      tenMin: rank10m,
      day: rankDay,
      all: rankAll,
    },
    recentRuns,
  };
}
