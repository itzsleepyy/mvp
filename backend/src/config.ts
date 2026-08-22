export type Environment = "development" | "test" | "production";

export interface Config {
  nodeEnv: Environment;
  host: string;
  port: number;
  databaseUrl: string;
  githubClientId: string;
  emailAuthEnabled: boolean;
  cloudflareAccountId: string | null;
  cloudflareEmailApiToken: string | null;
  emailFrom: string | null;
  websiteUrl: string;
  sessionSecret: string;
  sessionTtlDays: number;
  logLevel: string;
  trustProxy: boolean;
  minimumClientVersion: string;
  latestClientVersion: string;
}

function integer(
  name: string,
  value: string | undefined,
  fallback: number,
  min: number,
  max: number,
): number {
  const parsed = Number(value ?? fallback);
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(
      `${name} must be an integer from ${String(min)} to ${String(max)}`,
    );
  }
  return parsed;
}

export function readConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const nodeEnv = env.NODE_ENV ?? "development";
  if (
    nodeEnv !== "development" &&
    nodeEnv !== "test" &&
    nodeEnv !== "production"
  ) {
    throw new Error("NODE_ENV must be development, test, or production");
  }
  if (!env.DATABASE_URL) throw new Error("DATABASE_URL is required");
  if (!env.GITHUB_CLIENT_ID && nodeEnv !== "test")
    throw new Error("GITHUB_CLIENT_ID is required");
  if (
    (!env.SESSION_SECRET || env.SESSION_SECRET.length < 32) &&
    nodeEnv !== "test"
  )
    throw new Error("SESSION_SECRET must contain at least 32 characters");
  const trustProxy = env.TRUST_PROXY ?? "false";
  if (trustProxy !== "true" && trustProxy !== "false")
    throw new Error("TRUST_PROXY must be true or false");

  const optional = (value: string | undefined): string | undefined =>
    value?.trim() || undefined;
  const emailAuthEnabledValue = env.EMAIL_AUTH_ENABLED ?? "false";
  if (emailAuthEnabledValue !== "true" && emailAuthEnabledValue !== "false")
    throw new Error("EMAIL_AUTH_ENABLED must be true or false");
  const emailAuthEnabled = emailAuthEnabledValue === "true";
  const minimumClientVersion = optional(env.MINIMUM_CLIENT_VERSION) ?? "0.1.0";
  const latestClientVersion =
    optional(env.LATEST_CLIENT_VERSION) ?? minimumClientVersion;
  const semver = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
  if (!semver.test(minimumClientVersion) || !semver.test(latestClientVersion))
    throw new Error("Client versions must be semantic versions");
  const cloudflareAccountId = optional(env.CLOUDFLARE_ACCOUNT_ID);
  const cloudflareEmailApiToken = optional(env.CLOUDFLARE_EMAIL_API_TOKEN);
  const emailFrom = optional(env.EMAIL_FROM);
  const websiteUrl =
    optional(env.WEBSITE_URL) ??
    (nodeEnv === "production" ? undefined : "http://localhost:3001");
  const emailValues = [cloudflareAccountId, cloudflareEmailApiToken, emailFrom];
  const configuredEmailValues = emailValues.filter(
    (value) => value !== undefined,
  );
  if (
    configuredEmailValues.length !== 0 &&
    configuredEmailValues.length !== emailValues.length
  )
    throw new Error(
      "CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_EMAIL_API_TOKEN, and EMAIL_FROM must be configured together",
    );
  if (emailAuthEnabled && configuredEmailValues.length !== emailValues.length)
    throw new Error(
      "Cloudflare email settings are required when EMAIL_AUTH_ENABLED=true",
    );
  if (!websiteUrl)
    throw new Error(
      cloudflareAccountId
        ? "WEBSITE_URL is required when email authentication is configured"
        : "WEBSITE_URL is required in production",
    );
  if (websiteUrl) {
    const website = new URL(websiteUrl);
    if (website.protocol !== "http:" && website.protocol !== "https:")
      throw new Error("WEBSITE_URL must be an HTTP(S) URL");
    const loopback =
      website.hostname === "localhost" ||
      website.hostname === "127.0.0.1" ||
      website.hostname === "[::1]";
    if (nodeEnv === "production" && website.protocol !== "https:" && !loopback)
      throw new Error("WEBSITE_URL must use HTTPS in production");
  }

  return {
    nodeEnv,
    host: env.HOST ?? "127.0.0.1",
    port: integer("PORT", env.PORT, 3000, 1, 65_535),
    databaseUrl: env.DATABASE_URL,
    githubClientId: env.GITHUB_CLIENT_ID ?? "test-client",
    emailAuthEnabled,
    cloudflareAccountId: cloudflareAccountId ?? null,
    cloudflareEmailApiToken: cloudflareEmailApiToken ?? null,
    emailFrom: emailFrom ?? null,
    websiteUrl: websiteUrl.replace(/\/+$/, ""),
    sessionSecret:
      env.SESSION_SECRET ?? "test-session-secret-at-least-32-chars",
    sessionTtlDays: integer(
      "SESSION_TTL_DAYS",
      env.SESSION_TTL_DAYS,
      30,
      1,
      365,
    ),
    logLevel: env.LOG_LEVEL ?? "info",
    trustProxy: trustProxy === "true",
    minimumClientVersion,
    latestClientVersion,
  };
}
