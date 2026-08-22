CREATE TABLE login_handoffs (
  id uuid PRIMARY KEY,
  poll_token_hash text NOT NULL UNIQUE,
  browser_token_hash text NOT NULL UNIQUE,
  interval_seconds integer NOT NULL CHECK (interval_seconds > 0),
  next_poll_at timestamptz NOT NULL,
  expires_at timestamptz NOT NULL,
  completed_user_id uuid REFERENCES users(id) ON DELETE CASCADE,
  completed_session_expires_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (
    (completed_user_id IS NULL AND completed_session_expires_at IS NULL)
    OR (completed_user_id IS NOT NULL AND completed_session_expires_at IS NOT NULL)
  )
);
CREATE INDEX login_handoffs_expires_at_idx ON login_handoffs(expires_at);
