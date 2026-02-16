import { describe, expect, it } from "bun:test";
import { Asset, Networks } from "@stellar/stellar-sdk";
import { parseSacAssetFromName } from "../../src/chain/score";

const SCORE_SAC_NAME = "SCORE:GCHPTWXMT3HYF4RLZHWBNRF4MPXLTJ76ISHMSYIWCCDXWUYOQG5MR2AB";
const SCORE_TOKEN_CONTRACT_ID = "CBUCDXT6BY3WWP764AMW66QJA6ZRWL2TRV6VTYCWPZF4FUZRAXK2S253";

describe("parseSacAssetFromName", () => {
  it("parses native SAC names", () => {
    const asset = parseSacAssetFromName("native");
    expect(asset.toString()).toBe("native");
  });

  it("parses code:issuer SAC names and derives the known SAC contract id", () => {
    const asset = parseSacAssetFromName(SCORE_SAC_NAME);
    expect(asset.toString()).toBe(SCORE_SAC_NAME);
    expect(asset.contractId(Networks.TESTNET)).toBe(SCORE_TOKEN_CONTRACT_ID);
  });

  it("rejects contract-id strings as asset names", () => {
    expect(() => parseSacAssetFromName(SCORE_TOKEN_CONTRACT_ID)).toThrow(
      'invalid stellar asset name "CBUCDXT6BY3WWP764AMW66QJA6ZRWL2TRV6VTYCWPZF4FUZRAXK2S253"',
    );
  });
});

describe("Asset constructor contract-id handling", () => {
  it("does not accept a C-address as issuer", () => {
    expect(() => new Asset("SCORE", SCORE_TOKEN_CONTRACT_ID)).toThrow("Issuer is invalid");
  });

  it("does not accept a C-address as code", () => {
    expect(() => new Asset(SCORE_TOKEN_CONTRACT_ID)).toThrow("Asset code is invalid");
  });
});
