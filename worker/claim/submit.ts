import type { WorkerEnv } from "../env";
import { isDirectClaimConfigured, submitClaimDirect } from "./direct";
import type { RelayClaimRequest, RelaySubmitResult } from "./types";

export async function submitClaim(
  env: WorkerEnv,
  request: RelayClaimRequest,
): Promise<RelaySubmitResult> {
  if (!isDirectClaimConfigured(env)) {
    return {
      type: "fatal",
      message:
        "claim submission is not configured; set SCORE_CONTRACT_ID, RELAYER_URL, and RELAYER_API_KEY for relayer-only submission",
    };
  }

  return submitClaimDirect(env, request);
}
