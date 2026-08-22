export const GAME_IDS = ["stack_overflow", "daily_pr", "daily_fix"] as const;
export type GameId = (typeof GAME_IDS)[number];

export interface RunForNormalization {
  gameId: GameId;
  rawScore: number;
  durationMs: number;
  solved?: boolean;
  attempts?: number;
  hintUsed?: boolean;
}

export function normalizeRun(run: RunForNormalization): number {
  if (!Number.isInteger(run.rawScore) || run.rawScore < 0)
    throw new Error("raw_score must be a non-negative integer");
  if (!Number.isInteger(run.durationMs) || run.durationMs < 0)
    throw new Error("duration_ms must be a non-negative integer");

  switch (run.gameId) {
    case "stack_overflow":
      if (run.rawScore > 100_000 || run.rawScore % 50 !== 0)
        throw new Error(
          "stack_overflow raw_score must be at most 100000 and divisible by 50",
        );
      if (run.rawScore > 0 && run.durationMs === 0)
        throw new Error("a scored stack_overflow run must have a duration");
      return Math.min(run.rawScore, 10_000);
    case "daily_pr":
      if (typeof run.solved !== "boolean")
        throw new Error("result.solved is required");
      if (run.rawScore < 1 || run.rawScore > 6)
        throw new Error("daily_pr raw_score must be guesses from 1 to 6");
      if (run.durationMs > 3_600_000)
        throw new Error("daily_pr duration exceeds one hour");
      if (!run.solved && run.rawScore !== 6)
        throw new Error("an unsolved daily_pr result must use all 6 guesses");
      if (!run.solved) return 0;
      return Math.min(
        6_600,
        (7 - run.rawScore) * 1_000 +
          Math.floor(Math.max(0, 600_000 - run.durationMs) / 1_000),
      );
    case "daily_fix": {
      if (typeof run.solved !== "boolean")
        throw new Error("result.solved is required");
      if (!run.solved)
        throw new Error("daily_fix only accepts completed results");
      if (
        !Number.isInteger(run.attempts) ||
        run.attempts === undefined ||
        run.attempts < 0 ||
        run.attempts > 100
      )
        throw new Error("daily_fix result.attempts must be from 0 to 100");
      if (typeof run.hintUsed !== "boolean")
        throw new Error("daily_fix result.hint_used is required");
      if (run.hintUsed !== run.attempts >= 2)
        throw new Error("daily_fix hint usage does not match attempts");
      if (run.rawScore !== run.durationMs)
        throw new Error("daily_fix raw_score must equal charged duration_ms");
      const minimumCharged = run.attempts * 5_000 + (run.hintUsed ? 15_000 : 0);
      if (run.rawScore < minimumCharged || run.rawScore > 3_600_000)
        throw new Error("daily_fix charged duration is impossible");
      return Math.max(
        0,
        5_000 - Math.floor(Math.min(run.rawScore, 300_000) / 60),
      );
    }
  }
}

export function utcDate(now: Date): string {
  return now.toISOString().slice(0, 10);
}

export function validateChallenge(
  gameId: GameId,
  challenge: unknown,
  now: Date,
): { date: string; version: number; id: string } | null {
  if (gameId === "stack_overflow") {
    if (challenge !== undefined)
      throw new Error("stack_overflow must not include a challenge");
    return null;
  }
  if (!challenge || typeof challenge !== "object")
    throw new Error("challenge is required for daily games");
  const value = challenge as Record<string, unknown>;
  const date = utcDate(now);
  const expectedId = `${gameId}:v1:${date}`;
  if (value.date !== date)
    throw new Error("challenge.date must be the current UTC date");
  if (value.version !== 1) throw new Error("challenge.version must be 1");
  if (value.id !== expectedId)
    throw new Error(`challenge.id must be ${expectedId}`);
  return { date, version: 1, id: expectedId };
}
