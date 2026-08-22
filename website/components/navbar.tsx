import Link from "next/link";
import { AuthNav } from "@/components/auth-nav";

const REPOSITORY = "https://github.com/itzsleepyy/waitstate";

export function Navbar() {
  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background/80 backdrop-blur-md">
      <div className="mx-auto flex h-14 w-full max-w-5xl items-center justify-between px-2 sm:px-6">
        <Link
          href="/"
          className="font-pixelify text-xl leading-none tracking-wide text-foreground"
          aria-label="MVP home"
        >
          MVP
        </Link>
        <nav
          className="flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground sm:gap-6 sm:text-sm"
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
          <AuthNav />
        </nav>
      </div>
    </header>
  );
}
