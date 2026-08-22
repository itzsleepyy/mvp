import Link from "next/link";
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
    <main className="leaderboard-page shell" id="main-content">
      <header className="leaderboard-heading">
        <p className="eyebrow">GLOBAL RANKINGS</p>
        <h1>{title}</h1>
        <p>{board ? formatDate(board.through) : "LIVE MVP POINTS"}</p>
      </header>

      <nav className="filter-row" aria-label="Leaderboard period">
        {PERIODS.map((item) => (
          <Link
            key={item}
            href={leaderboardHref(item, item === "weekly" ? "overall" : game)}
            aria-current={period === item ? "page" : undefined}
          >
            {periodLabels[item]}
          </Link>
        ))}
      </nav>

      <nav className="filter-row games-filter" aria-label="Leaderboard game">
        {GAME_IDS.map((item) => {
          const targetPeriod = period === "weekly" && item !== "overall" ? "all-time" : period;
          return (
            <Link
              key={item}
              href={leaderboardHref(targetPeriod, item)}
              aria-current={game === item ? "page" : undefined}
            >
              {gameLabels[item]}
            </Link>
          );
        })}
      </nav>

      {unavailable ? (
        <div className="leaderboard-state" role="status">
          <h2>Leaderboard unavailable.</h2>
          <p>MVP can still be downloaded and played locally.</p>
          <Link href="/#install">Install MVP</Link>
        </div>
      ) : board?.entries.length === 0 ? (
        <div className="leaderboard-state" role="status">
          <h2>No scores yet.</h2>
          <p>Today&apos;s first MVP spot is still open.</p>
          <Link href="/#install">Claim it</Link>
        </div>
      ) : board ? (
        <section aria-label={`${title} rankings`}>
          <div className="leaderboard-labels" aria-hidden="true">
            <span># / programmer</span>
            <span>MVP</span>
          </div>
          <ol className="leaderboard-list">
            {board.entries.map((entry) => {
              const displayName = entry.user.display_name || entry.user.username;
              return (
                <li key={entry.user.id} className={entry.rank <= 3 ? "podium" : undefined}>
                  <span className="rank">{String(entry.rank).padStart(2, "0")}</span>
                  <span className="avatar">
                    {entry.user.avatar_url ? (
                      <img
                        src={entry.user.avatar_url}
                        alt={`${displayName} avatar`}
                        width="40"
                        height="40"
                        loading="lazy"
                        referrerPolicy="no-referrer"
                      />
                    ) : (
                      <span aria-hidden="true">{initials(displayName)}</span>
                    )}
                  </span>
                  <span className="programmer">
                    <strong>{displayName}</strong>
                    {displayName !== entry.user.username ? (
                      <small>@{entry.user.username}</small>
                    ) : null}
                  </span>
                  <strong className="points">{entry.points.toLocaleString("en-US")}</strong>
                </li>
              );
            })}
          </ol>
        </section>
      ) : null}
    </main>
  );
}

export function LeaderboardLoading() {
  return (
    <main className="leaderboard-page shell" id="main-content" aria-busy="true">
      <header className="leaderboard-heading">
        <p className="eyebrow">GLOBAL RANKINGS</p>
        <h1>Daily MVP</h1>
        <p>LOADING LIVE MVP POINTS</p>
      </header>
      <div className="leaderboard-state" role="status">
        <span className="loading-cursor" aria-hidden="true" /> Loading leaderboard...
      </div>
    </main>
  );
}
