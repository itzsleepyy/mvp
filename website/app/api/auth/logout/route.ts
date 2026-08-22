import {
  backendFetch,
  clearSessionCookie,
  readSessionToken,
} from "@/lib/auth";

export async function POST() {
  const token = await readSessionToken();
  try {
    if (token) {
      await backendFetch("/v1/auth/logout", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
      });
    }
  } catch {
    // Clearing the local credential must not depend on backend availability.
  }
  await clearSessionCookie();
  return new Response(null, {
    status: 204,
    headers: { "Cache-Control": "no-store" },
  });
}
