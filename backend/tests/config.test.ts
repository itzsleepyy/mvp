import { describe, expect, it } from "vitest";
import { readConfig } from "../src/config.js";

describe("readConfig", () => {
  it("uses safe defaults in test", () => {
    expect(
      readConfig({ NODE_ENV: "test", DATABASE_URL: "postgres://test" }),
    ).toMatchObject({
      nodeEnv: "test",
      port: 3000,
      sessionTtlDays: 30,
      trustProxy: false,
    });
  });

  it("requires production credentials", () => {
    expect(() =>
      readConfig({ NODE_ENV: "production", DATABASE_URL: "postgres://test" }),
    ).toThrow("GITHUB_CLIENT_ID");
  });

  it("rejects invalid numeric values", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        PORT: "zero",
      }),
    ).toThrow("PORT");
  });

  it("allows email authentication to be omitted", () => {
    expect(
      readConfig({ NODE_ENV: "test", DATABASE_URL: "postgres://test" }),
    ).toMatchObject({
      cloudflareAccountId: null,
      cloudflareEmailApiToken: null,
      emailFrom: null,
      websiteUrl: "http://localhost:3001",
    });
  });

  it("treats empty optional email settings as omitted", () => {
    expect(
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        CLOUDFLARE_ACCOUNT_ID: "",
        CLOUDFLARE_EMAIL_API_TOKEN: " ",
        EMAIL_FROM: "",
        WEBSITE_URL: "",
      }),
    ).toMatchObject({
      cloudflareAccountId: null,
      cloudflareEmailApiToken: null,
      emailFrom: null,
      websiteUrl: "http://localhost:3001",
    });
  });

  it("allows WEBSITE_URL without enabling email authentication", () => {
    expect(
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        WEBSITE_URL: "https://waitstate.example",
      }),
    ).toMatchObject({
      websiteUrl: "https://waitstate.example",
      emailFrom: null,
    });
  });

  it("requires all Cloudflare email settings together", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        CLOUDFLARE_ACCOUNT_ID: "account",
      }),
    ).toThrow("must be configured together");
  });

  it("requires WEBSITE_URL in production", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "production",
        DATABASE_URL: "postgres://test",
        GITHUB_CLIENT_ID: "client",
        SESSION_SECRET: "test-session-secret-at-least-32-chars",
      }),
    ).toThrow("WEBSITE_URL is required");
  });

  it("requires HTTPS for non-loopback production websites", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "production",
        DATABASE_URL: "postgres://test",
        GITHUB_CLIENT_ID: "client",
        SESSION_SECRET: "test-session-secret-at-least-32-chars",
        WEBSITE_URL: "http://waitstate.example",
      }),
    ).toThrow("WEBSITE_URL must use HTTPS in production");
  });

  it("allows loopback HTTP for the local production container", () => {
    expect(
      readConfig({
        NODE_ENV: "production",
        DATABASE_URL: "postgres://test",
        GITHUB_CLIENT_ID: "client",
        SESSION_SECRET: "test-session-secret-at-least-32-chars",
        WEBSITE_URL: "http://localhost:3001",
      }),
    ).toMatchObject({ websiteUrl: "http://localhost:3001" });
  });

  it("loads complete email authentication settings", () => {
    expect(
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        CLOUDFLARE_ACCOUNT_ID: "account",
        CLOUDFLARE_EMAIL_API_TOKEN: "secret-token",
        EMAIL_FROM: "login@waitstate.example",
        WEBSITE_URL: "https://waitstate.example/",
      }),
    ).toMatchObject({
      cloudflareAccountId: "account",
      cloudflareEmailApiToken: "secret-token",
      emailFrom: "login@waitstate.example",
      websiteUrl: "https://waitstate.example",
    });
  });
});
