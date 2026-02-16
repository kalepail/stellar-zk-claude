export interface RelayClaimRequest {
  jobId: string;
  claimantAddress: string;
  journalRawHex: string;
  journalDigestHex: string;
  proverResponse: unknown;
}

export type RelaySubmitResult =
  | { type: "success"; txHash: string }
  | { type: "retry"; message: string }
  | { type: "fatal"; message: string };
