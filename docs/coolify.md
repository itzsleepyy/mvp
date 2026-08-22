# Coolify Deployment

MVP deploys as one Coolify Docker Compose resource with three services:

- `website` serves `https://mostvaluedprogrammer.com` on container port 3001.
- `backend` serves `https://api.mostvaluedprogrammer.com` on container port 3000.
- `postgres` is private and persists data in the `mvp-postgres-production` volume.

Use `compose.coolify.yaml` for production. The root `compose.yaml` remains development-only.

## Prerequisites

1. Point Cloudflare `A` records for `@` and `api` to the Coolify server. Keep them DNS-only until Coolify has issued both TLS certificates.
2. Open ports 80 and 443 to the Coolify proxy.
3. Create the official GitHub OAuth App, enable Device Flow, set its homepage to `https://mostvaluedprogrammer.com`, and retain its client ID. No client secret is used.
4. Generate URL-safe secrets locally:

```bash
openssl rand -hex 32
openssl rand -hex 32
```

Use separate values for `POSTGRES_PASSWORD` and `SESSION_SECRET`. A hexadecimal PostgreSQL password is required because Compose inserts it into `DATABASE_URL` without URL encoding.

## Coolify Resource

1. Create a Docker Compose resource from this Git repository and select the deployment branch.
2. Set the Compose file to `/compose.coolify.yaml`.
3. Configure these environment variables before the first deployment:

```dotenv
POSTGRES_PASSWORD=<first-generated-secret>
SESSION_SECRET=<second-generated-secret>
GITHUB_CLIENT_ID=<official-github-oauth-client-id>
EMAIL_AUTH_ENABLED=false
MINIMUM_CLIENT_VERSION=0.1.0
LATEST_CLIENT_VERSION=0.1.0
```

Cloudflare email variables may remain unset while email authentication is disabled.

4. Assign `https://mostvaluedprogrammer.com:3001` to the `website` service.
5. Assign `https://api.mostvaluedprogrammer.com:3000` to the `backend` service.
6. Do not assign a domain to `postgres` and do not add host port mappings.
7. Deploy. The backend container applies pending PostgreSQL migrations before starting the API.

The ports in Coolify's domain fields select the destination container ports. Public visitors still use standard HTTPS without a port suffix.

## Verification

Confirm that both services are healthy:

```bash
curl --fail https://api.mostvaluedprogrammer.com/health
curl --fail https://api.mostvaluedprogrammer.com/v1/client-version
curl --fail --head https://mostvaluedprogrammer.com/sign-in
```

The API health response is `{"status":"ok"}`. The sign-in page initially displays GitHub only.

Test the installed terminal client:

```bash
mvp login
mvp whoami
```

Bare `mvp login` should open `https://mostvaluedprogrammer.com/sign-in` and hand the completed GitHub session back to the terminal.

After Coolify has valid certificates, Cloudflare proxying may be enabled for both DNS records with SSL/TLS mode set to Full (strict).

## Enabling Email Later

Set all four variables and redeploy both application services:

```dotenv
EMAIL_AUTH_ENABLED=true
CLOUDFLARE_ACCOUNT_ID=<account-id>
CLOUDFLARE_EMAIL_API_TOKEN=<email-service-token>
EMAIL_FROM=login@mostvaluedprogrammer.com
```

The backend refuses to start with email enabled and incomplete provider credentials. Disabling the flag hides the website email form and rejects all email authentication endpoints.

## Operations

- Back up the `mvp-postgres-production` volume or configure scheduled PostgreSQL backups before accepting production data.
- Keep automatic deployments limited to reviewed branches.
- Rotate `SESSION_SECRET` only with a planned global sign-out; changing it invalidates deterministic replay-safe flow sessions.
- Never expose port 5432 or commit Coolify environment values.
- Publish a CLI release before raising `MINIMUM_CLIENT_VERSION`; see [CLI Releases](releases.md).
