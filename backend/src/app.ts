import {
  createCipheriv,
  createDecipheriv,
  createHash,
  createHmac,
  randomBytes,
  randomUUID,
} from "node:crypto";
import Fastify, { type FastifyReply, type FastifyRequest } from "fastify";
import rateLimit from "@fastify/rate-limit";
import type { TypeBoxTypeProvider } from "@fastify/type-provider-typebox";
import { Type, type Static } from "@sinclair/typebox";
import type pg from "pg";
import type { Config } from "./config.js";
import type { Database } from "./db.js";
import { officialGithubProvider, type GithubProvider } from "./github.js";
import {
  normalizeRun,
  utcDate,
  validateChallenge,
  type GameId,
} from "./scoring.js";
import {
  CreateRunBodySchema,
  DateSchema,
  ErrorSchema,
  GameIdSchema,
  LeaderboardSchema,
  MeSchema,
  RunSchema,
  SessionSchema,
} from "./schemas.js";

interface AuthUser {
  id: string;
  username: string;
  display_name: string;
  avatar_url: string | null;
}

declare module "fastify" {
  interface FastifyRequest {
    authUser: AuthUser;
    sessionHash: string;
  }
}

interface AppOptions {
  config: Config;
  db: Database;
  github?: GithubProvider;
  now?: () => Date;
}

interface RunRow {
  id: string;
  client_run_id: string;
  game_id: GameId;
  challenge_date: string;
  challenge_version: number | null;
  challenge_id: string | null;
  raw_score: number;
  normalized_score: number;
  result: Record<string, unknown>;
  duration_ms: number;
  client_version: string;
  normalization_version: number;
  created_at: Date;
  user_id: string;
}

const ErrorResponses = {
  400: ErrorSchema,
  401: ErrorSchema,
  404: ErrorSchema,
  409: ErrorSchema,
  422: ErrorSchema,
  429: ErrorSchema,
};
const hash = (token: string): string =>
  createHash("sha256").update(token).digest("hex");
const opaqueToken = (): string => randomBytes(32).toString("base64url");

function encryptionKey(secret: string): Buffer {
  return createHash("sha256").update(`device-flow:${secret}`).digest();
}

function encryptDeviceCode(value: string, secret: string): string {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", encryptionKey(secret), nonce);
  const encrypted = Buffer.concat([
    cipher.update(value, "utf8"),
    cipher.final(),
  ]);
  return [nonce, cipher.getAuthTag(), encrypted]
    .map((part) => part.toString("base64url"))
    .join(".");
}

function decryptDeviceCode(value: string, secret: string): string {
  const [nonceValue, tagValue, encryptedValue] = value.split(".");
  if (!nonceValue || !tagValue || !encryptedValue)
    throw new Error("Stored device flow is malformed");
  const decipher = createDecipheriv(
    "aes-256-gcm",
    encryptionKey(secret),
    Buffer.from(nonceValue, "base64url"),
  );
  decipher.setAuthTag(Buffer.from(tagValue, "base64url"));
  return Buffer.concat([
    decipher.update(Buffer.from(encryptedValue, "base64url")),
    decipher.final(),
  ]).toString("utf8");
}

function deviceSessionToken(pollToken: string, secret: string): string {
  return createHmac("sha256", secret)
    .update(`device-session:${pollToken}`)
    .digest("base64url");
}

function sendError(
  reply: FastifyReply,
  status: number,
  code: string,
  message: string,
): FastifyReply {
  return reply.status(status).send({ error: { code, message } });
}

function validDate(value: string): boolean {
  const parsed = new Date(`${value}T00:00:00.000Z`);
  return !Number.isNaN(parsed.getTime()) && utcDate(parsed) === value;
}

function profile(row: AuthUser): AuthUser {
  return {
    id: row.id,
    username: row.username,
    display_name: row.display_name,
    avatar_url: row.avatar_url,
  };
}

function runResponse(
  row: RunRow,
): Omit<RunRow, "user_id" | "created_at"> & { created_at: string } {
  return {
    id: row.id,
    client_run_id: row.client_run_id,
    game_id: row.game_id,
    challenge_date: row.challenge_date,
    challenge_version: row.challenge_version,
    challenge_id: row.challenge_id,
    raw_score: row.raw_score,
    normalized_score: row.normalized_score,
    result: row.result,
    duration_ms: row.duration_ms,
    client_version: row.client_version,
    normalization_version: row.normalization_version,
    created_at: row.created_at.toISOString(),
  };
}

async function createSession(
  db: Database | pg.PoolClient,
  user: AuthUser,
  config: Config,
  now: Date,
) {
  const token = opaqueToken();
  const expires = new Date(now.getTime() + config.sessionTtlDays * 86_400_000);
  await db.query(
    "INSERT INTO sessions(id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    [randomUUID(), user.id, hash(token), expires],
  );
  return { token, expires_at: expires.toISOString(), user: profile(user) };
}

function monday(date: Date): string {
  const utc = new Date(
    Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()),
  );
  const day = utc.getUTCDay();
  utc.setUTCDate(utc.getUTCDate() - (day === 0 ? 6 : day - 1));
  return utcDate(utc);
}

export async function buildApp(options: AppOptions) {
  const {
    config,
    db,
    github = officialGithubProvider,
    now = () => new Date(),
  } = options;
  const app = Fastify({
    trustProxy: config.trustProxy,
    logger: {
      level: config.logLevel,
      redact: [
        "req.headers.authorization",
        "req.body.poll_token",
        "req.body.token",
        "device_code",
        "access_token",
      ],
    },
    ajv: { customOptions: { removeAdditional: false } },
  }).withTypeProvider<TypeBoxTypeProvider>();

  app.decorateRequest("authUser", null as unknown as AuthUser);
  app.decorateRequest("sessionHash", "");
  await app.register(rateLimit, {
    max: config.nodeEnv === "test" ? 10_000 : 120,
    timeWindow: "1 minute",
  });

  app.setErrorHandler((error, _request, reply) => {
    const requestError = error as Error & { validation?: unknown };
    if (requestError.validation)
      return sendError(reply, 400, "invalid_request", requestError.message);
    if ((requestError as { statusCode?: number }).statusCode === 429)
      return sendError(
        reply,
        429,
        "rate_limited",
        "Too many requests; try again later",
      );
    app.log.error({ err: error }, "request failed");
    return sendError(
      reply,
      500,
      "internal_error",
      "An internal error occurred",
    );
  });

  const authenticate = async (
    request: FastifyRequest,
    reply: FastifyReply,
  ): Promise<void> => {
    const authorization = request.headers.authorization;
    if (!authorization?.startsWith("Bearer ") || authorization.length <= 7) {
      sendError(
        reply,
        401,
        "unauthorized",
        "A valid bearer session is required",
      );
      return;
    }
    const tokenHash = hash(authorization.slice(7));
    const result = await db.query<AuthUser>(
      `SELECT u.id, u.username, u.display_name, u.avatar_url FROM sessions s JOIN users u ON u.id = s.user_id
       WHERE s.token_hash = $1 AND s.expires_at > now()`,
      [tokenHash],
    );
    const user = result.rows[0];
    if (!user) {
      sendError(
        reply,
        401,
        "unauthorized",
        "A valid bearer session is required",
      );
      return;
    }
    request.authUser = user;
    request.sessionHash = tokenHash;
  };

  app.get(
    "/health",
    {
      schema: {
        response: { 200: Type.Object({ status: Type.Literal("ok") }) },
      },
    },
    () => ({ status: "ok" as const }),
  );

  app.post(
    "/v1/auth/github/device",
    {
      config: { rateLimit: { max: 10, timeWindow: "1 minute" } },
      schema: {
        response: {
          201: Type.Object({
            flow_token: Type.String(),
            user_code: Type.String(),
            verification_uri: Type.String({ format: "uri" }),
            expires_in: Type.Integer(),
            interval: Type.Integer(),
          }),
          ...ErrorResponses,
        },
      },
    },
    async (_request, reply) => {
      const device = await github.start(config.githubClientId);
      const flowToken = opaqueToken();
      const current = now();
      await db.query(
        `INSERT INTO github_device_flows(id, poll_token_hash, device_code, interval_seconds, next_poll_at, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)`,
        [
          randomUUID(),
          hash(flowToken),
          encryptDeviceCode(device.deviceCode, config.sessionSecret),
          device.interval,
          new Date(current.getTime() + device.interval * 1_000),
          new Date(current.getTime() + device.expiresIn * 1_000),
        ],
      );
      return reply.status(201).send({
        flow_token: flowToken,
        user_code: device.userCode,
        verification_uri: device.verificationUri,
        expires_in: device.expiresIn,
        interval: device.interval,
      });
    },
  );

  const PollBody = Type.Object(
    { poll_token: Type.String({ minLength: 20, maxLength: 100 }) },
    { additionalProperties: false },
  );
  app.post(
    "/v1/auth/github/poll",
    {
      config: { rateLimit: { max: 60, timeWindow: "1 minute" } },
      schema: {
        body: PollBody,
        response: {
          200: SessionSchema,
          202: Type.Object({
            status: Type.Literal("pending"),
            retry_after: Type.Integer(),
          }),
          ...ErrorResponses,
        },
      },
    },
    async (request, reply) => {
      const tokenHash = hash(request.body.poll_token);
      type FlowRow = {
        device_code: string;
        interval_seconds: number;
        next_poll_at: Date;
        expires_at: Date;
        completed_user_id: string | null;
        completed_session_expires_at: Date | null;
      };
      const completedSession = async (flow: FlowRow) => {
        if (!flow.completed_user_id || !flow.completed_session_expires_at)
          return null;
        const users = await db.query<AuthUser>(
          "SELECT id, username, display_name, avatar_url FROM users WHERE id = $1",
          [flow.completed_user_id],
        );
        const user = users.rows[0];
        if (!user) throw new Error("Completed device flow user is missing");
        return {
          token: deviceSessionToken(
            request.body.poll_token,
            config.sessionSecret,
          ),
          expires_at: flow.completed_session_expires_at.toISOString(),
          user: profile(user),
        };
      };
      const claimed = await db.query<FlowRow>(
        `UPDATE github_device_flows SET next_poll_at = $2 + interval '120 seconds'
         WHERE poll_token_hash = $1 AND next_poll_at <= $2 AND expires_at > $2
         RETURNING device_code, interval_seconds, next_poll_at, expires_at,
                   completed_user_id, completed_session_expires_at`,
        [tokenHash, now()],
      );
      const flow = claimed.rows[0];
      if (!flow) {
        const existing = await db.query<FlowRow>(
          `SELECT device_code, interval_seconds, next_poll_at, expires_at,
                  completed_user_id, completed_session_expires_at
           FROM github_device_flows WHERE poll_token_hash = $1`,
          [tokenHash],
        );
        const row = existing.rows[0];
        if (!row || row.expires_at <= now()) {
          await db.query(
            "DELETE FROM github_device_flows WHERE poll_token_hash = $1",
            [tokenHash],
          );
          return sendError(
            reply,
            404,
            "flow_expired",
            "The device flow expired or does not exist",
          );
        }
        const completed = await completedSession(row);
        if (completed) return completed;
        const retry = Math.max(
          1,
          Math.ceil((row.next_poll_at.getTime() - now().getTime()) / 1_000),
        );
        return reply
          .status(202)
          .send({ status: "pending", retry_after: retry });
      }

      const completed = await completedSession(flow);
      if (completed) return completed;

      const polled = await github.poll(
        config.githubClientId,
        decryptDeviceCode(flow.device_code, config.sessionSecret),
      );
      if (polled.status === "pending") {
        await db.query(
          `UPDATE github_device_flows
           SET next_poll_at = $2 + interval_seconds * interval '1 second'
           WHERE poll_token_hash = $1`,
          [tokenHash, now()],
        );
        return reply
          .status(202)
          .send({ status: "pending", retry_after: flow.interval_seconds });
      }
      if (polled.status === "slow_down") {
        await db.query(
          `UPDATE github_device_flows SET interval_seconds = interval_seconds + 5,
            next_poll_at = $2 + (interval_seconds + 5) * interval '1 second'
            WHERE poll_token_hash = $1`,
          [tokenHash, now()],
        );
        return reply
          .status(202)
          .send({ status: "pending", retry_after: flow.interval_seconds + 5 });
      }
      if (polled.status === "expired" || polled.status === "denied") {
        await db.query(
          "DELETE FROM github_device_flows WHERE poll_token_hash = $1",
          [tokenHash],
        );
        return sendError(
          reply,
          401,
          polled.status === "denied" ? "access_denied" : "flow_expired",
          "GitHub authorization did not complete",
        );
      }

      const githubUser = await github.user(polled.accessToken);
      const client = await db.connect();
      try {
        await client.query("BEGIN");
        const users = await client.query<AuthUser>(
          `INSERT INTO users(id, github_id, username, display_name, avatar_url) VALUES ($1, $2, $3, $3, $4)
            ON CONFLICT (github_id) DO UPDATE SET username = EXCLUDED.username, avatar_url = EXCLUDED.avatar_url, updated_at = now()
            RETURNING id, username, display_name, avatar_url`,
          [randomUUID(), githubUser.id, githubUser.login, githubUser.avatarUrl],
        );
        const user = users.rows[0];
        if (!user) throw new Error("User upsert returned no row");
        const sessionToken = deviceSessionToken(
          request.body.poll_token,
          config.sessionSecret,
        );
        const sessionExpires = new Date(
          now().getTime() + config.sessionTtlDays * 86_400_000,
        );
        await client.query(
          `INSERT INTO sessions(id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)
           ON CONFLICT (token_hash) DO NOTHING`,
          [randomUUID(), user.id, hash(sessionToken), sessionExpires],
        );
        await client.query(
          `UPDATE github_device_flows
           SET completed_user_id = $2, completed_session_expires_at = $3, device_code = ''
           WHERE poll_token_hash = $1`,
          [tokenHash, user.id, sessionExpires],
        );
        await client.query("COMMIT");
        return {
          token: sessionToken,
          expires_at: sessionExpires.toISOString(),
          user: profile(user),
        };
      } catch (error) {
        await client.query("ROLLBACK");
        throw error;
      } finally {
        client.release();
      }
    },
  );

  app.post(
    "/v1/auth/logout",
    {
      preHandler: authenticate,
      schema: { response: { 204: Type.Null(), ...ErrorResponses } },
    },
    async (request, reply) => {
      await db.query("DELETE FROM sessions WHERE token_hash = $1", [
        request.sessionHash,
      ]);
      return reply.status(204).send(null);
    },
  );

  if (config.nodeEnv === "test") {
    const TestAuthBody = Type.Object({
      username: Type.String({
        minLength: 1,
        maxLength: 39,
        pattern: "^[A-Za-z0-9-]+$",
      }),
    });
    app.post(
      "/v1/auth/test",
      { schema: { body: TestAuthBody, response: { 200: SessionSchema } } },
      async (request) => {
        const users = await db.query<AuthUser>(
          `INSERT INTO users(id, username, display_name) VALUES ($1, $2, $2)
          ON CONFLICT ((lower(username))) DO UPDATE SET updated_at = now() RETURNING id, username, display_name, avatar_url`,
          [randomUUID(), request.body.username],
        );
        const user = users.rows[0];
        if (!user) throw new Error("Test user upsert returned no row");
        return createSession(db, user, config, now());
      },
    );
  }

  app.get(
    "/v1/me",
    {
      preHandler: authenticate,
      schema: { response: { 200: MeSchema, ...ErrorResponses } },
    },
    async (request) => {
      const today = utcDate(now());
      const weekStart = monday(now());
      const stats = await db.query<{
        daily_rank: string | null;
        weekly_rank: string | null;
        global_rank: string | null;
        best_stack: number;
        daily_pr_streak: string;
        daily_fix_this_week: string;
      }>(
        `WITH best AS (
           SELECT user_id, challenge_date, game_id, max(normalized_score) points
           FROM game_runs WHERE challenge_date <= $1 GROUP BY user_id, challenge_date, game_id
         ), daily_totals AS (
           SELECT user_id, challenge_date, sum(points) points FROM best
           GROUP BY user_id, challenge_date HAVING sum(points) > 0
         ), daily_ranks AS (
           SELECT user_id, rank() OVER (ORDER BY points DESC) rank FROM daily_totals WHERE challenge_date = $1
         ), weekly_totals AS (
           SELECT user_id, sum(points) points FROM daily_totals WHERE challenge_date BETWEEN $2 AND $1 GROUP BY user_id
         ), weekly_ranks AS (
           SELECT user_id, rank() OVER (ORDER BY points DESC) rank FROM weekly_totals
         ), all_totals AS (
           SELECT user_id, sum(points) points FROM daily_totals GROUP BY user_id
         ), all_ranks AS (
           SELECT user_id, rank() OVER (ORDER BY points DESC) rank FROM all_totals
         ), pr_days AS (
           SELECT DISTINCT challenge_date FROM game_runs
           WHERE user_id = $3 AND game_id = 'daily_pr' AND normalized_score > 0 AND challenge_date <= $1
         ), pr_numbered AS (
           SELECT challenge_date, row_number() OVER (ORDER BY challenge_date DESC) n FROM pr_days
         )
         SELECT
           (SELECT rank FROM daily_ranks WHERE user_id = $3) daily_rank,
           (SELECT rank FROM weekly_ranks WHERE user_id = $3) weekly_rank,
           (SELECT rank FROM all_ranks WHERE user_id = $3) global_rank,
           coalesce((SELECT max(normalized_score) FROM game_runs WHERE user_id = $3 AND game_id = 'stack_overflow'), 0)::integer best_stack,
           (SELECT count(*) FROM pr_numbered
            WHERE (SELECT max(challenge_date) FROM pr_days) >= $1::date - 1
              AND challenge_date = (SELECT max(challenge_date) FROM pr_days) - (n::integer - 1)) daily_pr_streak,
           (SELECT count(DISTINCT challenge_date) FROM game_runs WHERE user_id = $3 AND game_id = 'daily_fix' AND normalized_score > 0 AND challenge_date BETWEEN $2 AND $1) daily_fix_this_week`,
        [today, weekStart, request.authUser.id],
      );
      const row = stats.rows[0];
      if (!row) throw new Error("Profile stats query returned no row");
      return {
        ...profile(request.authUser),
        stats: {
          daily_rank: row.daily_rank === null ? null : Number(row.daily_rank),
          weekly_rank:
            row.weekly_rank === null ? null : Number(row.weekly_rank),
          global_rank:
            row.global_rank === null ? null : Number(row.global_rank),
          best_stack: row.best_stack,
          daily_pr_streak: Number(row.daily_pr_streak),
          daily_fix_this_week: Number(row.daily_fix_this_week),
        },
      };
    },
  );

  app.post(
    "/v1/runs",
    {
      preHandler: authenticate,
      config: { rateLimit: { max: 30, timeWindow: "1 minute" } },
      schema: {
        body: CreateRunBodySchema,
        response: { 200: RunSchema, 201: RunSchema, ...ErrorResponses },
      },
    },
    async (request, reply) => {
      const existing = await db.query<RunRow>(
        "SELECT * FROM game_runs WHERE client_run_id = $1",
        [request.body.client_run_id],
      );
      const prior = existing.rows[0];
      if (prior) {
        if (prior.user_id !== request.authUser.id)
          return sendError(
            reply,
            409,
            "client_run_id_conflict",
            "client_run_id belongs to another user",
          );
        return reply.status(200).send(runResponse(prior));
      }

      let challenge: ReturnType<typeof validateChallenge>;
      let normalized: number;
      try {
        challenge = validateChallenge(
          request.body.game_id,
          request.body.challenge,
          now(),
        );
        normalized = normalizeRun({
          gameId: request.body.game_id,
          rawScore: request.body.raw_score,
          durationMs: request.body.duration_ms,
          ...(request.body.result.solved === undefined
            ? {}
            : { solved: request.body.result.solved }),
          ...(request.body.result.attempts === undefined
            ? {}
            : { attempts: request.body.result.attempts }),
          ...(request.body.result.hint_used === undefined
            ? {}
            : { hintUsed: request.body.result.hint_used }),
        });
      } catch (error) {
        return sendError(
          reply,
          422,
          "invalid_run",
          error instanceof Error ? error.message : "Invalid run",
        );
      }
      const currentDate = utcDate(now());
      try {
        const inserted = await db.query<RunRow>(
          `INSERT INTO game_runs(
             id, client_run_id, user_id, game_id, challenge_date, challenge_version, challenge_id,
             raw_score, normalized_score, result, duration_ms, client_version, normalization_version
           ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,1) RETURNING *`,
          [
            randomUUID(),
            request.body.client_run_id,
            request.authUser.id,
            request.body.game_id,
            challenge?.date ?? currentDate,
            challenge?.version ?? null,
            challenge?.id ?? null,
            request.body.raw_score,
            normalized,
            request.body.result,
            request.body.duration_ms,
            request.body.client_version,
          ],
        );
        const row = inserted.rows[0];
        if (!row) throw new Error("Run insert returned no row");
        return await reply.status(201).send(runResponse(row));
      } catch (error) {
        if ((error as { code?: string }).code === "23505") {
          const raced = await db.query<RunRow>(
            "SELECT * FROM game_runs WHERE client_run_id = $1",
            [request.body.client_run_id],
          );
          const row = raced.rows[0];
          if (row && row.user_id === request.authUser.id)
            return reply.status(200).send(runResponse(row));
          if (row)
            return sendError(
              reply,
              409,
              "client_run_id_conflict",
              "client_run_id belongs to another user",
            );
          return sendError(
            reply,
            409,
            "daily_result_exists",
            "A result for this daily game and date already exists",
          );
        }
        throw error;
      }
    },
  );

  const LimitQuery = Type.Object({
    limit: Type.Optional(
      Type.Integer({ minimum: 1, maximum: 100, default: 50 }),
    ),
  });
  const DailyQuery = Type.Intersect([
    LimitQuery,
    Type.Object({ date: Type.Optional(DateSchema) }),
  ]);
  type Limit = Static<typeof LimitQuery>;
  type Daily = Static<typeof DailyQuery>;

  async function totalsLeaderboard(
    period: string,
    from: string | null,
    through: string,
    limit: number,
  ) {
    const values: unknown[] = [];
    let condition = "TRUE";
    if (from) {
      values.push(from, through);
      condition = "r.challenge_date BETWEEN $1 AND $2";
    } else {
      values.push(through);
      condition = "r.challenge_date <= $1";
    }
    values.push(limit);
    const result = await db.query<{
      rank: string;
      id: string;
      username: string;
      display_name: string;
      avatar_url: string | null;
      points: string;
    }>(
      `WITH best AS (
         SELECT r.user_id, r.challenge_date, r.game_id, max(r.normalized_score) points
         FROM game_runs r WHERE ${condition} GROUP BY r.user_id, r.challenge_date, r.game_id
         ), totals AS (
           SELECT user_id, sum(points) points FROM best GROUP BY user_id HAVING sum(points) > 0
       ), ranked AS (
         SELECT user_id, points, rank() OVER (ORDER BY points DESC) rank FROM totals
       )
        SELECT ranked.rank, u.id, u.username, u.display_name, u.avatar_url, ranked.points FROM ranked
       JOIN users u ON u.id = ranked.user_id
       ORDER BY ranked.points DESC, lower(u.username), u.id LIMIT $${String(values.length)}`,
      values,
    );
    return {
      period,
      from,
      through,
      game_id: null,
      entries: result.rows.map((row) => ({
        rank: Number(row.rank),
        user: profile(row),
        points: Number(row.points),
      })),
    };
  }

  app.get<{ Querystring: Daily }>(
    "/v1/leaderboards/daily",
    {
      schema: {
        querystring: DailyQuery,
        response: { 200: LeaderboardSchema, 400: ErrorSchema },
      },
    },
    async (request, reply) => {
      const date = request.query.date ?? utcDate(now());
      if (!validDate(date))
        return sendError(
          reply,
          400,
          "invalid_date",
          "date must be a real UTC date",
        );
      return totalsLeaderboard("daily", date, date, request.query.limit ?? 50);
    },
  );
  app.get<{ Querystring: Limit }>(
    "/v1/leaderboards/weekly",
    {
      schema: { querystring: LimitQuery, response: { 200: LeaderboardSchema } },
    },
    async (request) => {
      const through = utcDate(now());
      return totalsLeaderboard(
        "weekly",
        monday(now()),
        through,
        request.query.limit ?? 50,
      );
    },
  );
  app.get<{ Querystring: Limit }>(
    "/v1/leaderboards/all-time",
    {
      schema: { querystring: LimitQuery, response: { 200: LeaderboardSchema } },
    },
    async (request) => {
      return totalsLeaderboard(
        "all_time",
        null,
        utcDate(now()),
        request.query.limit ?? 50,
      );
    },
  );

  const GameParams = Type.Object({ game: GameIdSchema });
  const GameQuery = Type.Object({
    period: Type.Optional(
      Type.Union([Type.Literal("daily"), Type.Literal("all_time")], {
        default: "all_time",
      }),
    ),
    date: Type.Optional(DateSchema),
    limit: Type.Optional(
      Type.Integer({ minimum: 1, maximum: 100, default: 50 }),
    ),
  });
  app.get<{
    Params: Static<typeof GameParams>;
    Querystring: Static<typeof GameQuery>;
  }>(
    "/v1/leaderboards/games/:game",
    {
      schema: {
        params: GameParams,
        querystring: GameQuery,
        response: { 200: LeaderboardSchema, 400: ErrorSchema },
      },
    },
    async (request, reply) => {
      const period = request.query.period ?? "all_time";
      const date = request.query.date ?? utcDate(now());
      if (!validDate(date))
        return sendError(
          reply,
          400,
          "invalid_date",
          "date must be a real UTC date",
        );
      const daily = period === "daily";
      const result = await db.query<{
        rank: string;
        id: string;
        username: string;
        display_name: string;
        avatar_url: string | null;
        points: number;
      }>(
        `WITH best AS (
           SELECT user_id, max(normalized_score) points FROM game_runs
           WHERE game_id = $1 AND challenge_date <= $3
             AND ($2::boolean = false OR challenge_date = $3) GROUP BY user_id
           HAVING max(normalized_score) > 0
         ), ranked AS (
           SELECT user_id, points, rank() OVER (ORDER BY points DESC) rank FROM best
         )
          SELECT ranked.rank, u.id, u.username, u.display_name, u.avatar_url, ranked.points FROM ranked
         JOIN users u ON u.id = ranked.user_id
         ORDER BY ranked.points DESC, lower(u.username), u.id LIMIT $4`,
        [request.params.game, daily, date, request.query.limit ?? 50],
      );
      return {
        period,
        from: daily ? date : null,
        through: date,
        game_id: request.params.game,
        entries: result.rows.map((row) => ({
          rank: Number(row.rank),
          user: profile(row),
          points: row.points,
        })),
      };
    },
  );

  return app;
}
