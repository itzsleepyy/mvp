import { afterEach, describe, expect, it } from "vitest";
import type { FastifyInstance } from "fastify";
import { buildApp } from "../src/app.js";
import { readConfig } from "../src/config.js";
import type { Database } from "../src/db.js";

const config = readConfig({
  NODE_ENV: "production",
  DATABASE_URL: "postgres://unused",
  GITHUB_CLIENT_ID: "test-client",
  SESSION_SECRET: "test-session-secret-at-least-32-chars",
  LOG_LEVEL: "silent",
});
const db = {
  query: () => Promise.reject(new Error("database should not be called")),
} as unknown as Database;

describe("HTTP boundaries", () => {
  let app: FastifyInstance | undefined;

  afterEach(async () => app?.close());

  it("returns the stable 429 envelope", async () => {
    app = await buildApp({ config, db });
    for (let index = 0; index < 120; index += 1) {
      expect((await app.inject({ url: "/health" })).statusCode).toBe(200);
    }
    const limited = await app.inject({ url: "/health" });
    expect(limited.statusCode).toBe(429);
    expect(limited.json()).toEqual({
      error: {
        code: "rate_limited",
        message: "Too many requests; try again later",
      },
    });
  });

  it("rejects impossible calendar dates before SQL", async () => {
    app = await buildApp({ config: { ...config, nodeEnv: "test" }, db });
    const response = await app.inject({
      url: "/v1/leaderboards/daily?date=2026-99-99",
    });
    expect(response.statusCode).toBe(400);
    expect(response.json()).toMatchObject({ error: { code: "invalid_date" } });
  });
});
