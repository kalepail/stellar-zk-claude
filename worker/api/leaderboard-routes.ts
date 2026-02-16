import { Hono } from "hono";
import {
  generateAuthenticationOptions,
  verifyAuthenticationResponse,
} from "@simplewebauthn/server";
import type { WorkerEnv } from "../env";
import { LEADERBOARD_CACHE_CONTROL, LEADERBOARD_PRIVATE_CACHE_CONTROL } from "../cache-control";
import {
  DEFAULT_LEADERBOARD_LIMIT,
  MAX_LEADERBOARD_LIMIT,
  MAX_LEADERBOARD_OFFSET,
  parseLeaderboardWindow,
} from "../leaderboard";
import {
  getLeaderboardPage,
  getLeaderboardPlayer,
  getLeaderboardIngestionState,
  createLeaderboardProfileAuthChallenge,
  getLeaderboardProfileAuthChallenge,
  getLeaderboardProfileCredential,
  markLeaderboardProfileAuthChallengeUsed,
  purgeExpiredLeaderboardProfileAuthChallenges,
  updateLeaderboardProfileCredentialCounter,
  upsertLeaderboardProfile,
  upsertLeaderboardProfileCredential,
} from "../leaderboard-store";
import {
  assertCredentialBelongsToClaimantContract,
  encodeRawP256PublicKeyBase64UrlToCose,
  LeaderboardCredentialBindingError,
  normalizeAuthenticatorTransports,
} from "../leaderboard-profile-auth";
import { safeErrorMessage } from "../utils";

const CHALLENGE_TTL_MS = 5 * 60 * 1000;
const MAX_USERNAME_LENGTH = 32;
const MAX_LINK_URL_LENGTH = 240;
const USERNAME_PATTERN = /^[a-zA-Z0-9 _.@#-]+$/u;

const READ_RATE_LIMIT = 60;
const WRITE_RATE_LIMIT = 10;
const RATE_WINDOW_MS = 60_000;

const rateLimitCounters = new Map<string, { count: number; resetAt: number }>();

function checkRateLimit(ip: string, limit: number): boolean {
  const now = Date.now();
  const key = `${ip}:${limit}`;
  const entry = rateLimitCounters.get(key);

  if (!entry || now >= entry.resetAt) {
    rateLimitCounters.set(key, { count: 1, resetAt: now + RATE_WINDOW_MS });
    return true;
  }

  entry.count += 1;
  return entry.count <= limit;
}

function clientIp(c: { req: { raw: Request } }): string {
  return c.req.raw.headers.get("cf-connecting-ip") ?? "unknown";
}

function jsonError(
  c: { json: (body: unknown, status?: number) => Response },
  status: number,
  error: string,
): Response {
  return c.json({ success: false, error }, status);
}

function validateLinkUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" || parsed.protocol === "http:";
  } catch {
    return false;
  }
}

function sanitizeUsername(value: string): string | null {
  const trimmed = value.trim();
  if (trimmed.length === 0) {
    return null;
  }
  if (trimmed.length > MAX_USERNAME_LENGTH) {
    return null;
  }
  if (!USERNAME_PATTERN.test(trimmed)) {
    return null;
  }
  return trimmed;
}

function safeLinkUrl(url: string | null | undefined): string | null {
  if (!url || url.trim().length === 0) {
    return null;
  }
  const trimmed = url.trim();
  if (trimmed.length > MAX_LINK_URL_LENGTH) {
    return null;
  }
  if (!validateLinkUrl(trimmed)) {
    return null;
  }
  return trimmed;
}

export function createLeaderboardRouter(): Hono<{ Bindings: WorkerEnv }> {
  const router = new Hono<{ Bindings: WorkerEnv }>();

  // GET /api/leaderboard
  router.get("/", async (c) => {
    if (!checkRateLimit(clientIp(c), READ_RATE_LIMIT)) {
      c.header("Retry-After", "60");
      return jsonError(c, 429, "rate limit exceeded");
    }

    try {
      const windowRaw = c.req.query("window");
      const window = parseLeaderboardWindow(windowRaw);
      if (!window) {
        return jsonError(c, 400, `invalid window: ${windowRaw}`);
      }

      const limitRaw = c.req.query("limit");
      const limit = limitRaw
        ? Math.min(
            Math.max(Number.parseInt(limitRaw, 10) || DEFAULT_LEADERBOARD_LIMIT, 1),
            MAX_LEADERBOARD_LIMIT,
          )
        : DEFAULT_LEADERBOARD_LIMIT;

      const offsetRaw = c.req.query("offset");
      const offset = offsetRaw
        ? Math.min(Math.max(Number.parseInt(offsetRaw, 10) || 0, 0), MAX_LEADERBOARD_OFFSET)
        : 0;

      const address = c.req.query("address")?.trim() || null;

      const page = await getLeaderboardPage(c.env, {
        window,
        limit,
        offset,
        claimantAddress: address,
      });

      const ingestion = await getLeaderboardIngestionState(c.env);

      c.header("Cache-Control", LEADERBOARD_CACHE_CONTROL);
      return c.json({
        success: true,
        source: "d1",
        provider: ingestion.provider,
        provider_mode: ingestion.provider,
        source_mode: ingestion.sourceMode,
        window: page.window,
        generated_at: page.generatedAt,
        window_range: {
          start_at: page.windowRange.startAt,
          end_at: page.windowRange.endAt,
        },
        pagination: {
          limit: page.limit,
          offset: page.offset,
          total: page.totalPlayers,
          next_offset: page.nextOffset,
        },
        entries: page.entries,
        me: page.me,
        ingestion: {
          last_synced_at: ingestion.lastSyncedAt,
          highest_ledger: ingestion.highestLedger,
          total_events: ingestion.totalEvents,
        },
      });
    } catch (error) {
      console.error(`[leaderboard] GET / error: ${safeErrorMessage(error)}`);
      return jsonError(c, 503, "leaderboard temporarily unavailable");
    }
  });

  // GET /api/leaderboard/player/:address
  router.get("/player/:address", async (c) => {
    if (!checkRateLimit(clientIp(c), READ_RATE_LIMIT)) {
      c.header("Retry-After", "60");
      return jsonError(c, 429, "rate limit exceeded");
    }

    try {
      const address = c.req.param("address");
      if (!address || address.trim().length === 0) {
        return jsonError(c, 400, "missing player address");
      }

      const player = await getLeaderboardPlayer(c.env, address);

      c.header("Cache-Control", LEADERBOARD_PRIVATE_CACHE_CONTROL);
      return c.json({
        success: true,
        player: {
          claimant_address: address,
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
      });
    } catch (error) {
      console.error(`[leaderboard] GET /player/:address error: ${safeErrorMessage(error)}`);
      return jsonError(c, 503, "leaderboard temporarily unavailable");
    }
  });

  // POST /api/leaderboard/player/:address/profile/auth/options
  router.post("/player/:address/profile/auth/options", async (c) => {
    if (!checkRateLimit(clientIp(c), WRITE_RATE_LIMIT)) {
      c.header("Retry-After", "60");
      return jsonError(c, 429, "rate limit exceeded");
    }

    try {
      const address = c.req.param("address");
      if (!address || address.trim().length === 0) {
        return jsonError(c, 400, "missing player address");
      }

      let body: Record<string, unknown>;
      try {
        body = (await c.req.json()) as Record<string, unknown>;
      } catch {
        return jsonError(c, 400, "invalid JSON body");
      }

      const credentialId = body.credential_id;
      const credentialPublicKey = body.credential_public_key;
      if (typeof credentialId !== "string" || credentialId.trim().length === 0) {
        return jsonError(c, 400, "missing credential_id");
      }
      if (typeof credentialPublicKey !== "string" || credentialPublicKey.trim().length === 0) {
        return jsonError(c, 400, "missing credential_public_key");
      }

      let transports: string[] | null;
      try {
        transports = normalizeAuthenticatorTransports(body.transports ?? null);
      } catch (error) {
        return jsonError(c, 400, safeErrorMessage(error));
      }

      // Verify credential belongs to the claimant address
      try {
        await assertCredentialBelongsToClaimantContract({
          claimantAddress: address,
          credentialIdBase64Url: credentialId,
          indexerBaseUrl: c.env.SMART_ACCOUNT_INDEXER_URL,
        });
      } catch (error) {
        if (error instanceof LeaderboardCredentialBindingError) {
          return jsonError(c, error.statusCode, error.message);
        }
        throw error;
      }

      // Upsert credential
      await upsertLeaderboardProfileCredential(c.env, {
        claimantAddress: address,
        credentialId,
        publicKey: credentialPublicKey,
        transports: transports as string[] | undefined,
      });

      // Purge expired challenges
      await purgeExpiredLeaderboardProfileAuthChallenges(c.env);

      // Generate WebAuthn authentication options
      const rpId = new URL(c.req.url).hostname;
      const options = await generateAuthenticationOptions({
        rpID: rpId,
        allowCredentials: [
          {
            id: credentialId,
            transports: transports as
              | import("@simplewebauthn/server").AuthenticatorTransportFuture[]
              | undefined,
          },
        ],
        userVerification: "required",
        timeout: CHALLENGE_TTL_MS,
      });

      // Store challenge
      const challengeId = crypto.randomUUID();
      const nowMs = Date.now();
      const expiresAt = new Date(nowMs + CHALLENGE_TTL_MS).toISOString();
      const origin = new URL(c.req.url).origin;

      await createLeaderboardProfileAuthChallenge(c.env, {
        challengeId,
        claimantAddress: address,
        credentialId,
        challenge: options.challenge,
        expectedOrigin: origin,
        expectedRpId: rpId,
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
    } catch (error) {
      console.error(`[leaderboard] POST auth/options error: ${safeErrorMessage(error)}`);
      return jsonError(c, 503, "leaderboard temporarily unavailable");
    }
  });

  // PUT /api/leaderboard/player/:address/profile
  router.put("/player/:address/profile", async (c) => {
    if (!checkRateLimit(clientIp(c), WRITE_RATE_LIMIT)) {
      c.header("Retry-After", "60");
      return jsonError(c, 429, "rate limit exceeded");
    }

    try {
      const address = c.req.param("address");
      if (!address || address.trim().length === 0) {
        return jsonError(c, 400, "missing player address");
      }

      let body: Record<string, unknown>;
      try {
        body = (await c.req.json()) as Record<string, unknown>;
      } catch {
        return jsonError(c, 400, "invalid JSON body");
      }

      // Validate username
      const rawUsername = body.username;
      let username: string | null = null;
      if (rawUsername !== null && rawUsername !== undefined) {
        if (typeof rawUsername !== "string") {
          return jsonError(c, 400, "username must be a string");
        }
        if (rawUsername.trim().length > 0) {
          username = sanitizeUsername(rawUsername);
          if (username === null) {
            return jsonError(
              c,
              400,
              `username must be 1-${MAX_USERNAME_LENGTH} chars, alphanumeric and basic punctuation only`,
            );
          }
        }
      }

      // Validate link_url
      const rawLinkUrl = body.link_url;
      let linkUrl: string | null = null;
      if (rawLinkUrl !== null && rawLinkUrl !== undefined) {
        if (typeof rawLinkUrl !== "string") {
          return jsonError(c, 400, "link_url must be a string");
        }
        if (rawLinkUrl.trim().length > 0) {
          linkUrl = safeLinkUrl(rawLinkUrl);
          if (linkUrl === null) {
            return jsonError(
              c,
              400,
              `link_url must be a valid http or https URL (max ${MAX_LINK_URL_LENGTH} chars)`,
            );
          }
        }
      }

      // Validate auth
      const auth = body.auth;
      if (!auth || typeof auth !== "object" || Array.isArray(auth)) {
        return jsonError(c, 400, "missing auth object");
      }
      const authObj = auth as Record<string, unknown>;
      const challengeId = authObj.challenge_id;
      const authResponse = authObj.response;
      if (typeof challengeId !== "string" || challengeId.trim().length === 0) {
        return jsonError(c, 400, "missing auth.challenge_id");
      }
      if (!authResponse || typeof authResponse !== "object") {
        return jsonError(c, 400, "missing auth.response");
      }

      // Retrieve and validate challenge
      const challenge = await getLeaderboardProfileAuthChallenge(c.env, challengeId);
      if (!challenge) {
        return jsonError(c, 400, "invalid or expired challenge");
      }
      if (challenge.claimantAddress !== address) {
        return jsonError(c, 403, "challenge does not belong to this address");
      }
      if (challenge.usedAt !== null) {
        return jsonError(c, 400, "challenge already used");
      }

      // Check TTL
      const expiresAtMs = new Date(challenge.expiresAt).getTime();
      if (!Number.isFinite(expiresAtMs) || Date.now() > expiresAtMs) {
        return jsonError(c, 400, "challenge has expired");
      }

      // Get stored credential
      const credential = await getLeaderboardProfileCredential(c.env, challenge.credentialId);
      if (!credential) {
        return jsonError(c, 400, "credential not found");
      }
      if (credential.claimantAddress !== address) {
        return jsonError(c, 403, "credential does not belong to this address");
      }

      // Verify WebAuthn authentication response
      let verification;
      try {
        const coseKey = encodeRawP256PublicKeyBase64UrlToCose(credential.publicKey);
        verification = await verifyAuthenticationResponse({
          response: authResponse as import("@simplewebauthn/server").AuthenticationResponseJSON,
          expectedChallenge: challenge.challenge,
          expectedOrigin: challenge.expectedOrigin,
          expectedRPID: challenge.expectedRpId,
          credential: {
            id: credential.credentialId,
            publicKey: Uint8Array.from(coseKey),
            counter: credential.counter,
            transports: credential.transports as
              | import("@simplewebauthn/server").AuthenticatorTransportFuture[]
              | undefined,
          },
          requireUserVerification: true,
        });
      } catch (error) {
        return jsonError(c, 401, `authentication verification failed: ${safeErrorMessage(error)}`);
      }

      if (!verification.verified) {
        return jsonError(c, 401, "authentication verification failed");
      }

      // Mark challenge used and update counter
      await markLeaderboardProfileAuthChallengeUsed(c.env, challengeId);
      await updateLeaderboardProfileCredentialCounter(
        c.env,
        credential.credentialId,
        verification.authenticationInfo.newCounter,
      );

      // Upsert profile
      const profile = await upsertLeaderboardProfile(c.env, address, {
        username,
        linkUrl,
      });

      return c.json({
        success: true,
        profile,
      });
    } catch (error) {
      console.error(`[leaderboard] PUT profile error: ${safeErrorMessage(error)}`);
      return jsonError(c, 503, "leaderboard temporarily unavailable");
    }
  });

  return router;
}
