# Backend

Phase 4B is a Node 22, strict TypeScript, Fastify, and PostgreSQL service in `backend/`. The Compose stack is development-only; PostgreSQL binds to loopback with disposable local credentials.

## Local workflow

1. Create the official GitHub OAuth App used by MVP and enable Device Flow. No client secret is needed.
2. Run `cp backend/.env.example backend/.env`, set `GITHUB_CLIENT_ID`, set `WEBSITE_URL` to the public website origin, and generate a random `SESSION_SECRET` of at least 32 characters.
3. Email sign-in defaults off. To enable it, verify a sending domain with Cloudflare Email Service, set `EMAIL_AUTH_ENABLED=true`, and set `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_EMAIL_API_TOKEN`, and `EMAIL_FROM`.
4. Start PostgreSQL with `docker compose up -d postgres`.
5. Run `cd backend && npm ci && npm run migrate && npm run dev`.

The migration runner applies ordered `migrations/*.sql` files transactionally and records them in `schema_migrations`. For the complete local container stack, set `GITHUB_CLIENT_ID` and `SESSION_SECRET`, then run `docker compose up --build`. `EMAIL_AUTH_ENABLED` defaults to `false`; Compose passes it to both the backend and website. `WEBSITE_URL` defaults to `http://localhost:3001` in Compose. The API listens on port 3000 and the website on port 3001. Public deployments must set `WEBSITE_URL` and `NEXT_PUBLIC_SITE_URL` to the same HTTPS origin.

Quality commands are `npm run format:check`, `npm run lint`, `npm run typecheck`, `npm test`, and `npm run build`. Database E2E tests use a disposable database supplied as `TEST_DATABASE_URL`, for example:

```sh
TEST_DATABASE_URL=postgres://mvp:mvp@localhost:5432/mvp_test npm run test:db
```

Compose creates the disposable `mvp_test` database alongside the development database. The E2E suite refuses URLs that do not name `mvp_test`, truncates its application tables, and skips when `TEST_DATABASE_URL` is absent.

## Authentication

The default terminal flow starts with `POST /v1/auth/handoff/start`. It returns a private polling token as `flow_token` and a `verification_uri` containing a separate browser capability. After the user authenticates on the website, authenticated `POST /v1/auth/handoff/complete` binds that user to the pending handoff. The terminal polls `POST /v1/auth/handoff/poll` and receives a distinct MVP session, so signing out of the website does not sign out the terminal. Handoffs expire after 10 minutes. Direct GitHub Device Flow and email polling remain available as explicit CLI fallbacks.

`POST /v1/auth/github/device` takes no body. It starts GitHub's official Device Flow without a `scope` parameter. MVP uses a dedicated OAuth app that never requests broader scopes; GitHub may otherwise reuse scopes previously granted to the same OAuth app. The response is:

```json
{
  "flow_token": "opaque-value",
  "user_code": "ABCD-EFGH",
  "verification_uri": "https://github.com/login/device",
  "expires_in": 900,
  "interval": 5
}
```

`POST /v1/auth/github/poll` takes `{"poll_token":"..."}`. Poll no faster than `interval`; pending responses are HTTP 202 with `{"status":"pending","retry_after":5}`. Completion is HTTP 200 with:

```json
{
  "token": "opaque-bearer-token",
  "expires_at": "2026-09-19T12:00:00.000Z",
  "user": { "id": "uuid", "username": "octocat", "avatar_url": "https://..." }
}
```

The backend encrypts active GitHub device codes at rest, exchanges the code, calls GitHub `/user`, and then discards the GitHub token. GitHub tokens are never persisted. MVP bearer tokens are opaque values; only their SHA-256 hashes are stored. A completed flow remains replayable for its short authorization lifetime so a dropped CLI response does not strand the session. Supply sessions as `Authorization: Bearer <token>`. `POST /v1/auth/logout` revokes the current session and returns 204. `GET /v1/me` returns identity plus Daily, Weekly, and global ranks, normalized best Stack Overflow score, current Daily PR streak, and successful Daily Fix days this week. A `POST /v1/auth/test` route exists only when `NODE_ENV=test`; it accepts `{"username":"e2e-user"}` and returns a session for credential-free E2E testing.

When `EMAIL_AUTH_ENABLED=true`, email authentication uses `POST /v1/auth/email/start` with `{"email":"you@example.com"}`. The backend stores only hashes of independent verification and polling tokens, sends a 15-minute link through Cloudflare Email Service, and returns `flow_token`, `expires_in`, and `interval`. The website exchanges the link at `POST /v1/auth/email/verify`; terminal clients poll `POST /v1/auth/email/poll` with `{"poll_token":"..."}`. Both receive the same replay-safe MVP session. Email addresses are never used as public usernames or returned in profiles; email identities receive a pseudonymous `player-<hash>` username. GitHub and email identities are never merged automatically. When disabled, the website omits the email form and all three backend email endpoints return `email_auth_unavailable` without accessing stored flows.

The CLI partitions credential-store entries by canonical API origin. Production credentials are never loaded for a development override, and cleartext HTTP is accepted only for loopback development URLs.

## Run contract

Stable game IDs are `stack_overflow`, `daily_pr`, and `daily_fix`. `POST /v1/runs` requires authentication and accepts:

```json
{
  "client_run_id": "4c83cccb-f168-4dd8-bbe4-b286d74ca491",
  "game_id": "daily_pr",
  "raw_score": 3,
  "duration_ms": 125000,
  "client_version": "0.4.0",
  "result": { "solved": true },
  "challenge": {
    "date": "2026-08-20",
    "version": 1,
    "id": "daily_pr:v1:2026-08-20"
  }
}
```

`result` is retained as JSON. Daily games require `result.solved` to be a boolean; Daily Fix also requires `attempts` and `hint_used`. Stack Overflow must omit `challenge`; daily games require it. A daily submission is accepted only for the server's current UTC date, version 1, and the exact deterministic ID `daily_pr:v1:YYYY-MM-DD` or `daily_fix:v1:YYYY-MM-DD`. There is no previous-day grace window. These IDs verify date and game identity without exposing or reproducing challenge source content.

Successful creation is HTTP 201. The response includes server `id`, all submitted fields (with challenge fields flattened as `challenge_date`, `challenge_version`, and `challenge_id`), `normalized_score`, `normalization_version`, and ISO-8601 `created_at`. Repeating a `client_run_id` as its owner returns the existing run with HTTP 200, regardless of retry body. The globally unique ID used by another user is HTTP 409. Daily PR and Daily Fix allow one result per user/game/date and return HTTP 409 for another client run ID. Stack Overflow permits multiple runs so the best run can count.

The local outbox is partitioned by immutable online user ID and also binds every pending run to its issuing API origin. Cross-process file locking reserves work briefly, releases the lock during HTTP, and merges outcomes without concurrent MVP instances overwriting one another. Transient failures remain queued with capped exponential backoff; only permanent validation/conflict responses are removed.

Errors have one stable shape:

```json
{ "error": { "code": "invalid_run", "message": "challenge.version must be 1" } }
```

## Normalization version 1

All divisions below use integer floor. Unsolved Daily PR and Daily Fix submissions receive zero points.

- Stack Overflow: `points = min(raw_score, 10000)`. The best run per user per UTC day counts.
- Daily PR: `raw_score` is guesses and must be 1 through 6. For solved results, `points = (7 - guesses) * 1000 + floor(max(0, 600000 - duration_ms) / 1000)`, capped at 6,600. One result per UTC day counts.
- Daily Fix: `raw_score` is charged duration in milliseconds, including penalties. For solved results, `points = max(0, 5000 - floor(min(raw_score, 300000) / 60))`. One result per UTC day counts.

The daily total sums the three counted game values. Missing games contribute zero.

The initial validation boundary rejects Stack Overflow scores above 100,000 or not divisible by 50, scored zero-duration runs, Daily PR durations over one hour, incomplete PRs before six guesses, and inconsistent Daily Fix duration/attempt/hint metrics. This is deliberately basic validation for an open-source local client, not a claim of cheat-proof execution.

## Leaderboards

Leaderboard routes are public, default to 50 entries, and accept `limit=1..100`. Ties use shared competition rank (`1, 1, 3`). Display order is deterministic: points descending, then case-insensitive username, then user ID.

Submitting a run publishes the account's public username, optional avatar, and MVP points on these public boards. The backend does not request private-repository access and never publishes email addresses.

- `GET /v1/leaderboards/daily?date=YYYY-MM-DD` sums each game's best normalized run on that UTC date. Date defaults to today.
- `GET /v1/leaderboards/weekly` sums daily totals from the current UTC Monday through the current UTC date.
- `GET /v1/leaderboards/all-time` sums every daily total through the current UTC date.
- `GET /v1/leaderboards/games/:game?period=all_time|daily&date=YYYY-MM-DD` ranks each user's single best normalized run for that game. `all_time` is the default; `date` applies to `daily` and otherwise supplies the response's `through` date.

Responses include `period`, nullable `from`, `through`, nullable `game_id`, and `entries`. Each entry is `{"rank":1,"user":{...},"points":6600}`.

The database E2E test uses the test-only authentication route to submit all three games, query Daily MVP, and verify the resulting profile statistics without contacting GitHub.

## Operations and privacy

Environment startup validation covers database URL, GitHub client ID outside tests, website URL, the email feature flag and required provider settings, port, session lifetime, proxy trust, and runtime environment. Production website URLs must use HTTPS, except for loopback-only local containers. Set `TRUST_PROXY=true` only behind a trusted reverse proxy so IP rate limits cannot be spoofed. Global rate limiting is 120 requests/minute/IP, with tighter auth-start and run-write limits. Production logs redact authorization headers, email addresses, poll/session/verification tokens, device codes, and OAuth access tokens. Request payloads and credentials must not be added to application log statements.
