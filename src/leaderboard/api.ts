export type LeaderboardWindow = "10m" | "day" | "all";

export type ClaimStatus = "queued" | "submitting" | "retrying" | "succeeded" | "failed";

export interface PlayerProfile {
  claimantAddress: string;
  username: string | null;
  linkUrl: string | null;
  updatedAt: string;
}

export interface LeaderboardEntry {
  rank: number;
  jobId: string;
  claimantAddress: string;
  profile: PlayerProfile | null;
  score: number;
  seed: number;
  frameCount: number | null;
  completedAt: string;
  claimStatus: ClaimStatus;
  claimTxHash: string | null;
}

export interface LeaderboardPageResponse {
  success: true;
  window: LeaderboardWindow;
  generated_at: string;
  window_range: {
    start_at: string | null;
    end_at: string | null;
  };
  pagination: {
    limit: number;
    offset: number;
    total: number;
    next_offset: number | null;
  };
  entries: LeaderboardEntry[];
  me: LeaderboardEntry | null;
}

export interface LeaderboardPlayerResponse {
  success: true;
  player: {
    claimant_address: string;
    profile: PlayerProfile | null;
    stats: {
      total_runs: number;
      best_score: number;
      last_played_at: string | null;
    };
    ranks: {
      ten_min: number | null;
      day: number | null;
      all: number | null;
    };
    recent_runs: Array<Omit<LeaderboardEntry, "rank" | "profile">>;
  };
}

interface ApiErrorResponse {
  success: false;
  error?: string;
}

export class LeaderboardApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "LeaderboardApiError";
    this.status = status;
  }
}

async function fetchWithTimeout(
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  timeoutMs: number,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(input, { ...init, signal: controller.signal });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new LeaderboardApiError("request timed out", 0);
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

async function parseError(response: Response): Promise<LeaderboardApiError> {
  let message = `request failed (${response.status})`;
  try {
    const payload = (await response.json()) as ApiErrorResponse;
    if (payload.error && payload.error.trim().length > 0) {
      message = payload.error;
    }
  } catch {
    // ignored
  }

  return new LeaderboardApiError(message, response.status);
}

async function parseJson<T>(response: Response): Promise<T> {
  return (await response.json()) as T;
}

export async function getLeaderboard({
  window,
  limit = 25,
  offset = 0,
  address,
}: {
  window: LeaderboardWindow;
  limit?: number;
  offset?: number;
  address?: string | null;
}): Promise<LeaderboardPageResponse> {
  const params = new URLSearchParams();
  params.set("window", window);
  params.set("limit", `${limit}`);
  params.set("offset", `${offset}`);
  if (address && address.trim().length > 0) {
    params.set("address", address.trim());
  }

  const response = await fetchWithTimeout(
    `/api/leaderboard?${params.toString()}`,
    { method: "GET" },
    10_000,
  );
  if (!response.ok) {
    throw await parseError(response);
  }

  return parseJson<LeaderboardPageResponse>(response);
}

export async function getLeaderboardPlayer(
  claimantAddress: string,
): Promise<LeaderboardPlayerResponse> {
  const response = await fetchWithTimeout(
    `/api/leaderboard/player/${encodeURIComponent(claimantAddress)}`,
    { method: "GET" },
    10_000,
  );
  if (!response.ok) {
    throw await parseError(response);
  }

  return parseJson<LeaderboardPlayerResponse>(response);
}

export async function updateLeaderboardProfile(
  claimantAddress: string,
  updates: { username: string | null; linkUrl: string | null },
): Promise<{ success: true; profile: PlayerProfile }> {
  const response = await fetchWithTimeout(
    `/api/leaderboard/player/${encodeURIComponent(claimantAddress)}/profile`,
    {
      method: "PUT",
      headers: {
        "content-type": "application/json",
        "x-claimant-address": claimantAddress,
      },
      body: JSON.stringify({
        username: updates.username,
        link_url: updates.linkUrl,
      }),
    },
    10_000,
  );

  if (!response.ok) {
    throw await parseError(response);
  }

  return parseJson<{ success: true; profile: PlayerProfile }>(response);
}
