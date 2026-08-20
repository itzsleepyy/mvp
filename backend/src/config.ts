export type Environment = "development" | "test" | "production";

export interface Config {
  nodeEnv: Environment;
  host: string;
  port: number;
  databaseUrl: string;
  githubClientId: string;
  sessionSecret: string;
  sessionTtlDays: number;
  logLevel: string;
  trustProxy: boolean;
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

  return {
    nodeEnv,
    host: env.HOST ?? "127.0.0.1",
    port: integer("PORT", env.PORT, 3000, 1, 65_535),
    databaseUrl: env.DATABASE_URL,
    githubClientId: env.GITHUB_CLIENT_ID ?? "test-client",
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
  };
}
