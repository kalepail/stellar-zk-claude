import { describe, expect, it, mock } from "bun:test";
import type { WorkerEnv } from "../../worker/env";
import {
  LEADERBOARD_CACHE_CONTROL,
  LEADERBOARD_PRIVATE_CACHE_CONTROL,
} from "../../worker/cache-control";

const VALID_CLAIMANT_CONTRACT = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4";
const EXAMPLE_GENERATED_AT = "2026-02-14T00:00:00.000Z";

mock.module("../../worker/queue/consumer", () => ({
  handleDlqBatch: async () => undefined,
  handleQueueBatch: async () => undefined,
  handleClaimQueueBatch: async () => undefined,
  handleClaimDlqBatch: async () => undefined,
}));

mock.module("../../worker/leaderboard-sync", () => ({
  runScheduledLeaderboardSync: async () => ({
    enabled: false,
    warning: null,
  }),
  recordLeaderboardSyncFailure: async () => undefined,
  runLeaderboardSync: async () => ({
    mode: "forward",
    requested: {
      cursor: null,
      from_ledger: null,
      to_ledger: null,
      limit: null,
      source: "default",
    },
    fetched: {
      provider: "rpc",
      source_mode: "rpc",
      count: 0,
      cursor: null,
    },
    upserted: {
      inserted: 0,
      updated: 0,
      total_events: 1,
    },
    state: {
      provider: "rpc",
      sourceMode: "rpc",
      cursor: "ledger:1",
      highestLedger: 1,
      lastSyncedAt: EXAMPLE_GENERATED_AT,
      lastBackfillAt: null,
      totalEvents: 1,
      lastError: null,
    },
  }),
}));

mock.module("../../worker/leaderboard-store", () => ({
  countLeaderboardEvents: async () => 1,
  createLeaderboardProfileAuthChallenge: async () => undefined,
  getLeaderboardIngestionState: async () => ({
    provider: "rpc",
    sourceMode: "rpc",
    cursor: "ledger:1",
    highestLedger: 1,
    lastSyncedAt: EXAMPLE_GENERATED_AT,
    lastBackfillAt: null,
    totalEvents: 1,
    lastError: null,
  }),
  getLeaderboardPage: async (env: WorkerEnv, options: { claimantAddress: string | null }) => ({
    window: "all",
    generatedAt: EXAMPLE_GENERATED_AT,
    windowRange: {
      startAt: null,
      endAt: EXAMPLE_GENERATED_AT,
    },
    totalPlayers: 1,
    limit: 25,
    offset: 0,
    nextOffset: null,
    entries: [
      {
        rank: 1,
        jobId: "evt-1",
        claimantAddress: VALID_CLAIMANT_CONTRACT,
        score: 1337,
        mintedDelta: 1337,
        seed: 42,
        frameCount: 1200,
        finalRngState: 99,
        tapeChecksum: 0xdead,
        rulesDigest: 0x4153_5433,
        completedAt: EXAMPLE_GENERATED_AT,
        claimStatus: "succeeded",
        claimTxHash: "tx-1",
        profile: {
          claimantAddress: VALID_CLAIMANT_CONTRACT,
          username: "pilot",
          linkUrl: null,
          updatedAt: EXAMPLE_GENERATED_AT,
        },
      },
    ],
    me:
      options.claimantAddress === VALID_CLAIMANT_CONTRACT
        ? {
            rank: 1,
            jobId: "evt-1",
            claimantAddress: VALID_CLAIMANT_CONTRACT,
            score: 1337,
            mintedDelta: 1337,
            seed: 42,
            frameCount: 1200,
            finalRngState: 99,
            tapeChecksum: 0xdead,
            rulesDigest: 0x4153_5433,
            completedAt: EXAMPLE_GENERATED_AT,
            claimStatus: "succeeded",
            claimTxHash: "tx-1",
            profile: {
              claimantAddress: VALID_CLAIMANT_CONTRACT,
              username: "pilot",
              linkUrl: null,
              updatedAt: EXAMPLE_GENERATED_AT,
            },
          }
        : null,
  }),
  getLeaderboardPlayer: async () => ({
    profile: null,
    stats: {
      totalRuns: 0,
      bestScore: 0,
      totalMinted: 0,
      lastPlayedAt: null,
    },
    ranks: {
      tenMin: null,
      day: null,
      all: null,
    },
    recentRuns: [],
  }),
  getLeaderboardProfileAuthChallenge: async () => null,
  getLeaderboardProfileCredential: async () => null,
  markLeaderboardProfileAuthChallengeUsed: async () => false,
  purgeExpiredLeaderboardProfileAuthChallenges: async () => undefined,
  setLeaderboardIngestionState: async () => undefined,
  updateLeaderboardProfileCredentialCounter: async () => undefined,
  upsertLeaderboardEvents: async () => ({ inserted: 0, updated: 0 }),
  upsertLeaderboardProfile: async () => null,
  upsertLeaderboardProfileCredential: async () => null,
  upsertLeaderboardProfiles: async () => 0,
}));

mock.module("../../worker/durable/coordinator", () => ({
  coordinatorStub: (env: WorkerEnv) =>
    (env as WorkerEnv & { __coordinator: Record<string, unknown> }).__coordinator,
  asPublicJob: <T>(job: T): T => job,
  ProofCoordinatorDO: class ProofCoordinatorDO {},
}));

const workerModule = await import("../../worker/index");
const handler = workerModule.default;

function makeEnv(overrides: Partial<WorkerEnv> = {}): WorkerEnv {
  const assetsCalls: string[] = [];
  const env = {
    ASSETS: {
      fetch: async (request: Request) => {
        assetsCalls.push(request.url);
        return new Response("asset-ok", {
          status: 200,
          headers: {
            "cache-control": "public, max-age=3600",
          },
        });
      },
    } as Fetcher,
    PROOF_QUEUE: {
      send: async () => undefined,
    } as Queue<unknown>,
    CLAIM_QUEUE: {
      send: async () => undefined,
    } as Queue<unknown>,
    PROOF_COORDINATOR: {
      idFromName: () => "coordinator-id" as unknown as DurableObjectId,
      get: () =>
        ({
          getActiveJob: async () => null,
          getJob: async () => null,
          markFailed: async () => null,
          createJob: async () => ({ accepted: false, activeJob: null }),
          kickAlarm: async () => undefined,
        }) as DurableObjectStub,
    } as unknown as DurableObjectNamespace,
    PROOF_ARTIFACTS: {
      get: async () => null,
      put: async () => undefined,
      delete: async () => undefined,
    } as unknown as R2Bucket,
    PROVER_BASE_URL: "",
    __coordinator: {
      getActiveJob: async () => null,
      getJob: async () => null,
      markFailed: async () => null,
      createJob: async () => ({ accepted: false, activeJob: null }),
      kickAlarm: async () => undefined,
    },
    __assetsCalls: assetsCalls,
    ...overrides,
  } as WorkerEnv & {
    __coordinator: Record<string, unknown>;
    __assetsCalls: string[];
  };
  return env;
}

const noopExecutionContext = {
  waitUntil() {
    // no-op in tests
  },
  passThroughOnException() {
    // no-op in tests
  },
} as unknown as ExecutionContext;

async function requestWorker(path: string, init: RequestInit | undefined, env: WorkerEnv) {
  const request = new Request(`https://worker.test${path}`, init);
  return handler.fetch(request, env, noopExecutionContext);
}

describe("Worker live interface", () => {
  it("applies default no-store cache-control to /api routes without explicit policy", async () => {
    const response = await requestWorker("/api/health", undefined, makeEnv());
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe("no-store");
  });

  it("preserves public leaderboard cache policy for public reads", async () => {
    const response = await requestWorker("/api/leaderboard?window=all", undefined, makeEnv());
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe(LEADERBOARD_CACHE_CONTROL);
    expect(response.headers.get("etag")).toBeTruthy();
  });

  it("preserves private leaderboard cache policy for address-scoped reads", async () => {
    const response = await requestWorker(
      `/api/leaderboard?window=all&address=${VALID_CLAIMANT_CONTRACT}`,
      undefined,
      makeEnv(),
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe(LEADERBOARD_PRIVATE_CACHE_CONTROL);
  });

  it("supports conditional ETag revalidation on leaderboard route", async () => {
    const env = makeEnv();
    const first = await requestWorker("/api/leaderboard?window=all", undefined, env);
    expect(first.status).toBe(200);
    const etag = first.headers.get("etag");
    expect(etag).toBeTruthy();

    const second = await requestWorker(
      "/api/leaderboard?window=all",
      {
        headers: {
          "if-none-match": etag ?? "",
        },
      },
      env,
    );
    expect(second.status).toBe(304);
    expect(second.headers.get("cache-control")).toBe(LEADERBOARD_CACHE_CONTROL);
    expect(second.headers.get("etag")).toBe(etag);
  });

  it("returns API not-found payload for unknown /api route", async () => {
    const response = await requestWorker("/api/does-not-exist", undefined, makeEnv());
    expect(response.status).toBe(404);
    expect(response.headers.get("cache-control")).toBe("no-store");

    const payload = (await response.json()) as { success: boolean; error: string };
    expect(payload.success).toBe(false);
    expect(payload.error).toContain("unknown api route");
  });

  it("falls back to ASSETS fetch for non-api routes", async () => {
    const env = makeEnv() as WorkerEnv & { __assetsCalls: string[] };
    const response = await requestWorker("/leaderboard-ui", undefined, env);
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("asset-ok");
    expect(env.__assetsCalls.length).toBe(1);
    expect(env.__assetsCalls[0]?.endsWith("/leaderboard-ui")).toBe(true);
  });
});
