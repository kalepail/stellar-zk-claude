import { afterEach, describe, expect, it } from "bun:test";
import { fetchLeaderboardEventsFromGalexie } from "../../worker/leaderboard-ingestion";
import type { WorkerEnv } from "../../worker/env";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
    },
  });
}

function makeEnv(overrides: Partial<WorkerEnv> = {}): WorkerEnv {
  return {
    ASSETS: {} as Fetcher,
    PROOF_QUEUE: {} as Queue<unknown>,
    CLAIM_QUEUE: {} as Queue<unknown>,
    PROOF_COORDINATOR: {} as DurableObjectNamespace<never>,
    PROOF_ARTIFACTS: {} as R2Bucket,
    PROVER_BASE_URL: "http://127.0.0.1:8088",
    GALEXIE_RPC_BASE_URL: "https://rpc-pro.lightsail.network",
    ...overrides,
  } as WorkerEnv;
}

describe("leaderboard ingestion source selection", () => {
  it("defaults to rpc getEvents when source mode is not configured", async () => {
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "https://rpc-pro.lightsail.network/") {
        return jsonResponse({
          result: {
            events: [],
            cursor: "rpc-cursor-1",
          },
        });
      }
      return new Response(null, { status: 404 });
    }) as typeof fetch;

    const result = await fetchLeaderboardEventsFromGalexie(makeEnv(), {
      limit: 50,
    });

    expect(result.provider).toBe("rpc");
    expect(result.sourceMode).toBe("rpc");
    expect(result.nextCursor).toBe("rpc-cursor-1");
  });

  it("falls back from rpc to datalake in auto mode", async () => {
    let rpcGetEventsCalls = 0;
    globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "https://rpc-pro.lightsail.network/") {
        const payload = init?.body ? JSON.parse(String(init.body)) : {};
        const method = payload?.method;
        if (method === "getEvents") {
          rpcGetEventsCalls += 1;
          return new Response("rpc down", { status: 503 });
        }
        if (method === "getHealth") {
          return jsonResponse({ result: { latestLedger: 2 } });
        }
      }
      if (url.endsWith("/v1/.config.json")) {
        return jsonResponse({
          ledgersPerBatch: 1,
          batchesPerPartition: 1,
          compression: "zstd",
        });
      }
      if (url.includes(".xdr.zstd") || url.includes(".xdr.zst")) {
        return new Response(null, { status: 404 });
      }
      return new Response(null, { status: 404 });
    }) as typeof fetch;

    const result = await fetchLeaderboardEventsFromGalexie(
      makeEnv({
        GALEXIE_SOURCE_MODE: "auto",
        GALEXIE_API_BASE_URL: "https://galexie-pro.lightsail.network",
        GALEXIE_DATASTORE_ROOT_PATH: "/v1",
      }),
      {
        fromLedger: 2,
        toLedger: 2,
        limit: 1,
      },
    );

    expect(rpcGetEventsCalls).toBe(1);
    expect(result.provider).toBe("galexie");
    expect(result.sourceMode).toBe("datalake");
    expect(result.nextCursor).toBe("ledger:3");
  });

  it("falls back to rpc when datalake mode is configured but galexie is missing", async () => {
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === "https://rpc-pro.lightsail.network/") {
        return jsonResponse({
          result: {
            events: [],
            cursor: "rpc-cursor-2",
          },
        });
      }
      return new Response(null, { status: 404 });
    }) as typeof fetch;

    const result = await fetchLeaderboardEventsFromGalexie(
      makeEnv({
        GALEXIE_SOURCE_MODE: "datalake",
      }),
      {
        limit: 50,
      },
    );

    expect(result.provider).toBe("rpc");
    expect(result.sourceMode).toBe("rpc");
    expect(result.nextCursor).toBe("rpc-cursor-2");
  });
});
