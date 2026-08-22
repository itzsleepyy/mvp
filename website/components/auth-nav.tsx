"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import type { AuthUser } from "@/lib/auth-types";

export function AuthNav() {
  const [state, setState] = useState<"loading" | "out" | "in">("loading");
  const [user, setUser] = useState<AuthUser | null>(null);

  useEffect(() => {
    let active = true;
    fetch("/api/auth/session", { cache: "no-store" })
      .then(async (response) => {
        if (!active) return;
        if (!response.ok) {
          setState("out");
          return;
        }
        const body = (await response.json()) as { user: AuthUser };
        setUser(body.user);
        setState("in");
      })
      .catch(() => {
        if (active) setState("out");
      });
    return () => {
      active = false;
    };
  }, []);

  async function signOut() {
    await fetch("/api/auth/logout", { method: "POST" }).catch(() => undefined);
    setUser(null);
    setState("out");
  }

  if (state === "loading") {
    return <span className="inline-block w-[4.5rem]" aria-hidden="true" />;
  }
  if (state === "out") {
    return (
      <Link href="/sign-in" className="whitespace-nowrap transition-colors hover:text-foreground">
        Sign in
      </Link>
    );
  }
  return (
    <span className="flex min-w-0 items-center gap-2 sm:gap-3">
      <span className="max-w-[6ch] truncate text-foreground sm:max-w-[12ch]" title={`@${user?.username}`}>
        @{user?.username}
      </span>
      <button type="button" onClick={signOut} className="whitespace-nowrap transition-colors hover:text-foreground">
        Sign out
      </button>
    </span>
  );
}
