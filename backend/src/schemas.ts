import { Type, type Static } from "@sinclair/typebox";

export const GameIdSchema = Type.Union([
  Type.Literal("stack_overflow"),
  Type.Literal("daily_pr"),
  Type.Literal("daily_fix"),
]);
export const DateSchema = Type.String({ pattern: "^\\d{4}-\\d{2}-\\d{2}$" });
export const ErrorSchema = Type.Object({
  error: Type.Object({ code: Type.String(), message: Type.String() }),
});
export const ProfileSchema = Type.Object({
  id: Type.String({ format: "uuid" }),
  username: Type.String(),
  display_name: Type.String(),
  avatar_url: Type.Union([Type.String(), Type.Null()]),
});
export const MeSchema = Type.Intersect([
  ProfileSchema,
  Type.Object({
    stats: Type.Object({
      daily_rank: Type.Union([Type.Integer(), Type.Null()]),
      weekly_rank: Type.Union([Type.Integer(), Type.Null()]),
      global_rank: Type.Union([Type.Integer(), Type.Null()]),
      best_stack: Type.Integer(),
      daily_pr_streak: Type.Integer(),
      daily_fix_this_week: Type.Integer(),
    }),
  }),
]);
export const SessionSchema = Type.Object({
  token: Type.String(),
  expires_at: Type.String({ format: "date-time" }),
  user: ProfileSchema,
});
export const RunSchema = Type.Object({
  id: Type.String({ format: "uuid" }),
  client_run_id: Type.String({ format: "uuid" }),
  game_id: GameIdSchema,
  challenge_date: DateSchema,
  challenge_version: Type.Union([Type.Integer(), Type.Null()]),
  challenge_id: Type.Union([Type.String(), Type.Null()]),
  raw_score: Type.Integer(),
  normalized_score: Type.Integer(),
  result: Type.Record(Type.String(), Type.Unknown()),
  duration_ms: Type.Integer(),
  client_version: Type.String(),
  normalization_version: Type.Integer(),
  created_at: Type.String({ format: "date-time" }),
});
export const CreateRunBodySchema = Type.Object(
  {
    client_run_id: Type.String({ format: "uuid" }),
    game_id: GameIdSchema,
    raw_score: Type.Integer({ minimum: 0, maximum: 2_147_483_647 }),
    duration_ms: Type.Integer({ minimum: 0, maximum: 86_400_000 }),
    client_version: Type.String({ minLength: 1, maxLength: 64 }),
    result: Type.Object(
      {
        solved: Type.Optional(Type.Boolean()),
        attempts: Type.Optional(Type.Integer()),
        hint_used: Type.Optional(Type.Boolean()),
        height: Type.Optional(Type.Integer()),
      },
      { additionalProperties: false },
    ),
    challenge: Type.Optional(
      Type.Object(
        {
          date: DateSchema,
          version: Type.Integer(),
          id: Type.String({ minLength: 1, maxLength: 80 }),
        },
        { additionalProperties: false },
      ),
    ),
  },
  { additionalProperties: false },
);
export type CreateRunBody = Static<typeof CreateRunBodySchema>;

export const LeaderboardEntrySchema = Type.Object({
  rank: Type.Integer(),
  user: ProfileSchema,
  points: Type.Integer(),
});
export const LeaderboardSchema = Type.Object({
  period: Type.String(),
  from: Type.Union([DateSchema, Type.Null()]),
  through: DateSchema,
  game_id: Type.Union([GameIdSchema, Type.Null()]),
  entries: Type.Array(LeaderboardEntrySchema),
});
