import { afterEach, describe, expect, it, vi } from "vitest";
import type { FastifyInstance } from "fastify";
import { buildApp } from "../src/app.js";
import { readConfig } from "../src/config.js";
import type { Database } from "../src/db.js";

const config = readConfig({
  NODE_ENV: "production",
  DATABASE_URL: "postgres://unused",
  GITHUB_CLIENT_ID: "test-client",
  SESSION_SECRET: "test-session-secret-at-least-32-chars",
  WEBSITE_URL: "https://waitstate.example",
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

  it("returns 503 when email authentication is not configured", async () => {
    app = await buildApp({ config, db });
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/email/start",
      payload: { email: "player@example.com" },
    });
    expect(response.statusCode).toBe(503);
    expect(response.json()).toMatchObject({
      error: { code: "email_auth_unavailable" },
    });
  });

  it("creates separate private poll and browser handoff tokens", async () => {
    const query = vi.fn().mockResolvedValue({ rows: [] });
    app = await buildApp({
      config,
      db: { query } as unknown as Database,
    });
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/handoff/start",
    });
    expect(response.statusCode).toBe(201);
    const body = response.json<{
      flow_token: string;
      verification_uri: string;
    }>();
    const browserToken = new URL(body.verification_uri).searchParams.get(
      "handoff",
    );
    expect(browserToken).not.toBeNull();
    expect(browserToken).not.toBe(body.flow_token);
    const parameters = query.mock.calls[0]?.[1] as unknown[];
    expect(parameters).not.toContain(body.flow_token);
    expect(parameters).not.toContain(browserToken);
    expect(parameters[1]).not.toBe(parameters[2]);
  });

  it("rejects unauthenticated handoff completion before SQL", async () => {
    app = await buildApp({ config, db });
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/handoff/complete",
      payload: { browser_token: "b".repeat(32) },
    });
    expect(response.statusCode).toBe(401);
  });

  it("adds a valid handoff token to an email verification URL", async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const query = vi
      .fn()
      .mockResolvedValueOnce({
        rows: [
          {
            expires_at: new Date(Date.now() + 60_000),
            completed_user_id: null,
          },
        ],
      })
      .mockResolvedValueOnce({ rows: [] });
    app = await buildApp({
      config: { ...config, emailFrom: "login@waitstate.example" },
      db: { query } as unknown as Database,
      email: { send },
    });
    const browserToken = "b".repeat(32);
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/email/start",
      payload: { email: "player@example.com", browser_token: browserToken },
    });
    expect(response.statusCode).toBe(201);
    const message = send.mock.calls[0]?.[0] as { text: string };
    const link = message.text.match(/https:\/\/\S+/)?.[0] ?? "";
    expect(new URL(link).searchParams.get("handoff")).toBe(browserToken);
  });

  it("deletes the flow and returns 503 when email delivery fails", async () => {
    const query = vi.fn().mockResolvedValue({ rows: [] });
    app = await buildApp({
      config: {
        ...config,
        cloudflareAccountId: "account",
        cloudflareEmailApiToken: "token",
        emailFrom: "login@waitstate.example",
        websiteUrl: "https://waitstate.example",
      },
      db: { query } as unknown as Database,
      email: { send: () => Promise.reject(new Error("provider failed")) },
    });
    const response = await app.inject({
      method: "POST",
      url: "/v1/auth/email/start",
      payload: { email: " Player@Example.com " },
    });
    expect(response.statusCode).toBe(503);
    expect(query).toHaveBeenCalledTimes(2);
    expect(String(query.mock.calls[1]?.[0])).toContain(
      "DELETE FROM email_auth_flows",
    );
  });
});
