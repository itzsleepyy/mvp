import { afterEach, describe, expect, it, vi } from "vitest";
import {
  SESSION_COOKIE,
  sessionCookieOptions,
} from "@/lib/auth-cookie";

describe("server auth helper", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("stores the bearer in a production-secure, server-only cookie", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const expiresAt = "2026-09-19T12:00:00.000Z";

    expect(SESSION_COOKIE).toBe("mvp_session");
    expect(sessionCookieOptions(new Date(expiresAt))).toEqual({
      httpOnly: true,
      sameSite: "lax",
      secure: true,
      path: "/",
      expires: new Date(expiresAt),
    });
  });
});
