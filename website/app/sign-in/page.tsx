import type { Metadata } from "next";
import { SignInForm } from "@/components/sign-in-form";
import { emailAuthEnabled } from "@/lib/features";
import { validBrowserToken } from "@/lib/handoff";

export const metadata: Metadata = {
  title: "Sign in",
  description: "Sign in to MVP and connect your terminal scores.",
  alternates: { canonical: "/sign-in" },
  robots: { index: false, follow: false },
};

const errors: Record<string, string> = {
  invalid_link: "That email sign-in link is invalid or expired. Request a new one.",
  rate_limited: "Too many attempts. Please wait before requesting another link.",
  unavailable: "Sign-in is unavailable right now. Please try again.",
  handoff_failed:
    "Terminal sign-in could not be completed. Return to your terminal and try again.",
};

export default async function SignInPage({
  searchParams,
}: {
  searchParams: Promise<{
    error?: string | string[];
    handoff?: string | string[];
    cli?: string | string[];
  }>;
}) {
  const params = await searchParams;
  const error = params.error;
  const errorCode = Array.isArray(error) ? error[0] : error;
  const handoffParam = Array.isArray(params.handoff)
    ? params.handoff[0]
    : params.handoff;
  const browserToken = validBrowserToken(handoffParam)
    ? handoffParam
    : undefined;
  const cli = Array.isArray(params.cli) ? params.cli[0] : params.cli;

  return (
    <main id="main-content">
      <div className="mx-auto w-full max-w-4xl px-4 py-14 sm:px-6 sm:py-20">
        <header className="mb-10 max-w-xl">
          <p className="text-xs tracking-widest text-primary uppercase">Account</p>
          <h1 className="mt-2 font-pixelify text-5xl leading-none sm:text-6xl">
            Sign in to MVP
          </h1>
          <p className="mt-5 text-sm leading-6 text-muted-foreground">
            Keep your terminal scores connected across machines and compete on
            the global leaderboard.
          </p>
        </header>
        <SignInForm
          browserToken={browserToken}
          cliComplete={cli === "complete"}
          emailAuthEnabled={emailAuthEnabled()}
          initialError={errorCode ? errors[errorCode] ?? errors.invalid_link : undefined}
        />
      </div>
    </main>
  );
}
