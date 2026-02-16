import { describe, expect, it } from "bun:test";
import {
  API_CACHE_CONTROL,
  LEADERBOARD_CACHE_CONTROL,
  LEADERBOARD_PRIVATE_CACHE_CONTROL,
  applyApiCacheControl,
} from "../../worker/cache-control";

describe("applyApiCacheControl", () => {
  it("applies no-store when cache-control is missing", () => {
    const response = new Response(null, { status: 200 });
    applyApiCacheControl(response);
    expect(response.headers.get("cache-control")).toBe(API_CACHE_CONTROL);
  });

  it("preserves explicit cache-control from route handlers", () => {
    const response = new Response(null, {
      status: 200,
      headers: {
        "cache-control": LEADERBOARD_CACHE_CONTROL,
      },
    });
    applyApiCacheControl(response);
    expect(response.headers.get("cache-control")).toBe(LEADERBOARD_CACHE_CONTROL);
  });

  it("supports dedicated private leaderboard caching policy", () => {
    expect(LEADERBOARD_PRIVATE_CACHE_CONTROL).toContain("private");
    expect(LEADERBOARD_PRIVATE_CACHE_CONTROL).toContain("max-age=");
  });
});
