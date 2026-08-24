"use client";

import { useEffect, useState } from "react";
import {
  CheckCircle2,
  Copy,
  ExternalLink,
  LoaderCircle,
  Mail,
  SquareTerminal,
} from "lucide-react";
import { GitHubLogo } from "@/assets/icons/githubLogo";
import { useRouter } from "next/navigation";
import { Button, buttonVariants } from "@/components/ui/button";

interface GithubFlow {
  flow_token: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
  expiresAt: number;
}

async function responseError(response: Response): Promise<string> {
  const body = (await response.json().catch(() => null)) as {
    error?: string;
  } | null;
  return body?.error ?? "Sign-in is unavailable right now. Please try again.";
}

export function SignInForm({
  browserToken,
  cliComplete = false,
  emailAuthEnabled = false,
  initialError,
}: {
  browserToken?: string;
  cliComplete?: boolean;
  emailAuthEnabled?: boolean;
  initialError?: string;
}) {
  const router = useRouter();
  const [completed, setCompleted] = useState(cliComplete);
  const [sessionUsername, setSessionUsername] = useState("");
  const [sessionState, setSessionState] = useState<
    "checking" | "none" | "available" | "completing"
  >(browserToken ? "checking" : "none");
  const [handoffError, setHandoffError] = useState("");
  const [githubFlow, setGithubFlow] = useState<GithubFlow | null>(null);
  const [githubState, setGithubState] = useState<
    "idle" | "starting" | "pending" | "complete"
  >("idle");
  const [githubError, setGithubError] = useState(initialError ?? "");
  const [emailState, setEmailState] = useState<"idle" | "sending" | "sent">(
    "idle",
  );
  const [emailError, setEmailError] = useState("");
  const [codeStatus, setCodeStatus] = useState<"idle" | "copied">("idle");

  async function copyCode() {
    if (!githubFlow) return;
    try {
      await navigator.clipboard.writeText(githubFlow.user_code);
      setCodeStatus("copied");
      window.setTimeout(() => setCodeStatus("idle"), 1_500);
    } catch {
      setCodeStatus("idle");
    }
  }

  useEffect(() => {
    if (!browserToken) return;

    let cancelled = false;
    fetch("/api/auth/session", { cache: "no-store" })
      .then(async (response) => {
        if (!response.ok) return null;
        return (await response.json()) as { user?: { username?: string } };
      })
      .then((body) => {
        if (cancelled) return;
        if (body?.user?.username) {
          setSessionUsername(body.user.username);
          setSessionState("available");
        } else {
          setSessionState("none");
        }
      })
      .catch(() => {
        if (!cancelled) setSessionState("none");
      });

    return () => {
      cancelled = true;
    };
  }, [browserToken]);

  useEffect(() => {
    if (!githubFlow || githubState !== "pending") return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;

    const poll = async () => {
      if (Date.now() >= githubFlow.expiresAt) {
        setGithubError("This GitHub code expired. Please start again.");
        setGithubFlow(null);
        setGithubState("idle");
        return;
      }

      try {
        const response = await fetch("/api/auth/github/poll", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            poll_token: githubFlow.flow_token,
            ...(browserToken ? { browser_token: browserToken } : {}),
          }),
        });
        const body = (await response.json()) as {
          status?: string;
          retry_after?: number;
          error?: string;
        };
        if (!response.ok && response.status !== 202) {
          throw new Error(body.error ?? "GitHub sign-in could not be completed.");
        }
        if (body.status === "complete") {
          setGithubState("complete");
          if (browserToken) {
            setCompleted(true);
          } else {
            router.push("/leaderboard");
            router.refresh();
          }
          return;
        }
        const retryAfter = Math.max(1, body.retry_after ?? githubFlow.interval);
        if (!cancelled) timer = setTimeout(poll, retryAfter * 1_000);
      } catch (error) {
        if (cancelled) return;
        setGithubError(
          error instanceof Error
            ? error.message
            : "GitHub sign-in could not be completed.",
        );
        setGithubFlow(null);
        setGithubState("idle");
      }
    };

    timer = setTimeout(poll, Math.max(1, githubFlow.interval) * 1_000);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [browserToken, githubFlow, githubState, router]);

  async function completeExistingSession() {
    if (!browserToken) return;
    setHandoffError("");
    setSessionState("completing");
    try {
      const response = await fetch("/api/auth/handoff/complete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ browser_token: browserToken }),
      });
      if (!response.ok) throw new Error(await responseError(response));
      setCompleted(true);
    } catch (error) {
      setHandoffError(
        error instanceof Error
          ? error.message
          : "Terminal sign-in could not be completed.",
      );
      setSessionState("available");
    }
  }

  async function startGithub() {
    setGithubError("");
    setGithubState("starting");
    try {
      const response = await fetch("/api/auth/github/start", { method: "POST" });
      if (!response.ok) throw new Error(await responseError(response));
      const flow = (await response.json()) as Omit<GithubFlow, "expiresAt">;
      setGithubFlow({
        ...flow,
        expiresAt: Date.now() + flow.expires_in * 1_000,
      });
      setGithubState("pending");
    } catch (error) {
      setGithubError(
        error instanceof Error
          ? error.message
          : "GitHub sign-in is unavailable right now.",
      );
      setGithubState("idle");
    }
  }

  async function startEmail(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setEmailError("");
    setEmailState("sending");
    const data = new FormData(event.currentTarget);
    try {
      const response = await fetch("/api/auth/email/start", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          email: data.get("email"),
          ...(browserToken ? { browser_token: browserToken } : {}),
        }),
      });
      if (!response.ok) throw new Error(await responseError(response));
      setEmailState("sent");
    } catch (error) {
      setEmailError(
        error instanceof Error
          ? error.message
          : "Email sign-in is unavailable right now.",
      );
      setEmailState("idle");
    }
  }

  if (completed) {
    return (
      <section className="rounded-xl border border-primary/30 bg-card p-8 text-center shadow-[0_0_60px_-20px] shadow-primary/40 sm:p-10">
        <div className="mx-auto flex size-14 items-center justify-center rounded-full border border-primary/40 bg-primary/10">
          <CheckCircle2 aria-hidden="true" className="size-7 text-primary" />
        </div>
        <h2 className="mt-6 font-pixelify text-2xl">Terminal sign-in complete</h2>
        <p
          className="mx-auto mt-3 max-w-xs text-sm leading-6 text-muted-foreground"
          role="status"
        >
          Return to your terminal to continue. You can close this browser tab.
        </p>
      </section>
    );
  }

  return (
    <div>
      {browserToken && (
        <section className="mb-6 rounded-xl border border-primary/30 bg-card p-6 shadow-[0_0_60px_-25px] shadow-primary/50 sm:p-8">
          <div className="flex items-center gap-3">
            <SquareTerminal aria-hidden="true" className="size-5 text-primary" />
            <h2 className="text-sm font-medium tracking-widest uppercase">
              Terminal sign-in
            </h2>
          </div>
          {sessionState === "checking" ? (
            <p className="mt-4 flex items-center gap-2 text-sm text-muted-foreground" role="status">
              <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
              Checking your website session
            </p>
          ) : sessionState === "available" || sessionState === "completing" ? (
            <div className="mt-4 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-sm text-muted-foreground">
                Confirm this terminal sign-in with your existing website account.
              </p>
              <Button
                onClick={completeExistingSession}
                disabled={sessionState === "completing"}
              >
                {sessionState === "completing" && (
                  <LoaderCircle aria-hidden="true" className="animate-spin" />
                )}
                Continue as @{sessionUsername}
              </Button>
            </div>
          ) : (
            <p className="mt-4 text-sm leading-6 text-muted-foreground">
              Sign in below to connect this terminal. Nothing will be connected
              until you complete a sign-in method.
            </p>
          )}
          {handoffError && (
            <p className="mt-4 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive" role="alert">
              {handoffError}
            </p>
          )}
          <a
            href="/sign-in"
            className="mt-4 inline-block text-xs text-muted-foreground underline underline-offset-4 hover:text-foreground"
          >
            Cancel terminal sign-in
          </a>
        </section>
      )}

      <div
        className={
          emailAuthEnabled ? "grid items-start gap-6 md:grid-cols-2" : "max-w-xl"
        }
      >
      {githubFlow ? (
        <section className="flex min-h-80 flex-col rounded-xl border border-border bg-card p-6 shadow-[0_0_60px_-30px] shadow-primary/30 sm:p-8">
          <div className="mb-8 flex items-center gap-3">
            <div className="flex size-9 items-center justify-center rounded-lg border border-border bg-background">
              <GitHubLogo width="20px" height="20px" color="currentColor" className="text-primary" />
            </div>
            <h2 className="text-lg font-medium">GitHub</h2>
          </div>
          <div className="flex flex-1 flex-col justify-between gap-6">
            <div>
              <p className="text-sm text-muted-foreground">
                Copy this one-time code, then continue to GitHub.
              </p>
              <div className="mt-5 flex items-center justify-between gap-3 rounded-lg border border-dashed border-primary/40 bg-background px-4 py-3.5">
                <p className="truncate font-mono text-3xl tracking-[0.18em] text-primary select-all">
                  {githubFlow.user_code}
                </p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={copyCode}
                  aria-live="polite"
                >
                  {codeStatus === "copied" ? (
                    <CheckCircle2 aria-hidden="true" />
                  ) : (
                    <Copy aria-hidden="true" />
                  )}
                  {codeStatus === "copied" ? "Copied" : "Copy"}
                </Button>
              </div>
            </div>
            <div className="space-y-4">
              <a
                className={buttonVariants({ className: "w-full" })}
                href={githubFlow.verification_uri}
                target="_blank"
                rel="noreferrer"
              >
                Open GitHub <ExternalLink aria-hidden="true" />
              </a>
              <p className="flex items-center justify-center gap-2 text-xs text-muted-foreground" role="status" aria-live="polite">
                <LoaderCircle aria-hidden="true" className="size-3 animate-spin text-primary" />
                Waiting for authorization
              </p>
            </div>
          </div>
        </section>
      ) : (
        <div className="flex justify-center">
          <button
            type="button"
            onClick={startGithub}
            disabled={githubState === "starting"}
            className="inline-flex items-center gap-2.5 rounded-full border border-border bg-card py-1.5 pr-5 pl-4 text-sm font-medium shadow-sm transition-colors hover:border-primary/50 hover:bg-accent focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none disabled:pointer-events-none disabled:opacity-50"
          >
            <GitHubLogo width="22px" height="22px" color="currentColor" className="text-primary" />
            {githubState === "starting" ? "Connecting..." : "Continue with GitHub"}
            {githubState === "starting" && (
              <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
            )}
          </button>
        </div>
      )}
      {githubError && (
        <p className="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive md:col-span-2" role="alert">
          {githubError}
        </p>
      )}

      {emailAuthEnabled && (
        <section className="flex min-h-80 flex-col rounded-xl border border-border bg-card p-6 shadow-[0_0_60px_-30px] shadow-primary/30 sm:p-8">
        <div className="mb-8 flex items-center gap-3">
          <div className="flex size-9 items-center justify-center rounded-lg border border-border bg-background">
            <Mail aria-hidden="true" className="size-5 text-primary" />
          </div>
          <h2 className="text-lg font-medium">Email</h2>
        </div>
        {emailState === "sent" ? (
          <div className="flex flex-1 flex-col justify-center">
            <p className="text-lg font-medium">Check your email</p>
            <p className="mt-2 text-sm leading-6 text-muted-foreground" role="status">
              If an account can sign in with that address, a one-time link is on
              its way. You can close this page.
            </p>
          </div>
        ) : (
          <form className="flex flex-1 flex-col justify-between gap-6" onSubmit={startEmail}>
            <div>
              <label htmlFor="email" className="text-sm text-muted-foreground">
                Email address
              </label>
              <input
                id="email"
                name="email"
                type="email"
                autoComplete="email"
                required
                placeholder="you@example.com"
                className="mt-3 h-10 w-full rounded-lg border border-input bg-background px-3 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/30"
              />
              <p className="mt-3 text-xs leading-5 text-muted-foreground">
                We will send a short-lived sign-in link.
              </p>
            </div>
            <Button type="submit" variant="outline" disabled={emailState === "sending"} className="w-full">
              {emailState === "sending" && <LoaderCircle aria-hidden="true" className="animate-spin" />}
              Send magic link
            </Button>
          </form>
        )}
        {emailError && (
          <p className="mt-4 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive" role="alert">
            {emailError}
          </p>
        )}
        </section>
      )}
      </div>
    </div>
  );
}
