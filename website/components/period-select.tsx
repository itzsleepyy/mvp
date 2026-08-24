"use client";

import { useRouter } from "next/navigation";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  PERIODS,
  leaderboardHref,
  type GameFilter,
  type Period,
} from "@/lib/api";

const periodLabels: Record<Period, string> = {
  daily: "Daily",
  weekly: "Weekly",
  "all-time": "All Time",
};

export function PeriodSelect({
  period,
  game,
}: {
  period: Period;
  game: GameFilter;
}) {
  const router = useRouter();
  return (
    <Select
      items={periodLabels}
      value={period}
      onValueChange={(value) => {
        const next = value as Period;
        if (next !== period) {
          router.push(
            leaderboardHref(next, next === "weekly" ? "overall" : game),
          );
        }
      }}
    >
      <SelectTrigger
        aria-label="Leaderboard period"
        className="rounded-full border-border bg-background/80 backdrop-blur-md"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false}>
        {PERIODS.map((item) => (
          <SelectItem key={item} value={item}>
            {periodLabels[item]}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
