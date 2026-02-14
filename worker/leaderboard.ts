import type {
  LeaderboardComputedPage,
  LeaderboardEventRecord,
  LeaderboardRankedEntry,
  LeaderboardRunRecord,
  LeaderboardWindow,
  LeaderboardWindowMetadata,
} from "./types";

const TEN_MINUTES_MS = 10 * 60 * 1000;
const ONE_DAY_MS = 24 * 60 * 60 * 1000;

export const DEFAULT_LEADERBOARD_LIMIT = 25;
export const MAX_LEADERBOARD_LIMIT = 100;
export const MAX_LEADERBOARD_OFFSET = 10_000;

function timestampMs(value: string | null): number {
  if (!value) {
    return 0;
  }

  const parsed = new Date(value).getTime();
  return Number.isFinite(parsed) ? parsed : 0;
}

function getWindowCutoffMs(window: LeaderboardWindow, nowMs: number): number {
  if (window === "10m") {
    return nowMs - TEN_MINUTES_MS;
  }

  if (window === "day") {
    return nowMs - ONE_DAY_MS;
  }

  return 0;
}

function getWindowMetadata(window: LeaderboardWindow, nowMs: number): LeaderboardWindowMetadata {
  if (window === "all") {
    return {
      startAt: null,
      endAt: new Date(nowMs).toISOString(),
    };
  }

  return {
    startAt: new Date(getWindowCutoffMs(window, nowMs)).toISOString(),
    endAt: new Date(nowMs).toISOString(),
  };
}

function isBetterRun(candidate: LeaderboardRunRecord, current: LeaderboardRunRecord): boolean {
  if (candidate.score !== current.score) {
    return candidate.score > current.score;
  }

  const candidateMs = timestampMs(candidate.completedAt);
  const currentMs = timestampMs(current.completedAt);
  if (candidateMs !== currentMs) {
    return candidateMs < currentMs;
  }

  return candidate.jobId < current.jobId;
}

function compareRankedRuns(a: LeaderboardRunRecord, b: LeaderboardRunRecord): number {
  if (a.score !== b.score) {
    return b.score - a.score;
  }

  const aMs = timestampMs(a.completedAt);
  const bMs = timestampMs(b.completedAt);
  if (aMs !== bMs) {
    return aMs - bMs;
  }

  return a.jobId.localeCompare(b.jobId);
}

function toRankedEntries(runs: LeaderboardRunRecord[]): LeaderboardRankedEntry[] {
  return runs.map((run, index) => ({
    ...run,
    rank: index + 1,
  }));
}

export function parseLeaderboardWindow(raw: string | undefined): LeaderboardWindow | null {
  if (!raw || raw.trim().length === 0) {
    return "10m";
  }

  const normalized = raw.trim().toLowerCase();
  if (normalized === "10m" || normalized === "day" || normalized === "all") {
    return normalized;
  }

  return null;
}

export function extractLeaderboardRuns(events: LeaderboardEventRecord[]): LeaderboardRunRecord[] {
  const runs: LeaderboardRunRecord[] = [];

  for (const event of events) {
    const completedAtMs = timestampMs(event.closedAt);
    if (completedAtMs === 0) {
      continue;
    }
    if (event.newBest <= 0) {
      continue;
    }

    runs.push({
      jobId: event.eventId,
      claimantAddress: event.claimantAddress,
      score: event.newBest >>> 0,
      mintedDelta: event.mintedDelta >>> 0,
      seed: event.seed >>> 0,
      frameCount:
        typeof event.frameCount === "number" && Number.isFinite(event.frameCount)
          ? event.frameCount >>> 0
          : null,
      finalRngState:
        typeof event.finalRngState === "number" && Number.isFinite(event.finalRngState)
          ? event.finalRngState >>> 0
          : null,
      tapeChecksum:
        typeof event.tapeChecksum === "number" && Number.isFinite(event.tapeChecksum)
          ? event.tapeChecksum >>> 0
          : null,
      rulesDigest:
        typeof event.rulesDigest === "number" && Number.isFinite(event.rulesDigest)
          ? event.rulesDigest >>> 0
          : null,
      completedAt: event.closedAt,
      claimStatus: "succeeded",
      claimTxHash: event.txHash,
    });
  }

  return runs;
}

export function computeLeaderboard(
  runs: LeaderboardRunRecord[],
  {
    window,
    nowMs,
    limit,
    offset,
    claimantAddress,
  }: {
    window: LeaderboardWindow;
    nowMs?: number;
    limit?: number;
    offset?: number;
    claimantAddress?: string | null;
  },
): LeaderboardComputedPage {
  const effectiveNowMs = nowMs ?? Date.now();
  const cutoffMs = getWindowCutoffMs(window, effectiveNowMs);
  const effectiveLimit = Math.min(
    Math.max(limit ?? DEFAULT_LEADERBOARD_LIMIT, 1),
    MAX_LEADERBOARD_LIMIT,
  );
  const effectiveOffset = Math.min(Math.max(offset ?? 0, 0), MAX_LEADERBOARD_OFFSET);

  const bestByClaimant = new Map<string, LeaderboardRunRecord>();
  for (const run of runs) {
    const completedAtMs = timestampMs(run.completedAt);
    if (completedAtMs === 0) {
      continue;
    }
    if (window !== "all" && completedAtMs < cutoffMs) {
      continue;
    }

    const current = bestByClaimant.get(run.claimantAddress);
    if (!current || isBetterRun(run, current)) {
      bestByClaimant.set(run.claimantAddress, run);
    }
  }

  const sorted = Array.from(bestByClaimant.values());
  // eslint-disable-next-line unicorn/no-array-sort
  sorted.sort(compareRankedRuns);
  const ranked = toRankedEntries(sorted);

  const pagedEntries = ranked.slice(effectiveOffset, effectiveOffset + effectiveLimit);
  const me = claimantAddress
    ? (ranked.find((entry) => entry.claimantAddress === claimantAddress) ?? null)
    : null;
  const nextOffset =
    effectiveOffset + effectiveLimit < ranked.length ? effectiveOffset + effectiveLimit : null;

  return {
    window,
    generatedAt: new Date(effectiveNowMs).toISOString(),
    windowRange: getWindowMetadata(window, effectiveNowMs),
    totalPlayers: ranked.length,
    limit: effectiveLimit,
    offset: effectiveOffset,
    nextOffset,
    entries: pagedEntries,
    me,
  };
}
