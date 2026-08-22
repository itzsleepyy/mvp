CREATE TABLE auth_identities (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider text NOT NULL CHECK (provider <> ''),
  provider_subject text NOT NULL CHECK (provider_subject <> ''),
  email_normalized text,
  email_verified_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (provider, provider_subject),
  CHECK (email_normalized IS NULL OR (
    email_normalized = lower(btrim(email_normalized))
    AND char_length(email_normalized) <= 254
  ))
);
CREATE INDEX auth_identities_user_id_idx ON auth_identities(user_id);

INSERT INTO auth_identities(id, user_id, provider, provider_subject)
SELECT id, id, 'github', github_id::text
FROM users
WHERE github_id IS NOT NULL
ON CONFLICT (provider, provider_subject) DO NOTHING;

CREATE TABLE email_auth_flows (
  id uuid PRIMARY KEY,
  poll_token_hash text NOT NULL UNIQUE,
  verification_token_hash text NOT NULL UNIQUE,
  email_normalized text NOT NULL CHECK (
    email_normalized = lower(btrim(email_normalized))
    AND char_length(email_normalized) <= 254
  ),
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
CREATE INDEX email_auth_flows_expires_at_idx ON email_auth_flows(expires_at);
