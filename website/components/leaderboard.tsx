import Link from "next/link";
import { cn } from "@/lib/utils";
import { buttonVariants } from "@/components/ui/button";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@/components/ui/avatar";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  GAME_IDS,
  PERIODS,
  leaderboardHref,
  type GameFilter,
  type Leaderboard,
  type Period,
} from "@/lib/api";

const periodLabels: Record<Period, string> = {
  daily: "Daily",
  weekly: "Weekly",
  "all-time": "All Time",
};

const gameLabels: Record<GameFilter, string> = {
  overall: "Overall",
  stack_overflow: "Stack Overflow",
  daily_pr: "Daily PR",
  daily_fix: "Daily Fix",
};

function formatDate(value: string): string {
  return new Intl.DateTimeFormat("en-GB", {
    day: "2-digit",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  })
    .format(new Date(`${value}T00:00:00Z`))
    .toUpperCase();
}

function initials(name: string): string {
  return name.slice(0, 2).toUpperCase();
}

function FilterLink({
  href,
  active,
  children,
}: {
  href: string;
  active: boolean;
  children: React.ReactNode;
}) {
  return (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className={cn(
        buttonVariants({
          variant: active ? "default" : "ghost",
          size: "sm",
        }),
        "h-8 rounded-full px-3",
      )}
    >
      {children}
    </Link>
  );
}

function StateCard({
  title,
  body,
  href,
  cta,
}: {
  title: string;
  body: string;
  href: string;
  cta: string;
}) {
  return (
    <div
      className="flex flex-col items-center gap-4 rounded-lg border border-border px-6 py-16 text-center"
      role="status"
    >
      <h2 className="text-2xl font-medium">{title}</h2>
      <p className="text-sm text-muted-foreground">{body}</p>
      <Link
        href={href}
        className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
      >
        {cta}
      </Link>
    </div>
  );
}

export function LeaderboardView({
  board,
  period,
  game,
  unavailable = false,
}: {
  board: Leaderboard | null;
  period: Period;
  game: GameFilter;
  unavailable?: boolean;
}) {
  const title = game === "overall" ? `${periodLabels[period]} MVP` : gameLabels[game];

  return (
    <main id="main-content">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-10 px-4 py-16 sm:px-6">
        <header className="flex flex-col gap-2">
          <p className="text-xs tracking-widest text-primary uppercase">
            Global rankings
          </p>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <h1 className="font-pixelify text-5xl leading-none text-foreground sm:text-6xl">
              {title}
            </h1>
            <p className="text-sm text-muted-foreground">
              {board ? formatDate(board.through) : "Live MVP points"}
            </p>
          </div>
        </header>

        <nav aria-label="Leaderboard period" className="flex flex-wrap gap-1">
          {PERIODS.map((item) => (
            <FilterLink
              key={item}
              href={leaderboardHref(item, item === "weekly" ? "overall" : game)}
              active={period === item}
            >
              {periodLabels[item]}
            </FilterLink>
          ))}
        </nav>

        <nav aria-label="Leaderboard game" className="flex flex-wrap gap-1">
          {GAME_IDS.map((item) => {
            const targetPeriod =
              period === "weekly" && item !== "overall" ? "all-time" : period;
            return (
              <FilterLink
                key={item}
                href={leaderboardHref(targetPeriod, item)}
                active={game === item}
              >
                {gameLabels[item]}
              </FilterLink>
            );
          })}
        </nav>

        {unavailable ? (
          <StateCard
            title="Leaderboard unavailable"
            body="MVP can still be downloaded and played locally."
            href="/#install"
            cta="Install MVP"
          />
        ) : board?.entries.length === 0 ? (
          <StateCard
            title="No scores yet"
            body="Today's first MVP spot is still open."
            href="/#install"
            cta="Claim it"
          />
        ) : board ? (
          <section aria-label={`${title} rankings`}>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-16 text-muted-foreground">
                    Rank
                  </TableHead>
                  <TableHead>Programmer</TableHead>
                  <TableHead className="text-right">MVP</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {board.entries.map((entry) => {
                  const displayName =
                    entry.user.display_name || entry.user.username;
                  return (
                    <TableRow key={entry.user.id}>
                      <TableCell className="font-medium tabular-nums text-muted-foreground">
                        {entry.rank}
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-3">
                          <Avatar>
                            {entry.user.avatar_url ? (
                              <AvatarImage
                                src={entry.user.avatar_url}
                                alt={`${displayName} avatar`}
                              />
                            ) : null}
                            <AvatarFallback>
                              {initials(displayName)}
                            </AvatarFallback>
                          </Avatar>
                          <div className="flex flex-col leading-tight">
                            <span className="font-medium">{displayName}</span>
                            {displayName !== entry.user.username ? (
                              <span className="text-xs text-muted-foreground">
                                @{entry.user.username}
                              </span>
                            ) : null}
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-medium tabular-nums">
                        {entry.points.toLocaleString("en-US")}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </section>
        ) : null}
      </div>
    </main>
  );
}

export function LeaderboardLoading() {
  return (
    <main id="main-content" aria-busy="true">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-10 px-4 py-16 sm:px-6">
        <header className="flex flex-col gap-2">
          <p className="text-xs tracking-widest text-primary uppercase">
            Global rankings
          </p>
          <h1 className="font-pixelify text-5xl leading-none text-foreground sm:text-6xl">
            Daily MVP
          </h1>
        </header>
        <div
          className="flex flex-col items-center gap-4 rounded-lg border border-border px-6 py-16 text-center"
          role="status"
        >
          <span className="text-sm text-muted-foreground">
            Loading leaderboard...
          </span>
        </div>
      </div>
    </main>
  );
}
