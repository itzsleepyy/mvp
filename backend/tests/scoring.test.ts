import { describe, expect, it } from "vitest";
import { normalizeRun, validateChallenge } from "../src/scoring.js";

describe("normalization version 1", () => {
  it("caps Stack Overflow at 10,000", () => {
    expect(
      normalizeRun({
        gameId: "stack_overflow",
        rawScore: 12_350,
        durationMs: 1,
      }),
    ).toBe(10_000);
  });

  it("scores Daily PR guesses and whole speed seconds", () => {
    expect(
      normalizeRun({
        gameId: "daily_pr",
        rawScore: 1,
        durationMs: 0,
        solved: true,
      }),
    ).toBe(6_600);
    expect(
      normalizeRun({
        gameId: "daily_pr",
        rawScore: 6,
        durationMs: 599_999,
        solved: true,
      }),
    ).toBe(1_000);
    expect(
      normalizeRun({
        gameId: "daily_pr",
        rawScore: 6,
        durationMs: 0,
        solved: false,
      }),
    ).toBe(0);
  });

  it("scores Daily Fix from charged duration", () => {
    expect(
      normalizeRun({
        gameId: "daily_fix",
        rawScore: 60_000,
        durationMs: 60_000,
        solved: true,
        attempts: 0,
        hintUsed: false,
      }),
    ).toBe(4_000);
    expect(
      normalizeRun({
        gameId: "daily_fix",
        rawScore: 300_000,
        durationMs: 300_000,
        solved: true,
        attempts: 0,
        hintUsed: false,
      }),
    ).toBe(0);
    expect(() =>
      normalizeRun({
        gameId: "daily_fix",
        rawScore: 1,
        durationMs: 1,
        solved: false,
        attempts: 0,
        hintUsed: false,
      }),
    ).toThrow("completed results");
  });
});

describe("daily challenge identity", () => {
  const now = new Date("2026-08-20T23:59:00.000Z");

  it("accepts the current UTC deterministic identity", () => {
    expect(
      validateChallenge(
        "daily_pr",
        { date: "2026-08-20", version: 1, id: "daily_pr:v1:2026-08-20" },
        now,
      ),
    ).toEqual({
      date: "2026-08-20",
      version: 1,
      id: "daily_pr:v1:2026-08-20",
    });
  });

  it("rejects stale dates and mismatched IDs", () => {
    expect(() =>
      validateChallenge(
        "daily_fix",
        { date: "2026-08-19", version: 1, id: "daily_fix:v1:2026-08-19" },
        now,
      ),
    ).toThrow("current UTC date");
    expect(() =>
      validateChallenge(
        "daily_fix",
        { date: "2026-08-20", version: 1, id: "daily_pr:v1:2026-08-20" },
        now,
      ),
    ).toThrow("challenge.id");
  });
});
