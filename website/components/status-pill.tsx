"use client";

import type { CSSProperties } from "react";
import { ThinkingOrb } from "thinking-orbs-universal/react";

export function StatusPill() {
  return (
    <div
      className="hidden h-11 items-center gap-2.5 rounded-full bg-background/80 px-4 shadow-[0_8px_30px_-15px_rgba(0,0,0,0.8)] backdrop-blur-md sm:flex"
      style={{ "--orb-color-dark": "var(--primary)" } as CSSProperties}
    >
      <ThinkingOrb
        state="working"
        size={20}
        theme="dark"
        aria-label="MVP is live"
      />
      <span className="text-xs font-medium tracking-widest text-primary uppercase">
        Live
      </span>
    </div>
  );
}
