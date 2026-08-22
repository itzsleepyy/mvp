import Link from "next/link";

const REPOSITORY = "https://github.com/itzsleepyy/waitstate";

export function Navbar() {
  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background/80 backdrop-blur-md">
      <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between px-4 sm:px-6">
        <Link
          href="/"
          className="font-pixelify text-xl leading-none tracking-wide text-foreground"
          aria-label="MVP home"
        >
          MVP
        </Link>
        <nav
          className="flex items-center gap-6 text-sm text-muted-foreground"
          aria-label="Primary navigation"
        >
          <Link
            href="/about"
            className="transition-colors hover:text-foreground"
          >
            About
          </Link>
          <Link
            href="/leaderboard"
            className="transition-colors hover:text-foreground"
          >
            Leaderboard
          </Link>
          <a
            href={REPOSITORY}
            className="transition-colors hover:text-foreground"
          >
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}
