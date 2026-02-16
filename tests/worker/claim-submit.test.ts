import { describe, expect, it } from "bun:test";
import { submitClaim } from "../../worker/claim/submit";
import type { WorkerEnv } from "../../worker/env";

const BASE_REQUEST = {
  jobId: "job-1",
  claimantAddress: "GCHPTWXMT3HYF4RLZHWBNRF4MPXLTJ76ISHMSYIWCCDXWUYOQG5MR2AB",
  journalRawHex: "00",
  journalDigestHex: "11",
  proverResponse: {},
};

describe("submitClaim relayer-only config handling", () => {
  it("fails when relayer-only env is not configured", async () => {
    const env = {
      RELAYER_URL: "",
      SCORE_CONTRACT_ID: "",
    } as WorkerEnv;

    const result = await submitClaim(env, BASE_REQUEST);
    expect(result.type).toBe("fatal");
    expect(result.message).toContain("claim submission is not configured");
  });
});
