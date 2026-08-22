import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LeaderboardLoading, LeaderboardView } from "@/components/leaderboard";
import type { Leaderboard } from "@/lib/api";

const board: Leaderboard = {
  period: "daily",
  from: "2026-08-20",
  through: "2026-08-20",
  game_id: null,
  entries: [
    {
      rank: 1,
      user: {
        id: "1",
        username: "alice",
        display_name: "Alice",
        avatar_url: "https://avatars.example/alice.png",
      },
      points: 9_842,
    },
    {
      rank: 2,
      user: {
        id: "2",
        username: "alex",
        display_name: "alex",
        avatar_url: null,
      },
      points: 9_410,
    },
  ],
};

describe("leaderboard", () => {
  it("renders API rankings in a table with avatars and MVP points", () => {
    render(<LeaderboardView board={board} period="daily" game="overall" />);

    expect(screen.getByRole("heading", { name: "Daily MVP" })).toBeInTheDocument();
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Rank" })).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Programmer" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "MVP" })).toBeInTheDocument();
    expect(screen.getByText("Alice")).toBeInTheDocument();
    expect(screen.getByText("@alice")).toBeInTheDocument();
    expect(screen.getAllByText("AL")).toHaveLength(2);
    expect(screen.getByText("9,842")).toBeInTheDocument();
  });

  it("renders loading, empty, and API failure states", () => {
    const { rerender } = render(<LeaderboardLoading />);
    expect(screen.getByText(/loading leaderboard/i)).toBeInTheDocument();

    rerender(
      <LeaderboardView
        board={{ ...board, entries: [] }}
        period="daily"
        game="overall"
      />,
    );
    expect(screen.getByRole("heading", { name: "No scores yet" })).toBeInTheDocument();

    rerender(
      <LeaderboardView board={null} period="daily" game="overall" unavailable />,
    );
    expect(
      screen.getByRole("heading", { name: "Leaderboard unavailable" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/played locally/i)).toBeInTheDocument();
  });

  it("uses URL-backed period and game filters", () => {
    render(<LeaderboardView board={board} period="daily" game="overall" />);

    expect(screen.getByRole("link", { name: "Weekly" })).toHaveAttribute(
      "href",
      "/leaderboard?period=weekly",
    );
    expect(screen.getByRole("link", { name: "Stack Overflow" })).toHaveAttribute(
      "href",
      "/leaderboard?period=daily&game=stack_overflow",
    );
    expect(screen.getByRole("link", { name: "Daily" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});
