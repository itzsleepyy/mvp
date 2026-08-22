import "server-only";

import { cookies } from "next/headers";
import type { AuthSession } from "@/lib/auth-types";
import {
  SESSION_COOKIE,
  sessionCookieOptions,
} from "@/lib/auth-cookie";

export type { AuthSession, AuthUser } from "@/lib/auth-types";
export { SESSION_COOKIE } from "@/lib/auth-cookie";

export function apiUrl(path: string): string {
  const origin = (process.env.MVP_API_URL ?? "http://localhost:3000").replace(
    /\/$/,
    "",
  );
  return `${origin}${path}`;
}

export async function backendFetch(
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  return fetch(apiUrl(path), {
    ...init,
    cache: "no-store",
    headers: {
      Accept: "application/json",
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
    signal: init.signal ?? AbortSignal.timeout(10_000),
  });
}

export async function setSessionCookie(session: AuthSession): Promise<void> {
  const expires = new Date(session.expires_at);
  if (!Number.isFinite(expires.getTime())) throw new Error("Invalid session expiry");

  (await cookies()).set(
    SESSION_COOKIE,
    session.token,
    sessionCookieOptions(expires),
  );
}

export async function readSessionToken(): Promise<string | undefined> {
  return (await cookies()).get(SESSION_COOKIE)?.value;
}

export async function clearSessionCookie(): Promise<void> {
  (await cookies()).set(
    SESSION_COOKIE,
    "",
    sessionCookieOptions(new Date(0)),
  );
}

export async function completeBrowserHandoff(
  browserToken: string,
  sessionToken: string,
): Promise<Response> {
  return backendFetch("/v1/auth/handoff/complete", {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
    body: JSON.stringify({ browser_token: browserToken }),
  });
}

export function publicError(status: number): string {
  if (status === 429) return "Too many attempts. Please wait and try again.";
  if (status === 401 || status === 404)
    return "This sign-in request expired. Please start again.";
  if (status === 409)
    return "This terminal sign-in request was already completed. Please start again.";
  return "Sign-in is unavailable right now. Please try again.";
}

export function errorResponse(status: number): Response {
  return Response.json(
    { error: publicError(status) },
    { status, headers: { "Cache-Control": "no-store" } },
  );
}
