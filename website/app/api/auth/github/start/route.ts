import { backendFetch, errorResponse } from "@/lib/auth";

export async function POST() {
  try {
    const response = await backendFetch("/v1/auth/github/device", {
      method: "POST",
    });
    if (!response.ok) return errorResponse(response.status);

    return Response.json(await response.json(), {
      status: 201,
      headers: { "Cache-Control": "no-store" },
    });
  } catch {
    return errorResponse(503);
  }
}
