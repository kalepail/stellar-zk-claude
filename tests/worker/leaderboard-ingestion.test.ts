import { describe, expect, it } from "bun:test";
import { normalizeGalexieScoreEvents } from "../../worker/leaderboard-ingestion";

describe("normalizeGalexieScoreEvents", () => {
  it("normalizes ScoreSubmitted-style payloads", () => {
    const payload = {
      events: [
        {
          id: "evt-1",
          claimant: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4",
          seed: "42",
          frame_count: 2048,
          final_score: 1337,
          final_rng_state: 987654321,
          tape_checksum: 123456,
          rules_digest: 1095980083,
          previous_best: 1000,
          new_best: 1337,
          minted_delta: 337,
          journal_digest: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          tx_hash: "tx-1",
          event_index: 2,
          ledger: 777,
          closed_at: "2026-02-11T12:00:00.000Z",
        },
      ],
      next_cursor: "cursor-2",
    };

    const normalized = normalizeGalexieScoreEvents(payload, "2026-02-11T12:01:00.000Z");
    expect(normalized.fetchedCount).toBe(1);
    expect(normalized.events).toHaveLength(1);
    expect(normalized.nextCursor).toBe("cursor-2");
    expect(normalized.events[0]?.claimantAddress).toBe(
      "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4",
    );
    expect(normalized.events[0]?.frameCount).toBe(2048);
    expect(normalized.events[0]?.finalScore).toBe(1337);
    expect(normalized.events[0]?.finalRngState).toBe(987654321);
    expect(normalized.events[0]?.tapeChecksum).toBe(123456);
    expect(normalized.events[0]?.rulesDigest).toBe(1095980083);
    expect(normalized.events[0]?.newBest).toBe(1337);
    expect(normalized.events[0]?.source).toBe("galexie");
  });

  it("skips malformed events", () => {
    const payload = {
      events: [
        {
          id: "bad-1",
          claimant: "not-a-stellar-address",
          seed: 1,
          new_best: 10,
          closed_at: "2026-02-11T12:00:00.000Z",
        },
        {
          id: "bad-2",
          claimant: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEGWF",
          seed: 1,
          new_best: 0,
          closed_at: "2026-02-11T12:00:00.000Z",
        },
      ],
    };

    const normalized = normalizeGalexieScoreEvents(payload);
    expect(normalized.fetchedCount).toBe(2);
    expect(normalized.events).toHaveLength(0);
  });
});
