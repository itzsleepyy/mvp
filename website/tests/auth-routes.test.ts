import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const auth = vi.hoisted(() => ({
  backendFetch: vi.fn(),
  completeBrowserHandoff: vi.fn(),
  setSessionCookie: vi.fn(),
  readSessionToken: vi.fn(),
  clearSessionCookie: vi.fn(),
  errorResponse: (status: number) => Response.json({ error: "safe" }, { status }),
}));

vi.mock("@/lib/auth", () => auth);

import { POST as pollGithub } from "@/app/api/auth/github/poll/route";
import { POST as startEmail } from "@/app/api/auth/email/start/route";
import { POST as completeHandoff } from "@/app/api/auth/handoff/complete/route";
import { GET as getSession } from "@/app/api/auth/session/route";
import { POST as logout } from "@/app/api/auth/logout/route";
import { GET as verifyEmail } from "@/app/auth/email/verify/route";

describe("auth route handlers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubEnv("EMAIL_AUTH_ENABLED", "true");
  });
  afterEach(() => vi.unstubAllEnvs());

  it("stores a completed GitHub session without returning its bearer token", async () => {
    const session = {
      token: "secret-bearer-token",
      expires_at: "2026-09-19T12:00:00.000Z",
      user: {
        id: "1",
        username: "octocat",
        display_name: "Octocat",
        avatar_url: null,
      },
    };
    auth.backendFetch.mockResolvedValue(Response.json(session));

    const response = await pollGithub(
      new Request("http://localhost/api/auth/github/poll", {
        method: "POST",
        body: JSON.stringify({ poll_token: "flow-token" }),
      }),
    );
    const body = await response.json();

    expect(auth.setSessionCookie).toHaveBeenCalledWith(session);
    expect(body).toEqual({ status: "complete", user: session.user });
    expect(JSON.stringify(body)).not.toContain(session.token);
  });

  it("completes a GitHub handoff before storing the web session", async () => {
    const session = {
      token: "secret-bearer-token",
      expires_at: "2026-09-19T12:00:00.000Z",
      user: { id: "1", username: "octocat" },
    };
    auth.backendFetch.mockResolvedValue(Response.json(session));
    auth.completeBrowserHandoff.mockResolvedValue(
      new Response(null, { status: 204 }),
    );

    const response = await pollGithub(
      new Request("http://localhost/api/auth/github/poll", {
        method: "POST",
        body: JSON.stringify({
          poll_token: "flow-token",
          browser_token: "browser-token-with-enough-length",
        }),
      }),
    );

    expect(response.status).toBe(200);
    expect(auth.completeBrowserHandoff).toHaveBeenCalledWith(
      "browser-token-with-enough-length",
      session.token,
    );
    expect(auth.setSessionCookie).toHaveBeenCalledWith(session);
    expect(auth.completeBrowserHandoff.mock.invocationCallOrder[0]).toBeLessThan(
      auth.setSessionCookie.mock.invocationCallOrder[0],
    );
  });

  it("does not store or complete a GitHub login when handoff binding fails", async () => {
    auth.backendFetch.mockResolvedValue(
      Response.json({
        token: "secret",
        expires_at: "2026-09-19T12:00:00.000Z",
        user: { id: "1", username: "octocat" },
      }),
    );
    auth.completeBrowserHandoff.mockResolvedValue(
      new Response(null, { status: 409 }),
    );

    const response = await pollGithub(
      new Request("http://localhost/api/auth/github/poll", {
        method: "POST",
        body: JSON.stringify({
          poll_token: "flow-token",
          browser_token: "browser-token-with-enough-length",
        }),
      }),
    );

    expect(response.status).toBe(409);
    expect(await response.json()).toEqual({ error: "safe" });
    expect(auth.setSessionCookie).not.toHaveBeenCalled();
  });

  it("forwards an email handoff token to the backend", async () => {
    auth.backendFetch.mockResolvedValue(
      new Response(null, { status: 201 }),
    );

    const response = await startEmail(
      new Request("http://localhost/api/auth/email/start", {
        method: "POST",
        body: JSON.stringify({
          email: " person@example.com ",
          browser_token: "browser-token-with-enough-length",
        }),
      }),
    );

    expect(response.status).toBe(201);
    expect(auth.backendFetch).toHaveBeenCalledWith("/v1/auth/email/start", {
      method: "POST",
      body: JSON.stringify({
        email: "person@example.com",
        browser_token: "browser-token-with-enough-length",
      }),
    });
  });

  it("rejects email starts locally when email authentication is disabled", async () => {
    vi.stubEnv("EMAIL_AUTH_ENABLED", "false");

    const response = await startEmail(
      new Request("http://localhost/api/auth/email/start", {
        method: "POST",
        body: JSON.stringify({ email: "person@example.com" }),
      }),
    );

    expect(response.status).toBe(503);
    expect(auth.backendFetch).not.toHaveBeenCalled();
  });

  it("completes an email handoff before setting the cookie and redirects to CLI success", async () => {
    const session = {
      token: "email-session-secret",
      expires_at: "2026-09-19T12:00:00.000Z",
      user: { id: "1", username: "octocat" },
    };
    auth.backendFetch.mockResolvedValue(Response.json(session));
    auth.completeBrowserHandoff.mockResolvedValue(
      new Response(null, { status: 204 }),
    );

    const response = await verifyEmail(
      new Request(
        "http://localhost/auth/email/verify?token=email-token&handoff=browser-token-with-enough-length",
      ),
    );

    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe(
      "http://localhost/sign-in?cli=complete",
    );
    expect(auth.completeBrowserHandoff).toHaveBeenCalledWith(
      "browser-token-with-enough-length",
      session.token,
    );
    expect(auth.setSessionCookie).toHaveBeenCalledWith(session);
  });

  it("does not set the email session cookie when handoff completion fails", async () => {
    auth.backendFetch.mockResolvedValue(
      Response.json({
        token: "secret",
        expires_at: "2026-09-19T12:00:00.000Z",
        user: { id: "1", username: "octocat" },
      }),
    );
    auth.completeBrowserHandoff.mockResolvedValue(
      new Response(null, { status: 404 }),
    );

    const response = await verifyEmail(
      new Request(
        "http://localhost/auth/email/verify?token=email-token&handoff=browser-token-with-enough-length",
      ),
    );

    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toContain(
      "/sign-in?error=handoff_failed",
    );
    expect(auth.setSessionCookie).not.toHaveBeenCalled();
  });

  it("completes an existing cookie session without exposing its bearer", async () => {
    auth.readSessionToken.mockResolvedValue("cookie-secret");
    auth.completeBrowserHandoff.mockResolvedValue(
      new Response(null, { status: 204 }),
    );

    const response = await completeHandoff(
      new Request("http://localhost/api/auth/handoff/complete", {
        method: "POST",
        body: JSON.stringify({
          browser_token: "browser-token-with-enough-length",
        }),
      }),
    );

    expect(response.status).toBe(204);
    expect(auth.completeBrowserHandoff).toHaveBeenCalledWith(
      "browser-token-with-enough-length",
      "cookie-secret",
    );
    expect(await response.text()).toBe("");
  });

  it("reads the cookie server-side and returns only the backend profile", async () => {
    auth.readSessionToken.mockResolvedValue("secret");
    auth.backendFetch.mockResolvedValue(
      Response.json({ id: "1", username: "octocat", stats: {} }),
    );

    const response = await getSession();
    expect(await response.json()).toEqual({
      user: { id: "1", username: "octocat", stats: {} },
    });
    expect(auth.backendFetch).toHaveBeenCalledWith("/v1/me", {
      headers: { Authorization: "Bearer secret" },
    });
  });

  it("clears the cookie even when backend logout is unavailable", async () => {
    auth.readSessionToken.mockResolvedValue("secret");
    auth.backendFetch.mockRejectedValue(new Error("offline"));

    const response = await logout();
    expect(response.status).toBe(204);
    expect(auth.clearSessionCookie).toHaveBeenCalledOnce();
  });
});
