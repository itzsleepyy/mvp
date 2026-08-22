# MVP Website

Minimal Next.js site for the MVP landing page and global leaderboard. Built with Next.js 16, Tailwind CSS v4, and shadcn/ui (Base UI).

## Development

Requires Node.js 22.13 or newer.

```bash
npm install
NEXT_PUBLIC_MVP_API_URL=http://localhost:3000 npm run dev
```

The site runs at `http://localhost:3001`. Start the Phase 4B API separately from `backend/` for live leaderboard data.

## Environment

- `NEXT_PUBLIC_MVP_API_URL` - MVP backend origin. Defaults to `http://localhost:3000` for local development.
- `NEXT_PUBLIC_SITE_URL` - canonical public website origin. Defaults to `http://localhost:3001`.

Neither value is a credential. The website calls only public leaderboard endpoints and does not implement authentication or duplicate backend logic.

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

The production build is independently deployable. Cloudflare/Coolify configuration is intentionally deferred until hosting is selected.

## Typography

The reference site at `hello.itzsleepyzz.dev` delivers `Pixelify Sans` weight 400 with `MS PGothic, monospace` fallbacks and normal letter spacing. MVP uses that same Google Font through `next/font`, which self-hosts the generated font asset at build time. It is exposed as the `font-pixelify` utility for the wordmark and display headings, while the shadcn theme uses Geist as the UI sans. Pixelify Sans is distributed under the SIL Open Font License.
