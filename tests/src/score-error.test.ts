import { describe, expect, it } from "bun:test";
import { explainScoreSubmissionError } from "../../src/chain/score";

describe("explainScoreSubmissionError", () => {
  it("maps duplicate journal rejection to a friendly message", () => {
    const raw =
      'Transaction simulation failed: "HostError: Error(Contract, #3) Event log (newest first): ..."';
    const message = explainScoreSubmissionError(raw);
    expect(message).toContain("already claimed");
  });

  it("maps score-not-improved rejection to a friendly message", () => {
    const raw = "Error(Contract, #5)";
    const message = explainScoreSubmissionError(raw);
    expect(message).toContain("not improved");
  });

  it("maps fetch transport failures to a network/relayer hint", () => {
    const message = explainScoreSubmissionError("Failed to fetch");
    expect(message).toContain("network/relayer");
  });
});
