import {
  completeBrowserHandoff,
  errorResponse,
  readSessionToken,
} from "@/lib/auth";
import { validBrowserToken } from "@/lib/handoff";

export async function POST(request: Request) {
  const sessionToken = await readSessionToken();
  if (!sessionToken) return errorResponse(401);

  try {
    const body = (await request.json()) as { browser_token?: unknown };
    if (!validBrowserToken(body.browser_token)) return errorResponse(400);

    const response = await completeBrowserHandoff(
      body.browser_token,
      sessionToken,
    );
    if (!response.ok) return errorResponse(response.status);

    return new Response(null, {
      status: 204,
      headers: { "Cache-Control": "no-store" },
    });
  } catch {
    return errorResponse(503);
  }
}
