import { afterEach, describe, expect, it, vi } from "vitest";
import {
  fetchLeaderboard,
  LeaderboardUnavailableError,
  parseGame,
  parsePeriod,
} from "@/lib/api";

const responseBody = {
  period: "daily",
  from: "2026-08-20",
  through: "2026-08-20",
  game_id: null,
  entries: [],
};

describe("leaderboard API", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("uses the configured backend and existing versioned routes", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: vi.fn().mockResolvedValue(responseBody),
    });
    vi.stubGlobal("fetch", fetchMock);
    vi.stubEnv("MVP_API_URL", "https://api.example.test/");

    await expect(fetchLeaderboard("weekly", "overall")).resolves.toEqual(responseBody);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://api.example.test/v1/leaderboards/weekly",
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );

    await fetchLeaderboard("daily", "daily_fix");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "https://api.example.test/v1/leaderboards/games/daily_fix?period=daily",
      expect.any(Object),
    );
  });

  it("normalizes bad URL state and API failures", async () => {
    expect(parsePeriod("never")).toBe("daily");
    expect(parseGame(["daily_pr"])).toBe("daily_pr");

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("timeout")));
    await expect(fetchLeaderboard("daily", "overall")).rejects.toBeInstanceOf(
      LeaderboardUnavailableError,
    );
  });
});
