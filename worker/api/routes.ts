import { Hono } from "hono";
import {
  generateAuthenticationOptions,
  verifyAuthenticationResponse,
  type AuthenticationResponseJSON,
  type AuthenticatorTransportFuture,
} from "@simplewebauthn/server";
import { DEFAULT_MAX_TAPE_BYTES, EXPECTED_RULES_DIGEST, EXPECTED_RULESET } from "../constants";
import { asPublicJob, coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { resultKey } from "../keys";
import { recordLeaderboardSyncFailure, runLeaderboardSync } from "../leaderboard-sync";
import { parseLeaderboardSourceMode } from "../leaderboard-ingestion";
import {
  DEFAULT_LEADERBOARD_LIMIT,
  MAX_LEADERBOARD_LIMIT,
  MAX_LEADERBOARD_OFFSET,
  parseLeaderboardWindow,
} from "../leaderboard";
import {
  countLeaderboardEvents,
  createLeaderboardProfileAuthChallenge,
  getLeaderboardIngestionState,
  getLeaderboardPage,
  getLeaderboardPlayer,
  getLeaderboardProfileAuthChallenge,
  getLeaderboardProfileCredential,
  markLeaderboardProfileAuthChallengeUsed,
  purgeExpiredLeaderboardProfileAuthChallenges,
  setLeaderboardIngestionState,
  updateLeaderboardProfileCredentialCounter,
  upsertLeaderboardEvents,
  upsertLeaderboardProfileCredential,
  upsertLeaderboardProfile,
  upsertLeaderboardProfiles,
} from "../leaderboard-store";
import {
  DEFAULT_SMART_ACCOUNT_INDEXER_TIMEOUT_MS,
  LeaderboardCredentialBindingError,
  assertCredentialBelongsToClaimantContract,
  encodeRawP256PublicKeyBase64UrlToCose,
  normalizeAuthenticatorTransports,
} from "../leaderboard-profile-auth";
import { describeProverHealthError, getValidatedProverHealth } from "../prover/client";
import { parseAndValidateTape } from "../tape";
import { isTerminalProofStatus, parseInteger, safeErrorMessage } from "../utils";
import { validateClaimantStrKeyFromUserInput } from "../../shared/stellar/strkey";
import type { LeaderboardResolvedSourceMode } from "../leaderboard-ingestion";
import { LEADERBOARD_CACHE_CONTROL, LEADERBOARD_PRIVATE_CACHE_CONTROL } from "../cache-control";

const LEADERBOARD_VIEW_CACHE_TTL_MS = 5_000;
const LEADERBOARD_VIEW_CACHE_MAX_ENTRIES = 256;
const LEADERBOARD_ROLLING_CACHE_BUCKET_MS = 5_000;
const LEADERBOARD_RESPONSE_SCHEMA_VERSION = "2026-02-14.2";
const LEADERBOARD_PROFILE_AUTH_CHALLENGE_TTL_MS = 120_000;
const LEADERBOARD_PROFILE_AUTH_CHALLENGE_TTL_MAX_MS = 600_000;
const LEADERBOARD_PROFILE_AUTH_MIN_INTERVAL_MS = 1_500;
const LEADERBOARD_PROFILE_AUTH_RATE_LIMIT_ENTRIES = 1024;
const textEncoder = new TextEncoder();

interface LeaderboardViewCacheEntry {
  payload: unknown;
  expiresAtMs: number;
}

const leaderboardViewCache = new Map<string, LeaderboardViewCacheEntry>();
const profileAuthLastAttemptByKey = new Map<string, number>();

class PayloadTooLargeError extends Error {
  readonly sizeBytes: number;
  readonly maxBytes: number;

  constructor(sizeBytes: number, maxBytes: number) {
    super(`tape payload too large: ${sizeBytes} bytes (max ${maxBytes})`);
    this.name = "PayloadTooLargeError";
    this.sizeBytes = sizeBytes;
    this.maxBytes = maxBytes;
  }
}

function parseContentLength(value: string | undefined): number | null {
  if (!value) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return null;
  }

  return parsed;
}

function parseOffset(raw: string | undefined): number {
  const parsed = Number.parseInt(raw ?? "0", 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return 0;
  }
  return Math.min(parsed, MAX_LEADERBOARD_OFFSET);
}

function parseLimit(raw: string | undefined): number {
  const parsed = Number.parseInt(raw ?? `${DEFAULT_LEADERBOARD_LIMIT}`, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return DEFAULT_LEADERBOARD_LIMIT;
  }
  return Math.min(parsed, MAX_LEADERBOARD_LIMIT);
}

function parseOptionalNonNegativeInteger(raw: unknown): number | null {
  if (raw === null || raw === undefined || raw === "") {
    return null;
  }

  const parsed = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return null;
  }
  return parsed;
}

function parseSyncLimit(raw: unknown): number | undefined {
  const parsed = parseOptionalNonNegativeInteger(raw);
  if (parsed === null) {
    return undefined;
  }
  return Math.min(Math.max(parsed, 1), 1000);
}

function parseMigrationChunkSize(raw: string | undefined): number {
  const parsed = Number.parseInt(raw ?? "500", 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 500;
  }
  return Math.min(Math.max(parsed, 50), 2000);
}

function parseSyncSource(raw: unknown): LeaderboardResolvedSourceMode | "default" | null {
  if (raw === null || raw === undefined || raw === "") {
    return null;
  }

  const normalized = String(raw).trim().toLowerCase();
  if (normalized === "auto" || normalized === "default") {
    return "default";
  }
  if (normalized === "rpc" || normalized === "events_api" || normalized === "datalake") {
    return normalized;
  }

  return null;
}

function normalizeOptionalClaimantAddress(raw: string | undefined): string | null {
  if (!raw || raw.trim().length === 0) {
    return null;
  }

  return validateClaimantStrKeyFromUserInput(raw);
}

function constantTimeEqual(left: string, right: string): boolean {
  const leftBytes = textEncoder.encode(left);
  const rightBytes = textEncoder.encode(right);
  const maxLength = Math.max(leftBytes.length, rightBytes.length);
  let diff = leftBytes.length ^ rightBytes.length;
  for (let index = 0; index < maxLength; index += 1) {
    diff |= (leftBytes[index] ?? 0) ^ (rightBytes[index] ?? 0);
  }
  return diff === 0;
}

function requireLeaderboardAdmin(c: {
  req: { header: (name: string) => string | undefined };
  env: WorkerEnv;
}): string {
  const configured = c.env.LEADERBOARD_ADMIN_KEY?.trim();
  if (!configured) {
    throw new Error("leaderboard admin key is not configured");
  }

  const provided = c.req.header("x-leaderboard-admin-key")?.trim() ?? "";
  if (provided.length === 0) {
    throw new Error("x-leaderboard-admin-key is required");
  }
  if (!constantTimeEqual(provided, configured)) {
    throw new Error("invalid x-leaderboard-admin-key");
  }

  return provided;
}

function sanitizeProfileUsername(raw: unknown): string | null {
  if (raw === null || raw === undefined) {
    return null;
  }

  if (typeof raw !== "string") {
    throw new Error("username must be a string");
  }

  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return null;
  }
  if (trimmed.length > 32) {
    throw new Error("username must be 32 characters or fewer");
  }

  return trimmed;
}

function sanitizeProfileLinkUrl(raw: unknown): string | null {
  if (raw === null || raw === undefined) {
    return null;
  }

  if (typeof raw !== "string") {
    throw new Error("link_url must be a string");
  }

  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return null;
  }
  if (trimmed.length > 240) {
    throw new Error("link_url must be 240 characters or fewer");
  }

  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error("link_url must be a valid absolute URL");
  }

  if (url.protocol !== "https:" && url.protocol !== "http:") {
    throw new Error("link_url must use http or https");
  }

  return url.toString();
}

function parseProfileAuthChallengeTtlMs(raw: string | undefined): number {
  const parsed = parseInteger(raw, LEADERBOARD_PROFILE_AUTH_CHALLENGE_TTL_MS, 10_000);
  return Math.min(parsed, LEADERBOARD_PROFILE_AUTH_CHALLENGE_TTL_MAX_MS);
}

function parseIndexerTimeoutMs(raw: string | undefined): number {
  return parseInteger(raw, DEFAULT_SMART_ACCOUNT_INDEXER_TIMEOUT_MS, 1_000);
}

function ensureObject(raw: unknown, errorMessage: string): Record<string, unknown> {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error(errorMessage);
  }
  return raw as Record<string, unknown>;
}

function parseRequiredString(
  payload: Record<string, unknown>,
  key: string,
  message: string,
): string {
  const value = payload[key];
  if (typeof value !== "string") {
    throw new Error(message);
  }

  const normalized = value.trim();
  if (normalized.length === 0) {
    throw new Error(message);
  }
  return normalized;
}

function parseOptionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : undefined;
}

function parseOptionalAuthenticatorAttachment(
  value: unknown,
): AuthenticationResponseJSON["authenticatorAttachment"] {
  if (value === null || value === undefined) {
    return undefined;
  }
  if (value === "platform" || value === "cross-platform") {
    return value;
  }
  throw new Error(
    'auth.response.authenticatorAttachment must be "platform" or "cross-platform" when provided',
  );
}

function parseProfileAuthOptionsPayload(payload: Record<string, unknown>): {
  credentialId: string;
  credentialPublicKey: string;
  transports: AuthenticatorTransportFuture[] | null;
} {
  const credentialId = parseRequiredString(payload, "credential_id", "credential_id is required");
  const credentialPublicKey = parseRequiredString(
    payload,
    "credential_public_key",
    "credential_public_key is required",
  );
  const transports = normalizeAuthenticatorTransports(payload.transports);
  return {
    credentialId,
    credentialPublicKey,
    transports,
  };
}

function parseAuthenticationResponsePayload(raw: unknown): AuthenticationResponseJSON {
  const payload = ensureObject(raw, "auth.response must be an object");
  const responsePayload = ensureObject(
    payload.response,
    "auth.response.response must be an object",
  );

  const id = parseRequiredString(payload, "id", "auth.response.id is required");
  const rawId = parseRequiredString(payload, "rawId", "auth.response.rawId is required");
  if (id !== rawId) {
    throw new Error("auth.response.id must equal auth.response.rawId");
  }

  const type = parseRequiredString(payload, "type", "auth.response.type is required");
  if (type !== "public-key") {
    throw new Error('auth.response.type must be "public-key"');
  }

  const clientDataJSON = parseRequiredString(
    responsePayload,
    "clientDataJSON",
    "auth.response.response.clientDataJSON is required",
  );
  const authenticatorData = parseRequiredString(
    responsePayload,
    "authenticatorData",
    "auth.response.response.authenticatorData is required",
  );
  const signature = parseRequiredString(
    responsePayload,
    "signature",
    "auth.response.response.signature is required",
  );

  const clientExtensionResults = ensureObject(
    payload.clientExtensionResults,
    "auth.response.clientExtensionResults must be an object",
  );

  const userHandleRaw = responsePayload.userHandle;
  if (userHandleRaw !== undefined && userHandleRaw !== null && typeof userHandleRaw !== "string") {
    throw new Error("auth.response.response.userHandle must be a string when provided");
  }

  const authenticatorAttachment = parseOptionalAuthenticatorAttachment(
    payload.authenticatorAttachment,
  );

  return {
    id,
    rawId,
    type: "public-key",
    response: {
      clientDataJSON,
      authenticatorData,
      signature,
      userHandle: parseOptionalString(userHandleRaw),
    },
    clientExtensionResults,
    authenticatorAttachment,
  };
}

function parseProfileAuthAssertionPayload(payload: Record<string, unknown>): {
  challengeId: string;
  response: AuthenticationResponseJSON;
} {
  const authPayload = ensureObject(payload.auth, "auth payload must be an object");
  const challengeId = parseRequiredString(
    authPayload,
    "challenge_id",
    "auth.challenge_id is required",
  );
  const response = parseAuthenticationResponsePayload(authPayload.response);
  return {
    challengeId,
    response,
  };
}

function resolveProfileExpectedOrigin(c: { req: { url: string }; env: WorkerEnv }): string {
  const configured = c.env.LEADERBOARD_PROFILE_WEBAUTHN_ORIGIN?.trim();
  if (!configured) {
    return new URL(c.req.url).origin;
  }

  try {
    return new URL(configured).origin;
  } catch (error) {
    throw new Error(`invalid LEADERBOARD_PROFILE_WEBAUTHN_ORIGIN: ${safeErrorMessage(error)}`, {
      cause: error,
    });
  }
}

function resolveProfileExpectedRpId(c: { req: { url: string }; env: WorkerEnv }): string {
  const configured = c.env.LEADERBOARD_PROFILE_WEBAUTHN_RP_ID?.trim();
  if (configured && configured.length > 0) {
    return configured;
  }
  return new URL(c.req.url).hostname;
}

async function readRequestBodyWithLimit(
  request: Request,
  maxTapeBytes: number,
): Promise<Uint8Array> {
  const reader = request.body?.getReader();
  if (!reader) {
    return new Uint8Array();
  }

  const chunks: Uint8Array[] = [];
  let totalSize = 0;

  try {
    /* eslint-disable no-await-in-loop */
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }

      if (!value || value.byteLength === 0) {
        continue;
      }

      totalSize += value.byteLength;
      if (totalSize > maxTapeBytes) {
        void reader.cancel("payload too large");
        throw new PayloadTooLargeError(totalSize, maxTapeBytes);
      }
      chunks.push(value);
    }
    /* eslint-enable no-await-in-loop */
  } finally {
    reader.releaseLock();
  }

  const body = new Uint8Array(totalSize);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }

  return body;
}

function jsonError(
  c: { json: (body: unknown, status?: number) => Response },
  status: number,
  error: string,
): Response {
  return c.json(
    {
      success: false,
      error,
    },
    status,
  );
}

function weakHashHex(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function leaderboardEtag(cacheKey: string): string {
  return `W/"${weakHashHex(cacheKey)}"`;
}

function rollingCacheBucket(nowMs: number): string {
  return `${Math.floor(nowMs / LEADERBOARD_ROLLING_CACHE_BUCKET_MS)}`;
}

function readLeaderboardViewCache(cacheKey: string): unknown | null {
  const current = leaderboardViewCache.get(cacheKey);
  if (!current) {
    return null;
  }

  if (Date.now() > current.expiresAtMs) {
    leaderboardViewCache.delete(cacheKey);
    return null;
  }

  return current.payload;
}

function writeLeaderboardViewCache(cacheKey: string, payload: unknown): void {
  if (leaderboardViewCache.size >= LEADERBOARD_VIEW_CACHE_MAX_ENTRIES) {
    const oldestKey = leaderboardViewCache.keys().next().value;
    if (typeof oldestKey === "string") {
      leaderboardViewCache.delete(oldestKey);
    }
  }

  leaderboardViewCache.set(cacheKey, {
    payload,
    expiresAtMs: Date.now() + LEADERBOARD_VIEW_CACHE_TTL_MS,
  });
}

function isRateLimitedProfileAuthAttempt(throttleKey: string, nowMs: number): boolean {
  const lastAttemptMs = profileAuthLastAttemptByKey.get(throttleKey) ?? 0;
  if (lastAttemptMs > 0 && nowMs - lastAttemptMs < LEADERBOARD_PROFILE_AUTH_MIN_INTERVAL_MS) {
    return true;
  }

  if (profileAuthLastAttemptByKey.size >= LEADERBOARD_PROFILE_AUTH_RATE_LIMIT_ENTRIES) {
    const oldestKey = profileAuthLastAttemptByKey.keys().next().value;
    if (typeof oldestKey === "string") {
      profileAuthLastAttemptByKey.delete(oldestKey);
    }
  }
  profileAuthLastAttemptByKey.set(throttleKey, nowMs);
  return false;
}

function respondWithLeaderboardCaching(
  c: {
    req: { header: (name: string) => string | undefined };
    json: (body: unknown, status?: number) => Response;
  },
  payload: unknown,
  {
    cacheKey,
    generatedAt,
    privateScope,
  }: {
    cacheKey: string;
    generatedAt: string | null | undefined;
    privateScope: boolean;
  },
): Response {
  const etag = leaderboardEtag(cacheKey);
  const ifNoneMatch = c.req.header("if-none-match");
  if (ifNoneMatch && ifNoneMatch === etag) {
    const response = new Response(null, { status: 304 });
    response.headers.set(
      "cache-control",
      privateScope ? LEADERBOARD_PRIVATE_CACHE_CONTROL : LEADERBOARD_CACHE_CONTROL,
    );
    response.headers.set("etag", etag);
    if (generatedAt) {
      response.headers.set("last-modified", generatedAt);
    }
    return response;
  }

  const response = c.json(payload);
  response.headers.set(
    "cache-control",
    privateScope ? LEADERBOARD_PRIVATE_CACHE_CONTROL : LEADERBOARD_CACHE_CONTROL,
  );
  response.headers.set("etag", etag);
  if (generatedAt) {
    response.headers.set("last-modified", generatedAt);
  }
  return response;
}

export function createApiRouter(): Hono<{ Bindings: WorkerEnv }> {
  const api = new Hono<{ Bindings: WorkerEnv }>();

  api.get("/health", async (c) => {
    const coordinator = coordinatorStub(c.env);
    const activeJob = await coordinator.getActiveJob();
    const expectedImageIdRaw = c.env.PROVER_EXPECTED_IMAGE_ID?.trim() ?? "";
    const expectedImageId = expectedImageIdRaw.length > 0 ? expectedImageIdRaw : null;

    let prover:
      | {
          status: "compatible";
          image_id: string;
          rules_digest_hex: string;
          ruleset: string;
        }
      | {
          status: "degraded";
          error: string;
        };

    try {
      const health = await getValidatedProverHealth(c.env);
      prover = {
        status: "compatible",
        image_id: health.imageId,
        rules_digest_hex: health.rulesDigestHex,
        ruleset: health.ruleset,
      };
    } catch (error) {
      const healthError = describeProverHealthError(error);
      prover = {
        status: "degraded",
        error: healthError.message,
      };
    }

    return c.json({
      success: true,
      service: "stellar-zk-proof-gateway",
      mode: "single-active-job",
      expected: {
        rules_digest_hex: `0x${(EXPECTED_RULES_DIGEST >>> 0).toString(16).padStart(8, "0")}`,
        ruleset: EXPECTED_RULESET,
        image_id: expectedImageId,
      },
      checked_at: new Date().toISOString(),
      prover,
      active_job: activeJob ? asPublicJob(activeJob) : null,
    });
  });

  api.get("/leaderboard/sync/status", async (c) => {
    try {
      requireLeaderboardAdmin(c);
    } catch (error) {
      return jsonError(c, 401, safeErrorMessage(error));
    }

    const [state, eventCount] = await Promise.all([
      getLeaderboardIngestionState(c.env),
      countLeaderboardEvents(c.env),
    ]);

    return c.json({
      success: true,
      source: "events",
      provider: state.provider,
      provider_mode: parseLeaderboardSourceMode(c.env),
      source_mode: state.sourceMode,
      event_count: eventCount,
      state: {
        ...state,
        totalEvents: eventCount,
      },
    });
  });

  api.post("/leaderboard/sync", async (c) => {
    try {
      requireLeaderboardAdmin(c);
    } catch (error) {
      return jsonError(c, 401, safeErrorMessage(error));
    }

    let payload: Record<string, unknown>;
    try {
      const body = await c.req.json();
      if (!body || typeof body !== "object" || Array.isArray(body)) {
        return jsonError(c, 400, "sync payload must be an object");
      }
      payload = body as Record<string, unknown>;
    } catch (error) {
      return jsonError(c, 400, `invalid JSON payload: ${safeErrorMessage(error)}`);
    }

    const modeRaw =
      typeof payload.mode === "string" ? payload.mode.trim().toLowerCase() : "forward";
    const mode = modeRaw === "backfill" ? "backfill" : modeRaw === "forward" ? "forward" : null;
    if (!mode) {
      return jsonError(c, 400, "mode must be one of: forward, backfill");
    }

    const cursor =
      typeof payload.cursor === "string" && payload.cursor.trim().length > 0
        ? payload.cursor.trim()
        : null;
    const fromLedger = parseOptionalNonNegativeInteger(payload.from_ledger ?? payload.fromLedger);
    const toLedger = parseOptionalNonNegativeInteger(payload.to_ledger ?? payload.toLedger);
    const limit = parseSyncLimit(payload.limit);
    const sourceRaw = parseSyncSource(payload.source);

    if (payload.source !== undefined && payload.source !== null && sourceRaw === null) {
      return jsonError(c, 400, "source must be one of: auto, rpc, datalake, events_api");
    }
    const source = sourceRaw && sourceRaw !== "default" ? sourceRaw : null;

    if (mode === "backfill" && fromLedger === null) {
      return jsonError(c, 400, "backfill mode requires from_ledger");
    }
    if (fromLedger !== null && toLedger !== null && fromLedger > toLedger) {
      return jsonError(c, 400, "from_ledger must be <= to_ledger");
    }

    try {
      const result = await runLeaderboardSync(c.env, {
        mode,
        cursor,
        fromLedger,
        toLedger,
        limit,
        source,
      });

      return c.json({
        success: true,
        source: "events",
        provider_mode: parseLeaderboardSourceMode(c.env),
        ...result,
      });
    } catch (error) {
      await recordLeaderboardSyncFailure(c.env, error);
      return jsonError(c, 502, `leaderboard sync failed: ${safeErrorMessage(error)}`);
    }
  });

  api.post("/leaderboard/migrate/do-to-d1", async (c) => {
    try {
      requireLeaderboardAdmin(c);
    } catch (error) {
      return jsonError(c, 401, safeErrorMessage(error));
    }

    const migrationConfirm = c.req.header("x-migration-confirm")?.trim().toLowerCase();
    if (migrationConfirm !== "do-to-d1") {
      return jsonError(
        c,
        400,
        "x-migration-confirm header must be set to do-to-d1 for this operation",
      );
    }

    const coordinator = coordinatorStub(c.env);
    const chunkSize = parseMigrationChunkSize(c.req.query("chunk_size"));

    try {
      const state = await coordinator.getLeaderboardIngestionState();
      let eventsRead = 0;
      let profilesRead = 0;
      let eventsInserted = 0;
      let eventsUpdated = 0;
      let eventCursor: string | null = null;
      let profileCursor: string | null = null;

      /* eslint-disable no-await-in-loop */
      while (true) {
        const page = await coordinator.listLeaderboardEventsPage({
          startAfter: eventCursor,
          limit: chunkSize,
        });
        if (page.events.length === 0) {
          break;
        }

        const upsert = await upsertLeaderboardEvents(c.env, page.events);
        eventsRead += page.events.length;
        eventsInserted += upsert.inserted;
        eventsUpdated += upsert.updated;

        if (!page.nextStartAfter || page.done) {
          break;
        }
        eventCursor = page.nextStartAfter;
      }

      while (true) {
        const page = await coordinator.listLeaderboardProfilesPage({
          startAfter: profileCursor,
          limit: chunkSize,
        });
        if (page.profiles.length === 0) {
          break;
        }

        await upsertLeaderboardProfiles(c.env, page.profiles);
        profilesRead += page.profiles.length;

        if (!page.nextStartAfter || page.done) {
          break;
        }
        profileCursor = page.nextStartAfter;
      }
      /* eslint-enable no-await-in-loop */

      const totalEvents = await countLeaderboardEvents(c.env);
      await setLeaderboardIngestionState(c.env, {
        ...state,
        totalEvents,
      });
      leaderboardViewCache.clear();

      return c.json({
        success: true,
        source: "events",
        migrated: {
          chunk_size: chunkSize,
          events_read: eventsRead,
          profiles_read: profilesRead,
          events_inserted: eventsInserted,
          events_updated: eventsUpdated,
          total_events: totalEvents,
        },
      });
    } catch (error) {
      return jsonError(
        c,
        500,
        `failed migrating leaderboard DO data to D1: ${safeErrorMessage(error)}`,
      );
    }
  });

  api.get("/leaderboard", async (c) => {
    const window = parseLeaderboardWindow(c.req.query("window"));
    if (!window) {
      return jsonError(c, 400, "window must be one of: 10m, day, all");
    }

    const limit = parseLimit(c.req.query("limit"));
    const offset = parseOffset(c.req.query("offset"));
    const nowMs = Date.now();
    const rollingBucket = window === "all" ? "" : rollingCacheBucket(nowMs);

    let claimantAddress: string | null;
    try {
      claimantAddress = normalizeOptionalClaimantAddress(c.req.query("address"));
    } catch (error) {
      return jsonError(c, 400, `invalid address: ${safeErrorMessage(error)}`);
    }

    const ingestionState = await getLeaderboardIngestionState(c.env);
    const cacheKey = [
      LEADERBOARD_RESPONSE_SCHEMA_VERSION,
      "leaderboard",
      window,
      limit,
      offset,
      rollingBucket,
      claimantAddress ?? "",
      ingestionState.provider,
      ingestionState.sourceMode,
      ingestionState.totalEvents,
      ingestionState.highestLedger ?? "",
      ingestionState.lastSyncedAt ?? "",
    ].join("|");
    const cachedPayload = readLeaderboardViewCache(cacheKey);
    if (cachedPayload) {
      return respondWithLeaderboardCaching(c, cachedPayload, {
        cacheKey,
        generatedAt: ingestionState.lastSyncedAt,
        privateScope: claimantAddress !== null,
      });
    }

    const computed = await getLeaderboardPage(c.env, {
      window,
      limit,
      offset,
      claimantAddress,
      nowMs,
    });

    const payload = {
      success: true,
      source: "events",
      provider: ingestionState.provider,
      provider_mode: parseLeaderboardSourceMode(c.env),
      source_mode: ingestionState.sourceMode,
      window: computed.window,
      generated_at: computed.generatedAt,
      window_range: {
        start_at: computed.windowRange.startAt,
        end_at: computed.windowRange.endAt,
      },
      pagination: {
        limit: computed.limit,
        offset: computed.offset,
        total: computed.totalPlayers,
        next_offset: computed.nextOffset,
      },
      entries: computed.entries,
      me: computed.me,
      ingestion: {
        last_synced_at: ingestionState.lastSyncedAt,
        highest_ledger: ingestionState.highestLedger,
        total_events: ingestionState.totalEvents,
      },
    };

    writeLeaderboardViewCache(cacheKey, payload);
    return respondWithLeaderboardCaching(c, payload, {
      cacheKey,
      generatedAt: computed.generatedAt,
      privateScope: claimantAddress !== null,
    });
  });

  api.get("/leaderboard/player/:claimantAddress", async (c) => {
    const rawClaimantAddress = c.req.param("claimantAddress");
    let claimantAddress: string;
    try {
      claimantAddress = validateClaimantStrKeyFromUserInput(rawClaimantAddress);
    } catch (error) {
      return jsonError(c, 400, `invalid claimant address: ${safeErrorMessage(error)}`);
    }

    const ingestionState = await getLeaderboardIngestionState(c.env);
    const rollingBucket = rollingCacheBucket(Date.now());
    const cacheKey = [
      LEADERBOARD_RESPONSE_SCHEMA_VERSION,
      "player",
      claimantAddress,
      rollingBucket,
      ingestionState.provider,
      ingestionState.sourceMode,
      ingestionState.totalEvents,
      ingestionState.highestLedger ?? "",
      ingestionState.lastSyncedAt ?? "",
    ].join("|");
    const cachedPayload = readLeaderboardViewCache(cacheKey);
    if (cachedPayload) {
      return respondWithLeaderboardCaching(c, cachedPayload, {
        cacheKey,
        generatedAt: ingestionState.lastSyncedAt,
        privateScope: false,
      });
    }

    const player = await getLeaderboardPlayer(c.env, claimantAddress);

    const payload = {
      success: true,
      player: {
        claimant_address: claimantAddress,
        profile: player.profile,
        stats: {
          total_runs: player.stats.totalRuns,
          best_score: player.stats.bestScore,
          total_minted: player.stats.totalMinted,
          last_played_at: player.stats.lastPlayedAt,
        },
        ranks: {
          ten_min: player.ranks.tenMin,
          day: player.ranks.day,
          all: player.ranks.all,
        },
        recent_runs: player.recentRuns,
      },
    };

    writeLeaderboardViewCache(cacheKey, payload);
    return respondWithLeaderboardCaching(c, payload, {
      cacheKey,
      generatedAt: player.stats.lastPlayedAt ?? ingestionState.lastSyncedAt,
      privateScope: false,
    });
  });

  api.post("/leaderboard/player/:claimantAddress/profile/auth/options", async (c) => {
    const rawClaimantAddress = c.req.param("claimantAddress");
    let claimantAddress: string;
    try {
      claimantAddress = validateClaimantStrKeyFromUserInput(rawClaimantAddress);
    } catch (error) {
      return jsonError(c, 400, `invalid claimant address: ${safeErrorMessage(error)}`);
    }

    let payload: Record<string, unknown>;
    try {
      payload = ensureObject(await c.req.json(), "profile auth payload must be an object");
    } catch (error) {
      return jsonError(c, 400, `invalid JSON payload: ${safeErrorMessage(error)}`);
    }

    let authInput: {
      credentialId: string;
      credentialPublicKey: string;
      transports: AuthenticatorTransportFuture[] | null;
    };
    try {
      authInput = parseProfileAuthOptionsPayload(payload);
      // Ensure public key has the expected shape before storing it.
      encodeRawP256PublicKeyBase64UrlToCose(authInput.credentialPublicKey);
    } catch (error) {
      return jsonError(c, 400, safeErrorMessage(error));
    }

    const throttleKey = `${claimantAddress}|${authInput.credentialId}`;
    if (isRateLimitedProfileAuthAttempt(throttleKey, Date.now())) {
      return jsonError(c, 429, "too many auth option requests; retry shortly");
    }

    const existingCredential = await getLeaderboardProfileCredential(c.env, authInput.credentialId);
    if (!existingCredential) {
      try {
        await assertCredentialBelongsToClaimantContract({
          claimantAddress,
          credentialIdBase64Url: authInput.credentialId,
          indexerBaseUrl: c.env.SMART_ACCOUNT_INDEXER_URL,
          timeoutMs: parseIndexerTimeoutMs(c.env.SMART_ACCOUNT_INDEXER_TIMEOUT_MS),
        });
      } catch (error) {
        if (error instanceof LeaderboardCredentialBindingError) {
          return jsonError(c, error.statusCode, error.message);
        }
        return jsonError(c, 503, `failed verifying credential binding: ${safeErrorMessage(error)}`);
      }
    } else if (existingCredential.claimantAddress !== claimantAddress) {
      return jsonError(c, 403, "credential is already bound to another claimant address");
    }

    let credentialRecord;
    try {
      credentialRecord = await upsertLeaderboardProfileCredential(c.env, {
        claimantAddress,
        credentialId: authInput.credentialId,
        publicKey: authInput.credentialPublicKey,
        transports: authInput.transports,
      });
    } catch (error) {
      const message = safeErrorMessage(error);
      if (message.includes("already bound")) {
        return jsonError(c, 403, message);
      }
      if (message.includes("public key mismatch")) {
        return jsonError(c, 409, message);
      }
      return jsonError(c, 500, message);
    }

    let expectedOrigin: string;
    let expectedRpId: string;
    try {
      expectedOrigin = resolveProfileExpectedOrigin(c);
      expectedRpId = resolveProfileExpectedRpId(c);
    } catch (error) {
      return jsonError(c, 500, safeErrorMessage(error));
    }

    const challengeTtlMs = parseProfileAuthChallengeTtlMs(
      c.env.LEADERBOARD_PROFILE_AUTH_CHALLENGE_TTL_MS,
    );
    let options: Awaited<ReturnType<typeof generateAuthenticationOptions>>;
    try {
      options = await generateAuthenticationOptions({
        rpID: expectedRpId,
        allowCredentials: [
          {
            id: credentialRecord.credentialId,
            transports:
              (credentialRecord.transports as AuthenticatorTransportFuture[] | null) ?? undefined,
          },
        ],
        timeout: challengeTtlMs,
        userVerification: "required",
      });
    } catch (error) {
      return jsonError(c, 500, `failed generating auth options: ${safeErrorMessage(error)}`);
    }

    const expiresAt = new Date(Date.now() + challengeTtlMs).toISOString();
    const challengeId = crypto.randomUUID();
    await purgeExpiredLeaderboardProfileAuthChallenges(c.env);
    await createLeaderboardProfileAuthChallenge(c.env, {
      challengeId,
      claimantAddress,
      credentialId: credentialRecord.credentialId,
      challenge: options.challenge,
      expectedOrigin,
      expectedRpId,
      expiresAt,
    });

    return c.json({
      success: true,
      auth: {
        challenge_id: challengeId,
        options,
        expires_at: expiresAt,
      },
    });
  });

  api.put("/leaderboard/player/:claimantAddress/profile", async (c) => {
    const rawClaimantAddress = c.req.param("claimantAddress");
    let claimantAddress: string;
    try {
      claimantAddress = validateClaimantStrKeyFromUserInput(rawClaimantAddress);
    } catch (error) {
      return jsonError(c, 400, `invalid claimant address: ${safeErrorMessage(error)}`);
    }

    let payload: Record<string, unknown>;
    try {
      payload = ensureObject(await c.req.json(), "profile payload must be an object");
    } catch (error) {
      return jsonError(c, 400, `invalid JSON payload: ${safeErrorMessage(error)}`);
    }

    let username: string | null;
    let linkUrl: string | null;
    let authAssertion: {
      challengeId: string;
      response: AuthenticationResponseJSON;
    };
    try {
      username = sanitizeProfileUsername(payload.username);
      linkUrl = sanitizeProfileLinkUrl(payload.link_url ?? payload.linkUrl);
      authAssertion = parseProfileAuthAssertionPayload(payload);
    } catch (error) {
      return jsonError(c, 400, safeErrorMessage(error));
    }

    const challenge = await getLeaderboardProfileAuthChallenge(c.env, authAssertion.challengeId);
    if (!challenge) {
      return jsonError(c, 401, "auth challenge not found");
    }
    if (challenge.claimantAddress !== claimantAddress) {
      return jsonError(c, 403, "auth challenge claimant mismatch");
    }
    if (challenge.usedAt) {
      return jsonError(c, 409, "auth challenge already used");
    }

    const expiresAtMs = new Date(challenge.expiresAt).getTime();
    if (!Number.isFinite(expiresAtMs) || expiresAtMs <= Date.now()) {
      return jsonError(c, 401, "auth challenge expired");
    }

    if (authAssertion.response.id !== challenge.credentialId) {
      return jsonError(c, 401, "auth credential mismatch");
    }

    const credential = await getLeaderboardProfileCredential(c.env, challenge.credentialId);
    if (!credential) {
      return jsonError(c, 401, "credential not found");
    }
    if (credential.claimantAddress !== claimantAddress) {
      return jsonError(c, 403, "credential claimant mismatch");
    }

    let credentialPublicKey: Uint8Array;
    try {
      credentialPublicKey = encodeRawP256PublicKeyBase64UrlToCose(credential.publicKey);
    } catch (error) {
      return jsonError(
        c,
        500,
        `stored credential public key is invalid: ${safeErrorMessage(error)}`,
      );
    }

    const markedUsed = await markLeaderboardProfileAuthChallengeUsed(c.env, challenge.challengeId);
    if (!markedUsed) {
      return jsonError(c, 409, "auth challenge already used");
    }

    let verification;
    try {
      verification = await verifyAuthenticationResponse({
        response: authAssertion.response,
        expectedChallenge: challenge.challenge,
        expectedOrigin: challenge.expectedOrigin,
        expectedRPID: challenge.expectedRpId,
        credential: {
          id: credential.credentialId,
          publicKey: credentialPublicKey as Uint8Array<ArrayBuffer>,
          counter: credential.counter,
          transports: (credential.transports as AuthenticatorTransportFuture[] | null) ?? undefined,
        },
        requireUserVerification: true,
      });
    } catch (error) {
      return jsonError(c, 401, `passkey verification failed: ${safeErrorMessage(error)}`);
    }

    if (!verification.verified) {
      return jsonError(c, 401, "passkey verification failed");
    }

    await updateLeaderboardProfileCredentialCounter(
      c.env,
      credential.credentialId,
      verification.authenticationInfo.newCounter,
    );

    const profile = await upsertLeaderboardProfile(c.env, claimantAddress, {
      username,
      linkUrl,
    });
    leaderboardViewCache.clear();
    void purgeExpiredLeaderboardProfileAuthChallenges(c.env);

    return c.json({
      success: true,
      profile,
    });
  });

  api.post("/proofs/jobs", async (c) => {
    const maxTapeBytes = parseInteger(c.env.MAX_TAPE_BYTES, DEFAULT_MAX_TAPE_BYTES, 1);
    const declaredLength = parseContentLength(c.req.header("content-length"));
    if (declaredLength !== null && declaredLength > maxTapeBytes) {
      return jsonError(
        c,
        413,
        `tape payload too large: ${declaredLength} bytes (max ${maxTapeBytes})`,
      );
    }

    let tapeBytes: Uint8Array;
    try {
      tapeBytes = await readRequestBodyWithLimit(c.req.raw, maxTapeBytes);
    } catch (error) {
      if (error instanceof PayloadTooLargeError) {
        return jsonError(c, 413, error.message);
      }
      return jsonError(c, 400, `failed reading request body: ${safeErrorMessage(error)}`);
    }

    let metadata;
    try {
      metadata = parseAndValidateTape(tapeBytes, maxTapeBytes);
    } catch (error) {
      return jsonError(c, 400, safeErrorMessage(error));
    }

    const rawClaimant = c.req.header("x-claimant-address") ?? "";
    let claimantAddress: string;
    try {
      claimantAddress = validateClaimantStrKeyFromUserInput(rawClaimant);
    } catch (error) {
      return jsonError(c, 400, `invalid x-claimant-address: ${safeErrorMessage(error)}`);
    }

    const coordinator = coordinatorStub(c.env);
    const createResult = await coordinator.createJob({
      sizeBytes: tapeBytes.byteLength,
      metadata,
      claimantAddress,
    });

    if (!createResult.accepted) {
      return c.json(
        {
          success: false,
          error: "proof queue is currently busy; retry when the active job completes",
          active_job: asPublicJob(createResult.activeJob),
        },
        429,
      );
    }

    const { job } = createResult;

    try {
      await c.env.PROOF_ARTIFACTS.put(job.tape.key, tapeBytes, {
        httpMetadata: {
          contentType: "application/octet-stream",
        },
        customMetadata: {
          jobId: job.jobId,
        },
      });
    } catch (error) {
      await coordinator.markFailed(
        job.jobId,
        `failed storing tape in R2: ${safeErrorMessage(error)}`,
      );
      return jsonError(c, 503, "failed storing tape artifact");
    }

    try {
      await c.env.PROOF_QUEUE.send(
        {
          jobId: job.jobId,
        },
        {
          contentType: "json",
        },
      );
    } catch (error) {
      await coordinator.markFailed(
        job.jobId,
        `failed enqueueing proof job: ${safeErrorMessage(error)}`,
      );
      await c.env.PROOF_ARTIFACTS.delete(job.tape.key);
      return jsonError(c, 503, "failed enqueueing proof job");
    }

    const refreshed = await coordinator.getJob(job.jobId);
    if (!refreshed) {
      return jsonError(c, 500, "job disappeared after enqueue");
    }

    return c.json(
      {
        success: true,
        status_url: `/api/proofs/jobs/${job.jobId}`,
        job: asPublicJob(refreshed),
      },
      202,
    );
  });

  api.get("/proofs/jobs/:jobId", async (c) => {
    const jobId = c.req.param("jobId");
    if (!jobId) {
      return jsonError(c, 400, "invalid job id in path");
    }

    const coordinator = coordinatorStub(c.env);
    let job = await coordinator.getJob(jobId);
    if (!job) {
      return jsonError(c, 404, `job not found: ${jobId}`);
    }

    // Opportunistic: if the DO alarm hasn't polled recently (unreliable in local
    // dev), do a single-shot prover check so the frontend sees progress.
    // DOs are single-threaded so this is safe from races in prod.
    if (
      !isTerminalProofStatus(job.status) &&
      job.prover.jobId &&
      (!job.prover.lastPolledAt || Date.now() - new Date(job.prover.lastPolledAt).getTime() > 5_000)
    ) {
      try {
        await coordinator.kickAlarm();
        job = (await coordinator.getJob(jobId)) ?? job;
      } catch {
        // Best-effort — don't fail the read if kicking the alarm errors.
      }
    }

    return c.json({
      success: true,
      job: asPublicJob(job),
    });
  });

  api.get("/proofs/jobs/:jobId/result", async (c) => {
    const jobId = c.req.param("jobId");
    if (!jobId) {
      return jsonError(c, 400, "invalid job id in path");
    }

    // Try the DO first for the canonical artifact key.
    const coordinator = coordinatorStub(c.env);
    const job = await coordinator.getJob(jobId);

    let artifact: R2ObjectBody | null = null;

    if (job?.result?.artifactKey) {
      artifact = await c.env.PROOF_ARTIFACTS.get(job.result.artifactKey);
    } else if (!job) {
      // DO record was pruned — fall back to the well-known R2 key.
      // result.json is retained in R2 beyond DO pruning so users can
      // fetch proof data for on-chain submission.
      artifact = await c.env.PROOF_ARTIFACTS.get(resultKey(jobId));
    }

    if (!artifact) {
      if (job && !job.result?.artifactKey) {
        return jsonError(c, 409, "proof result is not available for this job");
      }
      return jsonError(c, 404, "proof result not found");
    }

    return new Response(artifact.body, {
      status: 200,
      headers: {
        "content-type": "application/json; charset=utf-8",
      },
    });
  });

  api.delete("/proofs/jobs/:jobId", async (c) => {
    const jobId = c.req.param("jobId");
    if (!jobId) {
      return jsonError(c, 400, "invalid job id in path");
    }

    const coordinator = coordinatorStub(c.env);
    const job = await coordinator.markFailed(jobId, "cancelled by user");
    if (!job) {
      return jsonError(c, 404, `job not found: ${jobId}`);
    }

    return c.json({
      success: true,
      job: asPublicJob(job),
    });
  });

  api.notFound((c) => {
    return jsonError(c, 404, `unknown api route: ${c.req.path}`);
  });

  return api;
}
