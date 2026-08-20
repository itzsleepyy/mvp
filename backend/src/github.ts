export interface DeviceAuthorization {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export type DevicePoll =
  | { status: "pending" }
  | { status: "slow_down" }
  | { status: "expired" }
  | { status: "denied" }
  | { status: "complete"; accessToken: string };

export interface GithubUser {
  id: number;
  login: string;
  avatarUrl: string | null;
}

export interface GithubProvider {
  start(clientId: string): Promise<DeviceAuthorization>;
  poll(clientId: string, deviceCode: string): Promise<DevicePoll>;
  user(accessToken: string): Promise<GithubUser>;
}

async function githubRequest(
  url: string,
  init: RequestInit,
): Promise<Record<string, unknown>> {
  const headers = new Headers(init.headers);
  if (!headers.has("Accept")) headers.set("Accept", "application/json");
  if (!headers.has("User-Agent")) headers.set("User-Agent", "mvp-backend");
  const response = await fetch(url, {
    ...init,
    headers,
    signal: AbortSignal.timeout(10_000),
  });
  if (!response.ok)
    throw new Error(`GitHub returned HTTP ${String(response.status)}`);
  return (await response.json()) as Record<string, unknown>;
}

function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || !value)
    throw new Error(`GitHub response omitted ${name}`);
  return value;
}

function requiredNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value))
    throw new Error(`GitHub response omitted ${name}`);
  return value;
}

export const officialGithubProvider: GithubProvider = {
  async start(clientId) {
    const body = await githubRequest("https://github.com/login/device/code", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ client_id: clientId }),
    });
    return {
      deviceCode: requiredString(body.device_code, "device_code"),
      userCode: requiredString(body.user_code, "user_code"),
      verificationUri: requiredString(
        body.verification_uri,
        "verification_uri",
      ),
      expiresIn: requiredNumber(body.expires_in, "expires_in"),
      interval: requiredNumber(body.interval, "interval"),
    };
  },
  async poll(clientId, deviceCode) {
    const body = await githubRequest(
      "https://github.com/login/oauth/access_token",
      {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          client_id: clientId,
          device_code: deviceCode,
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        }),
      },
    );
    if (typeof body.access_token === "string")
      return { status: "complete", accessToken: body.access_token };
    switch (body.error) {
      case "authorization_pending":
        return { status: "pending" };
      case "slow_down":
        return { status: "slow_down" };
      case "expired_token":
        return { status: "expired" };
      case "access_denied":
        return { status: "denied" };
      default:
        throw new Error("Unexpected GitHub device flow response");
    }
  },
  async user(accessToken) {
    const body = await githubRequest("https://api.github.com/user", {
      method: "GET",
      headers: {
        Authorization: `Bearer ${accessToken}`,
        "X-GitHub-Api-Version": "2022-11-28",
      },
    });
    return {
      id: requiredNumber(body.id, "id"),
      login: requiredString(body.login, "login"),
      avatarUrl: typeof body.avatar_url === "string" ? body.avatar_url : null,
    };
  },
};
