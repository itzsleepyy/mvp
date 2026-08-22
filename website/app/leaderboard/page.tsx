import type { Metadata } from "next";
import { LeaderboardView } from "@/components/leaderboard";
import { fetchLeaderboard, parseGame, parsePeriod } from "@/lib/api";

export const metadata: Metadata = {
  title: "Global Leaderboard",
  description: "See today's top programmers and the global MVP rankings.",
  alternates: { canonical: "/leaderboard" },
  openGraph: {
    title: "Global MVP Leaderboard",
    description: "See today's top programmers and compete for Daily MVP.",
    url: "/leaderboard",
    images: [{ url: "/opengraph-image", width: 1200, height: 630 }],
  },
  twitter: {
    card: "summary_large_image",
    title: "Global MVP Leaderboard",
    description: "See today's top programmers and compete for Daily MVP.",
    images: ["/opengraph-image"],
  },
};

export default async function LeaderboardPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const query = await searchParams;
  const period = parsePeriod(query.period);
  const requestedGame = parseGame(query.game);
  const game = period === "weekly" ? "overall" : requestedGame;

  const board = await fetchLeaderboard(period, game).catch(() => null);
  return (
    <LeaderboardView
      board={board}
      period={period}
      game={game}
      unavailable={board === null}
    />
  );
}
