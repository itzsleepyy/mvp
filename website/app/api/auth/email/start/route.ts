import { backendFetch, errorResponse } from "@/lib/auth";
import { emailAuthEnabled } from "@/lib/features";
import { validBrowserToken } from "@/lib/handoff";

export async function POST(request: Request) {
  if (!emailAuthEnabled()) return errorResponse(503);
  try {
    const body = (await request.json()) as {
      email?: unknown;
      browser_token?: unknown;
    };
    if (typeof body.email !== "string" || !body.email.trim())
      return errorResponse(400);
    if (
      body.browser_token !== undefined &&
      !validBrowserToken(body.browser_token)
    )
      return errorResponse(400);

    const response = await backendFetch("/v1/auth/email/start", {
      method: "POST",
      body: JSON.stringify({
        email: body.email.trim(),
        ...(body.browser_token
          ? { browser_token: body.browser_token }
          : {}),
      }),
    });
    if (!response.ok) return errorResponse(response.status);

    return Response.json(
      { status: "sent" },
      { status: 201, headers: { "Cache-Control": "no-store" } },
    );
  } catch {
    return errorResponse(503);
  }
}
