# MVP Website

Minimal Next.js site for the MVP landing page and global leaderboard. Built with Next.js 16, Tailwind CSS v4, and shadcn/ui (Base UI).

## Development

Requires Node.js 22.13 or newer.

```bash
npm install
MVP_API_URL=http://localhost:3000 npm run dev
```

The site runs at `http://localhost:3001`. Start the Phase 4B API separately from `backend/` for live leaderboard data.

To run PostgreSQL, the backend, and the website together from the repository root:

```bash
docker compose up --build
```

The website is available at `http://localhost:3001` and calls the backend over the private Compose network. Set `GITHUB_CLIENT_ID` and a `SESSION_SECRET` of at least 32 characters before starting the stack.

## Environment

- `MVP_API_URL` - server-only MVP backend origin used for leaderboard and authentication requests. Defaults to `http://localhost:3000` for local development. Do not expose it through a `NEXT_PUBLIC_` variable.
- `NEXT_PUBLIC_SITE_URL` - canonical public website origin. Defaults to `http://localhost:3001`.

Neither value is a credential. `MVP_API_URL` may use the backend's private network origin, while the backend's `WEBSITE_URL` and this site's `NEXT_PUBLIC_SITE_URL` must identify the same public HTTPS origin in a deployment. GitHub device authorization and email magic-link exchange are proxied through same-origin route handlers. Backend bearer tokens are stored only in the `mvp_session` cookie, which is `HttpOnly`, `SameSite=Lax`, scoped to `/`, expires with the backend session, and is `Secure` in production. Browser JavaScript receives profile/status data but never the bearer token. Sign-out attempts backend revocation and always clears the local cookie.

The default `mvp login` command opens `/sign-in` with a short-lived browser handoff capability. After website authentication and explicit confirmation when necessary, the backend issues the terminal a separate session; the browser never receives the terminal polling token or terminal session.

## Components

shadcn/ui components live in `components/ui/` and are added with the shadcn CLI:

```bash
npx shadcn@latest add <component>
```

The project currently uses the `table`, `button`, `badge`, and `avatar` components.

### ASCII hero

`components/ascii-logo.tsx` renders the homepage wordmark using the exact FIGlet "doh" ASCII art the package prints in its terminal menu (`package/src/ui.rs` `DOH_LOGO`). Every character is placed in a fixed-width inline-block cell so the proportional Pixelify Sans still lands on a perfect monospace grid, and a `requestAnimationFrame` loop animates a soft brightness wave across the glyphs. The animation respects `prefers-reduced-motion`.

## Quality

```bash
npm run lint
npm run typecheck
npm test
npm run build
```

The production build is independently deployable. Cloudflare Email Service credentials belong only to the backend; the website never receives them.

## Typography

The reference site at `hello.itzsleepyzz.dev` delivers `Pixelify Sans` weight 400 with `MS PGothic, monospace` fallbacks and normal letter spacing. MVP uses that same Google Font through `next/font`, which self-hosts the generated font asset at build time. It is exposed as the `font-pixelify` utility for the wordmark and display headings, while the shadcn theme uses Geist as the UI sans. Pixelify Sans is distributed under the SIL Open Font License.
