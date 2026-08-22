import {
  type AuthSession,
  backendFetch,
  completeBrowserHandoff,
  errorResponse,
  setSessionCookie,
} from "@/lib/auth";
import { validBrowserToken } from "@/lib/handoff";

export async function POST(request: Request) {
  try {
    const body = (await request.json()) as {
      poll_token?: unknown;
      browser_token?: unknown;
    };
    if (typeof body.poll_token !== "string") return errorResponse(400);
    if (
      body.browser_token !== undefined &&
      !validBrowserToken(body.browser_token)
    )
      return errorResponse(400);

    const response = await backendFetch("/v1/auth/github/poll", {
      method: "POST",
      body: JSON.stringify({ poll_token: body.poll_token }),
    });
    if (!response.ok && response.status !== 202)
      return errorResponse(response.status);

    const result = (await response.json()) as
      | { status: "pending"; retry_after: number }
      | AuthSession;
    if (response.status === 202) {
      return Response.json(result, {
        status: 202,
        headers: { "Cache-Control": "no-store" },
      });
    }

    const session = result as AuthSession;
    if (body.browser_token) {
      const handoff = await completeBrowserHandoff(
        body.browser_token,
        session.token,
      );
      if (!handoff.ok) return errorResponse(handoff.status);
    }
    await setSessionCookie(session);
    return Response.json(
      { status: "complete", user: session.user },
      { headers: { "Cache-Control": "no-store" } },
    );
  } catch {
    return errorResponse(503);
  }
}
