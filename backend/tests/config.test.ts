import { describe, expect, it } from "vitest";
import { readConfig } from "../src/config.js";

describe("readConfig", () => {
  it("uses safe defaults in test", () => {
    expect(
      readConfig({ NODE_ENV: "test", DATABASE_URL: "postgres://test" }),
    ).toMatchObject({
      nodeEnv: "test",
      port: 3000,
      emailAuthEnabled: false,
      sessionTtlDays: 30,
      trustProxy: false,
      minimumClientVersion: "0.1.0",
      latestClientVersion: "0.1.0",
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

  it("loads and validates client versions", () => {
    expect(
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        MINIMUM_CLIENT_VERSION: "1.2.0",
        LATEST_CLIENT_VERSION: "1.4.3",
      }),
    ).toMatchObject({
      minimumClientVersion: "1.2.0",
      latestClientVersion: "1.4.3",
    });
    expect(() =>
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        MINIMUM_CLIENT_VERSION: "latest",
      }),
    ).toThrow("semantic versions");
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

  it("rejects an invalid email authentication flag", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        EMAIL_AUTH_ENABLED: "yes",
      }),
    ).toThrow("EMAIL_AUTH_ENABLED must be true or false");
  });

  it("requires Cloudflare settings when email authentication is enabled", () => {
    expect(() =>
      readConfig({
        NODE_ENV: "test",
        DATABASE_URL: "postgres://test",
        EMAIL_AUTH_ENABLED: "true",
      }),
    ).toThrow("Cloudflare email settings are required");
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
        EMAIL_AUTH_ENABLED: "true",
        CLOUDFLARE_ACCOUNT_ID: "account",
        CLOUDFLARE_EMAIL_API_TOKEN: "secret-token",
        EMAIL_FROM: "login@waitstate.example",
        WEBSITE_URL: "https://waitstate.example/",
      }),
    ).toMatchObject({
      emailAuthEnabled: true,
      cloudflareAccountId: "account",
      cloudflareEmailApiToken: "secret-token",
      emailFrom: "login@waitstate.example",
      websiteUrl: "https://waitstate.example",
    });
  });
});
