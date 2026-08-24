"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useCallback, useEffect, useRef } from "react";
import { AuthNav } from "@/components/auth-nav";

const LINKS = [
  { href: "/about", label: "About" },
  { href: "/leaderboard", label: "Leaderboard" },
];

type TabKey = "about" | "leaderboard" | "auth";

export function NavLinks() {
  const pathname = usePathname() ?? "/";
  const pillRef = useRef<HTMLSpanElement>(null);
  const aboutRef = useRef<HTMLAnchorElement>(null);
  const leaderboardRef = useRef<HTMLAnchorElement>(null);
  const authRef = useRef<HTMLSpanElement>(null);

  const activeKey: TabKey | null =
    pathname === "/about" || pathname.startsWith("/about/")
      ? "about"
      : pathname === "/leaderboard" || pathname.startsWith("/leaderboard/")
        ? "leaderboard"
        : pathname === "/sign-in" || pathname.startsWith("/sign-in/")
          ? "auth"
          : null;

  const positionPill = useCallback(
    (animate: boolean) => {
      const pill = pillRef.current;
      if (!pill) return;
      const target =
        activeKey === "about"
          ? aboutRef.current
          : activeKey === "leaderboard"
            ? leaderboardRef.current
            : activeKey === "auth"
              ? authRef.current
              : null;
      if (!target) {
        pill.dataset.visible = "false";
        return;
      }
      if (!animate) {
        pill.style.transition = "none";
      }
      pill.dataset.visible = "true";
      pill.style.transform = `translateX(${target.offsetLeft}px)`;
      pill.style.width = `${target.offsetWidth}px`;
      if (!animate) {
        void pill.offsetHeight;
        pill.style.transition = "";
      }
    },
    [activeKey],
  );

  useEffect(() => {
    positionPill(false);
    const onResize = () => positionPill(false);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [positionPill]);

  return (
    <nav
      aria-label="Primary navigation"
      className="nav-tabs flex h-11 items-center gap-1 rounded-full border border-border bg-background/80 p-1 shadow-[0_8px_30px_-15px_rgba(0,0,0,0.8)] backdrop-blur-md"
    >
      <span ref={pillRef} aria-hidden="true" className="nav-tabs-pill" />
      {LINKS.map((link) => {
        const active = activeKey === link.href.slice(1);
        return (
          <Link
            key={link.href}
            ref={link.href === "/about" ? aboutRef : leaderboardRef}
            href={link.href}
            aria-current={active ? "page" : undefined}
            className={`nav-tabs-item rounded-full px-3 py-1.5 text-[11px] sm:px-5 sm:text-sm ${
              active
                ? "text-foreground"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {link.label}
          </Link>
        );
      })}
      <span aria-hidden="true" className="relative z-1 h-5 w-px bg-border" />
      <span
        ref={authRef}
        className={`nav-tabs-item flex items-center py-1.5 pr-3 pl-3 text-[11px] sm:pr-5 sm:pl-5 sm:text-sm ${
          activeKey === "auth"
            ? "text-foreground"
            : "text-muted-foreground hover:text-foreground"
        }`}
      >
        <AuthNav />
      </span>
    </nav>
  );
}
