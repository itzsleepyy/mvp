CREATE TABLE users (
  id uuid PRIMARY KEY,
  github_id bigint UNIQUE,
  username text NOT NULL,
  display_name text NOT NULL,
  avatar_url text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX users_username_lower_unique ON users (lower(username));

CREATE TABLE sessions (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  expires_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_id_idx ON sessions(user_id);
CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);

CREATE TABLE github_device_flows (
  id uuid PRIMARY KEY,
  poll_token_hash text NOT NULL UNIQUE,
  device_code text NOT NULL,
  interval_seconds integer NOT NULL CHECK (interval_seconds > 0),
  next_poll_at timestamptz NOT NULL,
  expires_at timestamptz NOT NULL,
  completed_user_id uuid REFERENCES users(id) ON DELETE CASCADE,
  completed_session_expires_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX github_device_flows_expires_at_idx ON github_device_flows(expires_at);

CREATE TABLE game_runs (
  id uuid PRIMARY KEY,
  client_run_id uuid NOT NULL UNIQUE,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  game_id text NOT NULL CHECK (game_id IN ('stack_overflow', 'daily_pr', 'daily_fix')),
  challenge_date date NOT NULL,
  challenge_version integer,
  challenge_id text,
  raw_score integer NOT NULL CHECK (raw_score >= 0),
  normalized_score integer NOT NULL CHECK (normalized_score >= 0),
  result jsonb NOT NULL,
  duration_ms integer NOT NULL CHECK (duration_ms >= 0),
  client_version text NOT NULL,
  normalization_version integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (octet_length(result::text) <= 2048),
  CHECK (
    (game_id = 'stack_overflow' AND challenge_version IS NULL AND challenge_id IS NULL)
    OR
    (game_id IN ('daily_pr', 'daily_fix') AND challenge_version = 1
      AND challenge_id = game_id || ':v1:' || challenge_date::text)
  )
);
CREATE UNIQUE INDEX game_runs_one_daily_result
  ON game_runs(user_id, game_id, challenge_date)
  WHERE game_id IN ('daily_pr', 'daily_fix');
CREATE INDEX game_runs_leaderboard_idx
  ON game_runs(challenge_date, game_id, normalized_score DESC, user_id);
CREATE INDEX game_runs_user_idx ON game_runs(user_id, created_at DESC);
