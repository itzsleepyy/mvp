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
});
