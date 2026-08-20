import { randomUUID } from "node:crypto";
import { resolve } from "node:path";
import type { FastifyInstance } from "fastify";
import type pg from "pg";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { readConfig } from "../src/config.js";
import { createPool } from "../src/db.js";
import { migrate } from "../src/migrate.js";

const databaseUrl = process.env.TEST_DATABASE_URL ?? "";

describe.skipIf(!databaseUrl)("database API", () => {
  let db: pg.Pool;
  let app: FastifyInstance;
  const initialNow = new Date("2026-08-20T12:00:00.000Z");
  let now = initialNow;

  beforeAll(async () => {
    if (new URL(databaseUrl).pathname.replace(/^\//, "") !== "mvp_test")
      throw new Error(
        "TEST_DATABASE_URL must name a disposable mvp_test database",
      );
    await migrate(databaseUrl, resolve(import.meta.dirname, "../migrations"));
    db = createPool(databaseUrl);
    app = await buildApp({
      config: readConfig({ NODE_ENV: "test", DATABASE_URL: databaseUrl }),
      db,
      now: () => now,
      github: {
        start: () =>
          Promise.resolve({
            deviceCode: "github-device-secret",
            userCode: "ABCD-EFGH",
            verificationUri: "https://github.com/login/device",
            expiresIn: 900,
            interval: 5,
          }),
        poll: () =>
          Promise.resolve({
            status: "complete" as const,
            accessToken: "github-access-token",
          }),
        user: () =>
          Promise.resolve({
            id: 42,
            login: "octocat",
            avatarUrl: "https://avatars.example/octocat",
          }),
      },
    });
  });

  beforeEach(async () => {
    now = initialNow;
    await db.query(
      "TRUNCATE game_runs, github_device_flows, sessions, users CASCADE",
    );
  });

  afterAll(async () => {
    await app.close();
    await db.end();
  });

  async function token(username: string): Promise<string> {
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/test",
      payload: { username },
    });
    expect(response.statusCode).toBe(200);
    return response.json<{ token: string }>().token;
  }

  it("completes and replays encrypted GitHub Device Flow", async () => {
    const started = await app.inject({
      method: "POST",
      url: "/v1/auth/github/device",
    });
    expect(started.statusCode).toBe(201);
    const flow = started.json<{ flow_token: string }>();
    const stored = await db.query<{ device_code: string }>(
      "SELECT device_code FROM github_device_flows",
    );
    expect(stored.rows[0]?.device_code).not.toContain("github-device-secret");

    now = new Date(initialNow.getTime() + 5_000);
    const completed = await app.inject({
      method: "POST",
      url: "/v1/auth/github/poll",
      payload: { poll_token: flow.flow_token },
    });
    expect(completed.statusCode).toBe(200);
    const session = completed.json<{
      token: string;
      user: { username: string };
    }>();
    expect(session.user.username).toBe("octocat");

    const replayed = await app.inject({
      method: "POST",
      url: "/v1/auth/github/poll",
      payload: { poll_token: flow.flow_token },
    });
    expect(replayed.statusCode).toBe(200);
    expect(replayed.json<{ token: string }>().token).toBe(session.token);
    expect((await db.query("SELECT 1 FROM sessions")).rowCount).toBe(1);
  });

  it("authenticates, stores, normalizes, and idempotently returns a run", async () => {
    const bearer = await token("octocat");
    const id = randomUUID();
    const payload = {
      client_run_id: id,
      game_id: "daily_pr",
      raw_score: 2,
      duration_ms: 100_000,
      client_version: "0.4.0",
      result: { solved: true },
      challenge: {
        date: "2026-08-20",
        version: 1,
        id: "daily_pr:v1:2026-08-20",
      },
    };
    const created = await app.inject({
      method: "POST",
      url: "/v1/runs",
      headers: { authorization: `Bearer ${bearer}` },
      payload,
    });
    expect(created.statusCode).toBe(201);
    expect(created.json()).toMatchObject({
      client_run_id: id,
      normalized_score: 5_500,
      normalization_version: 1,
    });

    const duplicate = await app.inject({
      method: "POST",
      url: "/v1/runs",
      headers: { authorization: `Bearer ${bearer}` },
      payload,
    });
    expect(duplicate.statusCode).toBe(200);
    expect(duplicate.json<{ id: string }>().id).toBe(
      created.json<{ id: string }>().id,
    );
  });

  it("enforces one result for each daily game and date", async () => {
    const bearer = await token("hubot");
    const base = {
      game_id: "daily_fix",
      raw_score: 60_000,
      duration_ms: 60_000,
      client_version: "test",
      result: { solved: true, attempts: 0, hint_used: false },
      challenge: {
        date: "2026-08-20",
        version: 1,
        id: "daily_fix:v1:2026-08-20",
      },
    };
    const first = await app.inject({
      method: "POST",
      url: "/v1/runs",
      headers: { authorization: `Bearer ${bearer}` },
      payload: { ...base, client_run_id: randomUUID() },
    });
    const second = await app.inject({
      method: "POST",
      url: "/v1/runs",
      headers: { authorization: `Bearer ${bearer}` },
      payload: { ...base, client_run_id: randomUUID() },
    });
    expect(first.statusCode).toBe(201);
    expect(second.statusCode).toBe(409);
    expect(second.json()).toMatchObject({
      error: { code: "daily_result_exists" },
    });
  });

  it("uses competition rank and deterministic username order for ties", async () => {
    for (const username of ["zeta", "alpha"]) {
      const bearer = await token(username);
      await app.inject({
        method: "POST",
        url: "/v1/runs",
        headers: { authorization: `Bearer ${bearer}` },
        payload: {
          client_run_id: randomUUID(),
          game_id: "stack_overflow",
          raw_score: 100,
          duration_ms: 1,
          client_version: "test",
          result: {},
        },
      });
    }
    const leaderboard = await app.inject({
      method: "GET",
      url: "/v1/leaderboards/daily?date=2026-08-20",
    });
    expect(leaderboard.statusCode).toBe(200);
    expect(leaderboard.json()).toMatchObject({
      entries: [
        { rank: 1, points: 100, user: { username: "alpha" } },
        { rank: 1, points: 100, user: { username: "zeta" } },
      ],
    });
  });

  it("completes the local credential-free three-game Daily MVP flow", async () => {
    const bearer = await token("daily-mvp");
    const runs = [
      {
        client_run_id: randomUUID(),
        game_id: "stack_overflow",
        raw_score: 100,
        duration_ms: 1_000,
        client_version: "test",
        result: {},
      },
      {
        client_run_id: randomUUID(),
        game_id: "daily_pr",
        raw_score: 2,
        duration_ms: 100_000,
        client_version: "test",
        result: { solved: true },
        challenge: {
          date: "2026-08-20",
          version: 1,
          id: "daily_pr:v1:2026-08-20",
        },
      },
      {
        client_run_id: randomUUID(),
        game_id: "daily_fix",
        raw_score: 60_000,
        duration_ms: 60_000,
        client_version: "test",
        result: { solved: true, attempts: 0, hint_used: false },
        challenge: {
          date: "2026-08-20",
          version: 1,
          id: "daily_fix:v1:2026-08-20",
        },
      },
    ];
    for (const payload of runs) {
      const response = await app.inject({
        method: "POST",
        url: "/v1/runs",
        headers: { authorization: `Bearer ${bearer}` },
        payload,
      });
      expect(response.statusCode).toBe(201);
    }

    const leaderboard = await app.inject({
      method: "GET",
      url: "/v1/leaderboards/daily?date=2026-08-20",
    });
    expect(leaderboard.json()).toMatchObject({
      entries: [{ rank: 1, points: 9_600, user: { username: "daily-mvp" } }],
    });

    const profile = await app.inject({
      method: "GET",
      url: "/v1/me",
      headers: { authorization: `Bearer ${bearer}` },
    });
    expect(profile.json()).toMatchObject({
      username: "daily-mvp",
      stats: {
        daily_rank: 1,
        weekly_rank: 1,
        global_rank: 1,
        best_stack: 100,
        daily_pr_streak: 1,
        daily_fix_this_week: 1,
      },
    });
  });
});
