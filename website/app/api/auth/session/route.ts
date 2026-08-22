import {
  backendFetch,
  clearSessionCookie,
  errorResponse,
  readSessionToken,
} from "@/lib/auth";

export async function GET() {
  const token = await readSessionToken();
  if (!token) return errorResponse(401);

  try {
    const response = await backendFetch("/v1/me", {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!response.ok) {
      if (response.status === 401) await clearSessionCookie();
      return errorResponse(response.status);
    }
    return Response.json(
      { user: await response.json() },
      { headers: { "Cache-Control": "no-store" } },
    );
  } catch {
    return errorResponse(503);
  }
}
