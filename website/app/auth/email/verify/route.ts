import { NextResponse } from "next/server";
import {
  type AuthSession,
  backendFetch,
  completeBrowserHandoff,
  setSessionCookie,
} from "@/lib/auth";
import { validBrowserToken } from "@/lib/handoff";

function signInRedirect(request: Request, error: string) {
  const url = new URL("/sign-in", request.url);
  url.searchParams.set("error", error);
  const response = NextResponse.redirect(url, 303);
  response.headers.set("X-Robots-Tag", "noindex, nofollow");
  return response;
}

export async function GET(request: Request) {
  const requestUrl = new URL(request.url);
  const token = requestUrl.searchParams.get("token");
  const handoff = requestUrl.searchParams.get("handoff");
  if (!token) return signInRedirect(request, "invalid_link");
  if (handoff !== null && !validBrowserToken(handoff))
    return signInRedirect(request, "invalid_link");

  try {
    const response = await backendFetch("/v1/auth/email/verify", {
      method: "POST",
      body: JSON.stringify({ token }),
    });
    if (!response.ok) {
      return signInRedirect(
        request,
        response.status === 429 ? "rate_limited" : "invalid_link",
      );
    }
    const session = (await response.json()) as AuthSession;
    if (handoff) {
      const completion = await completeBrowserHandoff(handoff, session.token);
      if (!completion.ok) return signInRedirect(request, "handoff_failed");
    }
    await setSessionCookie(session);
    const redirect = NextResponse.redirect(
      new URL(handoff ? "/sign-in?cli=complete" : "/leaderboard", request.url),
      303,
    );
    redirect.headers.set("X-Robots-Tag", "noindex, nofollow");
    return redirect;
  } catch {
    return signInRedirect(request, "unavailable");
  }
}
