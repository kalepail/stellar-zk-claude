import { Hono } from "hono";
import { DEFAULT_MAX_TAPE_BYTES, EXPECTED_RULES_DIGEST, EXPECTED_RULESET } from "../constants";
import { asPublicJob, coordinatorStub } from "../durable/coordinator";
import type { WorkerEnv } from "../env";
import { resultKey } from "../keys";
import { recordLeaderboardSyncFailure, runLeaderboardSync } from "../leaderboard-sync";
import { parseLeaderboardSourceMode } from "../leaderboard-ingestion";
import {
  computeLeaderboard,
  DEFAULT_LEADERBOARD_LIMIT,
  extractLeaderboardRuns,
  MAX_LEADERBOARD_LIMIT,
  parseLeaderboardWindow,
} from "../leaderboard";
import { describeProverHealthError, getValidatedProverHealth } from "../prover/client";
import { parseAndValidateTape } from "../tape";
import type { LeaderboardRankedEntry, PlayerProfileRecord } from "../types";
import { isTerminalProofStatus, parseInteger, safeErrorMessage } from "../utils";
import { validateClaimantStrKeyFromUserInput } from "../../shared/stellar/strkey";
import type { LeaderboardResolvedSourceMode } from "../leaderboard-ingestion";

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
  return parsed;
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
  if (provided !== configured) {
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

function attachProfilesToEntry(
  entry: LeaderboardRankedEntry,
  profiles: Record<string, PlayerProfileRecord>,
): LeaderboardRankedEntry & { profile: PlayerProfileRecord | null } {
  return {
    ...entry,
    profile: profiles[entry.claimantAddress] ?? null,
  };
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

    const coordinator = coordinatorStub(c.env);
    const [state, events] = await Promise.all([
      coordinator.getLeaderboardIngestionState(),
      coordinator.listLeaderboardEvents(),
    ]);

    return c.json({
      success: true,
      source: "events",
      provider: state.provider,
      provider_mode: parseLeaderboardSourceMode(c.env),
      source_mode: state.sourceMode,
      event_count: events.length,
      state,
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

  api.get("/leaderboard", async (c) => {
    const window = parseLeaderboardWindow(c.req.query("window"));
    if (!window) {
      return jsonError(c, 400, "window must be one of: 10m, day, all");
    }

    const limit = parseLimit(c.req.query("limit"));
    const offset = parseOffset(c.req.query("offset"));

    let claimantAddress: string | null;
    try {
      claimantAddress = normalizeOptionalClaimantAddress(c.req.query("address"));
    } catch (error) {
      return jsonError(c, 400, `invalid address: ${safeErrorMessage(error)}`);
    }

    const coordinator = coordinatorStub(c.env);
    const [events, ingestionState] = await Promise.all([
      coordinator.listLeaderboardEvents(),
      coordinator.getLeaderboardIngestionState(),
    ]);
    const runs = extractLeaderboardRuns(events);
    const computed = computeLeaderboard(runs, {
      window,
      limit,
      offset,
      claimantAddress,
    });

    const profileAddresses = new Set<string>();
    for (const entry of computed.entries) {
      profileAddresses.add(entry.claimantAddress);
    }
    if (computed.me) {
      profileAddresses.add(computed.me.claimantAddress);
    }

    const profiles = await coordinator.getProfiles(Array.from(profileAddresses));

    return c.json({
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
      entries: computed.entries.map((entry) => attachProfilesToEntry(entry, profiles)),
      me: computed.me ? attachProfilesToEntry(computed.me, profiles) : null,
      ingestion: {
        last_synced_at: ingestionState.lastSyncedAt,
        highest_ledger: ingestionState.highestLedger,
        total_events: ingestionState.totalEvents,
      },
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

    const coordinator = coordinatorStub(c.env);
    const [profile, events] = await Promise.all([
      coordinator.getProfile(claimantAddress),
      coordinator.listLeaderboardEvents(),
    ]);

    const runs = extractLeaderboardRuns(events);
    const playerRuns = runs.filter((run) => run.claimantAddress === claimantAddress);
    // eslint-disable-next-line unicorn/no-array-sort
    playerRuns.sort((left, right) => {
      const leftMs = new Date(left.completedAt).getTime();
      const rightMs = new Date(right.completedAt).getTime();
      if (leftMs !== rightMs) {
        return rightMs - leftMs;
      }
      if (left.score !== right.score) {
        return right.score - left.score;
      }
      return left.jobId.localeCompare(right.jobId);
    });

    const bestRun = playerRuns.reduce<(typeof playerRuns)[number] | null>((best, run) => {
      if (!best || run.score > best.score) {
        return run;
      }
      if (run.score === best.score && run.completedAt < best.completedAt) {
        return run;
      }
      return best;
    }, null);

    const rank10m = computeLeaderboard(runs, {
      window: "10m",
      claimantAddress,
      limit: 1,
      offset: 0,
    }).me?.rank;
    const rankDay = computeLeaderboard(runs, {
      window: "day",
      claimantAddress,
      limit: 1,
      offset: 0,
    }).me?.rank;
    const rankAll = computeLeaderboard(runs, {
      window: "all",
      claimantAddress,
      limit: 1,
      offset: 0,
    }).me?.rank;

    return c.json({
      success: true,
      player: {
        claimant_address: claimantAddress,
        profile,
        stats: {
          total_runs: playerRuns.length,
          best_score: bestRun?.score ?? 0,
          last_played_at: playerRuns[0]?.completedAt ?? null,
        },
        ranks: {
          ten_min: rank10m ?? null,
          day: rankDay ?? null,
          all: rankAll ?? null,
        },
        recent_runs: playerRuns.slice(0, 25),
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

    const rawActor = c.req.header("x-claimant-address");
    if (!rawActor) {
      return jsonError(c, 401, "x-claimant-address is required to update profile");
    }

    let actorAddress: string;
    try {
      actorAddress = validateClaimantStrKeyFromUserInput(rawActor);
    } catch (error) {
      return jsonError(c, 400, `invalid x-claimant-address: ${safeErrorMessage(error)}`);
    }

    if (actorAddress !== claimantAddress) {
      return jsonError(c, 403, "profile updates are only allowed for the same claimant address");
    }

    let payload: Record<string, unknown>;
    try {
      const body = await c.req.json();
      if (!body || typeof body !== "object" || Array.isArray(body)) {
        return jsonError(c, 400, "profile payload must be an object");
      }
      payload = body as Record<string, unknown>;
    } catch (error) {
      return jsonError(c, 400, `invalid JSON payload: ${safeErrorMessage(error)}`);
    }

    let username: string | null;
    let linkUrl: string | null;
    try {
      username = sanitizeProfileUsername(payload.username);
      linkUrl = sanitizeProfileLinkUrl(payload.link_url ?? payload.linkUrl);
    } catch (error) {
      return jsonError(c, 400, safeErrorMessage(error));
    }

    const coordinator = coordinatorStub(c.env);
    const profile = await coordinator.upsertProfile(claimantAddress, {
      username,
      linkUrl,
    });

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
