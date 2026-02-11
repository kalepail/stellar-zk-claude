import { coordinatorStub } from "./durable/coordinator";
import type { WorkerEnv } from "./env";
import { fetchLeaderboardEventsFromGalexie } from "./leaderboard-ingestion";
import type { LeaderboardIngestionState } from "./types";
import { parseInteger, safeErrorMessage } from "./utils";
import type { LeaderboardResolvedSourceMode } from "./leaderboard-ingestion";

export interface LeaderboardSyncRequest {
  mode: "forward" | "backfill";
  cursor?: string | null;
  fromLedger?: number | null;
  toLedger?: number | null;
  limit?: number;
  source?: LeaderboardResolvedSourceMode | "default" | null;
}

export interface LeaderboardSyncResult {
  mode: "forward" | "backfill";
  requested: {
    cursor: string | null;
    from_ledger: number | null;
    to_ledger: number | null;
    limit: number | null;
    source: LeaderboardResolvedSourceMode | "default";
  };
  fetched_count: number;
  accepted_count: number;
  inserted_count: number;
  updated_count: number;
  next_cursor: string | null;
  provider: "galexie" | "rpc";
  source_mode: LeaderboardResolvedSourceMode;
  state: LeaderboardIngestionState;
}

function parseBoolean(raw: string | undefined, defaultValue: boolean): boolean {
  if (!raw) {
    return defaultValue;
  }

  const normalized = raw.trim().toLowerCase();
  if (["1", "true", "yes", "y", "on"].includes(normalized)) {
    return true;
  }
  if (["0", "false", "no", "n", "off"].includes(normalized)) {
    return false;
  }

  return defaultValue;
}

function shouldRunCatchup(
  state: LeaderboardIngestionState,
  nowMs: number,
  intervalMinutes: number,
): boolean {
  if (intervalMinutes <= 0) {
    return false;
  }

  if (!state.lastBackfillAt) {
    return true;
  }

  const lastBackfillMs = new Date(state.lastBackfillAt).getTime();
  if (!Number.isFinite(lastBackfillMs)) {
    return true;
  }

  return nowMs - lastBackfillMs >= intervalMinutes * 60_000;
}

export async function runLeaderboardSync(
  env: WorkerEnv,
  request: LeaderboardSyncRequest,
): Promise<LeaderboardSyncResult> {
  const coordinator = coordinatorStub(env);
  const existingState = await coordinator.getLeaderboardIngestionState();

  const effectiveCursor =
    request.mode === "forward" ? (request.cursor ?? existingState.cursor) : request.cursor;

  const fetched = await fetchLeaderboardEventsFromGalexie(env, {
    cursor: effectiveCursor,
    fromLedger: request.fromLedger,
    toLedger: request.toLedger,
    limit: request.limit,
    source: request.source ?? null,
  });

  const upsert = await coordinator.upsertLeaderboardEvents(fetched.events);
  const ledgers = fetched.events
    .map((event) => event.ledger)
    .filter((value): value is number => typeof value === "number");
  const highestLedgerFromBatch = ledgers.length > 0 ? Math.max(...ledgers) : null;
  const nowIso = new Date().toISOString();

  const nextState: LeaderboardIngestionState = {
    ...existingState,
    provider: fetched.provider,
    sourceMode: fetched.sourceMode,
    cursor:
      request.mode === "forward"
        ? (fetched.nextCursor ?? effectiveCursor ?? existingState.cursor)
        : existingState.cursor,
    highestLedger:
      highestLedgerFromBatch === null
        ? existingState.highestLedger
        : Math.max(existingState.highestLedger ?? 0, highestLedgerFromBatch),
    lastSyncedAt: nowIso,
    lastBackfillAt: request.mode === "backfill" ? nowIso : existingState.lastBackfillAt,
    totalEvents: existingState.totalEvents + upsert.inserted,
    lastError: null,
  };

  await coordinator.setLeaderboardIngestionState(nextState);

  return {
    mode: request.mode,
    requested: {
      cursor: effectiveCursor ?? null,
      from_ledger: request.fromLedger ?? null,
      to_ledger: request.toLedger ?? null,
      limit: request.limit ?? null,
      source: request.source ?? "default",
    },
    fetched_count: fetched.fetchedCount,
    accepted_count: fetched.events.length,
    inserted_count: upsert.inserted,
    updated_count: upsert.updated,
    next_cursor: fetched.nextCursor,
    provider: fetched.provider,
    source_mode: fetched.sourceMode,
    state: nextState,
  };
}

export async function runScheduledLeaderboardSync(
  env: WorkerEnv,
  scheduledTimeMs = Date.now(),
): Promise<{
  enabled: boolean;
  forward: LeaderboardSyncResult | null;
  catchup: LeaderboardSyncResult | null;
  warning: string | null;
}> {
  const enabled = parseBoolean(env.LEADERBOARD_SYNC_CRON_ENABLED, true);
  if (!enabled) {
    return {
      enabled: false,
      forward: null,
      catchup: null,
      warning: null,
    };
  }

  const limit = parseInteger(env.LEADERBOARD_SYNC_CRON_LIMIT, 200, 1);
  const catchupIntervalMinutes = parseInteger(env.LEADERBOARD_CATCHUP_INTERVAL_MINUTES, 30, 0);
  const catchupWindowLedgers = parseInteger(env.LEADERBOARD_CATCHUP_WINDOW_LEDGERS, 0, 0);

  const forward = await runLeaderboardSync(env, {
    mode: "forward",
    limit,
  });

  let catchup: LeaderboardSyncResult | null = null;
  let warning: string | null = null;

  if (
    catchupWindowLedgers > 0 &&
    shouldRunCatchup(forward.state, scheduledTimeMs, catchupIntervalMinutes)
  ) {
    const highestLedger = forward.state.highestLedger;
    if (highestLedger === null) {
      warning = "skipped catchup backfill because highest ledger is unknown";
    } else {
      const fromLedger = Math.max(2, highestLedger - catchupWindowLedgers);
      catchup = await runLeaderboardSync(env, {
        mode: "backfill",
        fromLedger,
        toLedger: highestLedger,
        limit,
        source: "datalake",
      });
    }
  }

  return {
    enabled: true,
    forward,
    catchup,
    warning,
  };
}

export async function recordLeaderboardSyncFailure(env: WorkerEnv, error: unknown): Promise<void> {
  const coordinator = coordinatorStub(env);
  const existingState = await coordinator.getLeaderboardIngestionState();
  await coordinator.setLeaderboardIngestionState({
    ...existingState,
    lastError: safeErrorMessage(error),
    lastSyncedAt: new Date().toISOString(),
  });
}
