import { describe, expect, it } from "bun:test";
import {
  normalizeClaimantStrKeyInput,
  parseClaimantStrKeyFromUserInput,
} from "../../shared/stellar/strkey";

const SAMPLE_G = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEGWF";
const SAMPLE_C = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4";

describe("claimant strkey parsing", () => {
  it("accepts G account addresses", () => {
    const parsed = parseClaimantStrKeyFromUserInput(SAMPLE_G);
    expect(parsed.normalized).toBe(SAMPLE_G);
    expect(parsed.type).toBe("account");
  });

  it("accepts C contract addresses", () => {
    const parsed = parseClaimantStrKeyFromUserInput(SAMPLE_C);
    expect(parsed.normalized).toBe(SAMPLE_C);
    expect(parsed.type).toBe("contract");
  });

  it("normalizes mixed-case input to canonical uppercase", () => {
    const mixed = `  ${SAMPLE_G.toLowerCase()}  `;
    expect(normalizeClaimantStrKeyInput(mixed)).toBe(SAMPLE_G);

    const parsed = parseClaimantStrKeyFromUserInput(mixed);
    expect(parsed.normalized).toBe(SAMPLE_G);
    expect(parsed.type).toBe("account");
  });
});
