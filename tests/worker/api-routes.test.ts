import { describe, expect, it, mock } from "bun:test";
import type { WorkerEnv } from "../../worker/env";

const VALID_CLAIMANT_CONTRACT = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4";
const EXAMPLE_GENERATED_AT = "2026-02-14T00:00:00.000Z";
const EXAMPLE_INGESTION_STATE = {
  provider: "rpc" as const,
  sourceMode: "rpc" as const,
  cursor: "ledger:10",
  highestLedger: 10,
  lastSyncedAt: EXAMPLE_GENERATED_AT,
  lastBackfillAt: null,
  totalEvents: 12,
  lastError: null,
};
const EXAMPLE_ENTRY = {
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
  claimStatus: "succeeded" as const,
  claimTxHash: "tx-1",
  profile: {
    claimantAddress: VALID_CLAIMANT_CONTRACT,
    username: "pilot",
    linkUrl: null,
    updatedAt: EXAMPLE_GENERATED_AT,
  },
};

mock.module("../../worker/leaderboard-store", () => ({
  countLeaderboardEvents: async () => EXAMPLE_INGESTION_STATE.totalEvents,
  createLeaderboardProfileAuthChallenge: async () => undefined,
  getLeaderboardIngestionState: async () => EXAMPLE_INGESTION_STATE,
  getLeaderboardPage: async () => ({
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
    entries: [EXAMPLE_ENTRY],
    me: EXAMPLE_ENTRY,
  }),
  getLeaderboardPlayer: async () => ({
    profile: EXAMPLE_ENTRY.profile,
    stats: {
      totalRuns: 1,
      bestScore: EXAMPLE_ENTRY.score,
      totalMinted: EXAMPLE_ENTRY.mintedDelta,
      lastPlayedAt: EXAMPLE_GENERATED_AT,
    },
    ranks: {
      tenMin: 1,
      day: 1,
      all: 1,
    },
    recentRuns: [
      {
        jobId: EXAMPLE_ENTRY.jobId,
        claimantAddress: EXAMPLE_ENTRY.claimantAddress,
        score: EXAMPLE_ENTRY.score,
        mintedDelta: EXAMPLE_ENTRY.mintedDelta,
        seed: EXAMPLE_ENTRY.seed,
        frameCount: EXAMPLE_ENTRY.frameCount,
        finalRngState: EXAMPLE_ENTRY.finalRngState,
        tapeChecksum: EXAMPLE_ENTRY.tapeChecksum,
        rulesDigest: EXAMPLE_ENTRY.rulesDigest,
        completedAt: EXAMPLE_ENTRY.completedAt,
        claimStatus: "succeeded" as const,
        claimTxHash: EXAMPLE_ENTRY.claimTxHash,
      },
    ],
  }),
  getLeaderboardProfileAuthChallenge: async () => null,
  getLeaderboardProfileCredential: async () => null,
  markLeaderboardProfileAuthChallengeUsed: async () => false,
  purgeExpiredLeaderboardProfileAuthChallenges: async () => undefined,
  setLeaderboardIngestionState: async () => undefined,
  updateLeaderboardProfileCredentialCounter: async () => undefined,
  upsertLeaderboardEvents: async () => ({ inserted: 0, updated: 0 }),
  upsertLeaderboardProfile: async () => EXAMPLE_ENTRY.profile,
  upsertLeaderboardProfileCredential: async () => ({
    claimantAddress: VALID_CLAIMANT_CONTRACT,
    credentialId: "credential-1",
    publicKey: "public-key",
    counter: 0,
    transports: null,
    createdAt: EXAMPLE_GENERATED_AT,
    updatedAt: EXAMPLE_GENERATED_AT,
  }),
  upsertLeaderboardProfiles: async () => 0,
}));

mock.module("../../worker/leaderboard-sync", () => ({
  runScheduledLeaderboardSync: async () => ({
    enabled: false,
    warning: null,
  }),
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
      total_events: EXAMPLE_INGESTION_STATE.totalEvents,
    },
    state: EXAMPLE_INGESTION_STATE,
  }),
  recordLeaderboardSyncFailure: async () => undefined,
}));

mock.module("../../worker/durable/coordinator", () => ({
  coordinatorStub: (env: WorkerEnv) =>
    (env as WorkerEnv & { __coordinator: Record<string, unknown> }).__coordinator,
  asPublicJob: <T>(job: T): T => job,
  ProofCoordinatorDO: class ProofCoordinatorDO {},
}));

const { createApiRouter } = await import("../../worker/api/routes");

const noopExecutionContext = {
  waitUntil() {
    // no-op in tests
  },
  passThroughOnException() {
    // no-op in tests
  },
} as unknown as ExecutionContext;

function makeCoordinatorStub(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    getActiveJob: async () => null,
    getJob: async () => null,
    markFailed: async () => null,
    createJob: async () => ({ accepted: false, activeJob: null }),
    kickAlarm: async () => undefined,
    getLeaderboardIngestionState: async () => ({
      provider: "rpc",
      sourceMode: "rpc",
      cursor: null,
      highestLedger: null,
      lastSyncedAt: null,
      lastBackfillAt: null,
      totalEvents: 0,
      lastError: null,
    }),
    listLeaderboardEventsPage: async () => ({
      events: [],
      nextStartAfter: null,
      done: true,
    }),
    listLeaderboardProfilesPage: async () => ({
      profiles: [],
      nextStartAfter: null,
      done: true,
    }),
    ...overrides,
  };
}

function makeEnv(
  overrides: (Partial<WorkerEnv> & { __coordinator?: Record<string, unknown> }) | undefined = {},
): WorkerEnv {
  const coordinator = overrides.__coordinator ?? makeCoordinatorStub();

  return {
    ASSETS: {
      fetch: async () => new Response("not found", { status: 404 }),
    } as Fetcher,
    PROOF_QUEUE: {
      send: async () => undefined,
    } as Queue<unknown>,
    CLAIM_QUEUE: {
      send: async () => undefined,
    } as Queue<unknown>,
    PROOF_COORDINATOR: {
      idFromName: () => "coordinator-id" as unknown as DurableObjectId,
      get: () => coordinator,
    } as unknown as DurableObjectNamespace,
    PROOF_ARTIFACTS: {
      get: async () => null,
      put: async () => undefined,
      delete: async () => undefined,
    } as unknown as R2Bucket,
    PROVER_BASE_URL: "",
    __coordinator: coordinator,
    ...overrides,
  } as WorkerEnv & { __coordinator: Record<string, unknown> };
}

async function requestApi(
  path: string,
  init: RequestInit | undefined,
  env: WorkerEnv,
): Promise<Response> {
  const router = createApiRouter();
  const request = new Request(`https://worker.test${path}`, init);
  return router.fetch(request, env, noopExecutionContext);
}

describe("API routes", () => {
  it("GET /health returns degraded prover status when health validation fails", async () => {
    const response = await requestApi("/health", undefined, makeEnv({ PROVER_BASE_URL: "" }));
    expect(response.status).toBe(200);

    const payload = (await response.json()) as {
      success: boolean;
      prover: { status: string };
    };
    expect(payload.success).toBe(true);
    expect(payload.prover.status).toBe("degraded");
  });

  it("GET /leaderboard/sync/status requires leaderboard admin key", async () => {
    const response = await requestApi(
      "/leaderboard/sync/status",
      undefined,
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(401);
  });

  it("GET /leaderboard/sync/status returns ingestion state for admins", async () => {
    const response = await requestApi(
      "/leaderboard/sync/status",
      {
        headers: {
          "x-leaderboard-admin-key": "secret-admin-key",
        },
      },
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(200);

    const payload = (await response.json()) as {
      success: boolean;
      event_count: number;
      source_mode: string;
    };
    expect(payload.success).toBe(true);
    expect(payload.event_count).toBe(EXAMPLE_INGESTION_STATE.totalEvents);
    expect(payload.source_mode).toBe(EXAMPLE_INGESTION_STATE.sourceMode);
  });

  it("POST /leaderboard/sync validates mode", async () => {
    const response = await requestApi(
      "/leaderboard/sync",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-leaderboard-admin-key": "secret-admin-key",
        },
        body: JSON.stringify({ mode: "invalid-mode" }),
      },
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(400);
  });

  it("POST /leaderboard/sync succeeds for valid admin requests", async () => {
    const response = await requestApi(
      "/leaderboard/sync",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-leaderboard-admin-key": "secret-admin-key",
        },
        body: JSON.stringify({ mode: "forward", limit: 10 }),
      },
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(200);
  });

  it("POST /leaderboard/migrate/do-to-d1 requires migration confirmation header", async () => {
    const response = await requestApi(
      "/leaderboard/migrate/do-to-d1",
      {
        method: "POST",
        headers: {
          "x-leaderboard-admin-key": "secret-admin-key",
        },
      },
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(400);
  });

  it("POST /leaderboard/migrate/do-to-d1 succeeds with admin and confirmation headers", async () => {
    const response = await requestApi(
      "/leaderboard/migrate/do-to-d1",
      {
        method: "POST",
        headers: {
          "x-leaderboard-admin-key": "secret-admin-key",
          "x-migration-confirm": "do-to-d1",
        },
      },
      makeEnv({ LEADERBOARD_ADMIN_KEY: "secret-admin-key" }),
    );
    expect(response.status).toBe(200);
  });

  it("GET /leaderboard validates window query", async () => {
    const response = await requestApi("/leaderboard?window=bad-window", undefined, makeEnv());
    expect(response.status).toBe(400);
  });

  it("GET /leaderboard returns leaderboard page for valid queries", async () => {
    const response = await requestApi("/leaderboard?window=all&limit=25&offset=0", undefined, makeEnv());
    expect(response.status).toBe(200);

    const payload = (await response.json()) as {
      success: boolean;
      entries: unknown[];
      pagination: { total: number };
    };
    expect(payload.success).toBe(true);
    expect(payload.entries.length).toBe(1);
    expect(payload.pagination.total).toBe(1);
  });

  it("GET /leaderboard/player/:claimantAddress validates claimant address", async () => {
    const response = await requestApi("/leaderboard/player/not-a-valid-claimant", undefined, makeEnv());
    expect(response.status).toBe(400);
  });

  it("GET /leaderboard/player/:claimantAddress returns player summary", async () => {
    const response = await requestApi(
      `/leaderboard/player/${VALID_CLAIMANT_CONTRACT}`,
      undefined,
      makeEnv(),
    );
    expect(response.status).toBe(200);

    const payload = (await response.json()) as {
      success: boolean;
      player: { claimant_address: string };
    };
    expect(payload.success).toBe(true);
    expect(payload.player.claimant_address).toBe(VALID_CLAIMANT_CONTRACT);
  });

  it("POST /leaderboard/player/:claimantAddress/profile/auth/options validates payload", async () => {
    const response = await requestApi(
      `/leaderboard/player/${VALID_CLAIMANT_CONTRACT}/profile/auth/options`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
        },
        body: JSON.stringify({}),
      },
      makeEnv(),
    );
    expect(response.status).toBe(400);
  });

  it("PUT /leaderboard/player/:claimantAddress/profile validates auth payload", async () => {
    const response = await requestApi(
      `/leaderboard/player/${VALID_CLAIMANT_CONTRACT}/profile`,
      {
        method: "PUT",
        headers: {
          "content-type": "application/json",
        },
        body: JSON.stringify({ username: "player-one" }),
      },
      makeEnv(),
    );
    expect(response.status).toBe(400);
  });

  it("POST /proofs/jobs enforces MAX_TAPE_BYTES before reading body", async () => {
    const response = await requestApi(
      "/proofs/jobs",
      {
        method: "POST",
        headers: {
          "content-length": "10",
        },
        body: "0123456789",
      },
      makeEnv({ MAX_TAPE_BYTES: "5" }),
    );
    expect(response.status).toBe(413);
  });

  it("GET /proofs/jobs/:jobId returns 404 when job does not exist", async () => {
    const response = await requestApi("/proofs/jobs/job-missing", undefined, makeEnv());
    expect(response.status).toBe(404);
  });

  it("GET /proofs/jobs/:jobId returns job when present", async () => {
    const response = await requestApi(
      "/proofs/jobs/job-present",
      undefined,
      makeEnv({
        __coordinator: makeCoordinatorStub({
          getJob: async () => ({
            jobId: "job-present",
            status: "failed",
          }),
        }),
      }),
    );
    expect(response.status).toBe(200);
  });

  it("GET /proofs/jobs/:jobId/result returns 404 when result artifact is not found", async () => {
    const response = await requestApi("/proofs/jobs/job-missing/result", undefined, makeEnv());
    expect(response.status).toBe(404);
  });

  it("GET /proofs/jobs/:jobId/result returns artifact payload when present", async () => {
    const response = await requestApi(
      "/proofs/jobs/job-present/result",
      undefined,
      makeEnv({
        __coordinator: makeCoordinatorStub({
          getJob: async () => ({
            result: { artifactKey: "results/job-present.json" },
          }),
        }),
        PROOF_ARTIFACTS: {
          get: async () =>
            ({
              body: '{"success":true}',
            }) as unknown as R2ObjectBody,
          put: async () => undefined,
          delete: async () => undefined,
        } as unknown as R2Bucket,
      }),
    );
    expect(response.status).toBe(200);
    expect(await response.text()).toBe('{"success":true}');
  });

  it("DELETE /proofs/jobs/:jobId returns 404 when job does not exist", async () => {
    const response = await requestApi(
      "/proofs/jobs/job-missing",
      {
        method: "DELETE",
      },
      makeEnv(),
    );
    expect(response.status).toBe(404);
  });

  it("DELETE /proofs/jobs/:jobId marks job as failed when present", async () => {
    const response = await requestApi(
      "/proofs/jobs/job-present",
      {
        method: "DELETE",
      },
      makeEnv({
        __coordinator: makeCoordinatorStub({
          markFailed: async () => ({
            jobId: "job-present",
            status: "failed",
          }),
        }),
      }),
    );
    expect(response.status).toBe(200);
  });
});
