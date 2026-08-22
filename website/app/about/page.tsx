import type { Metadata } from "next";
import { InstallPill } from "@/components/install-pill";

export const metadata: Metadata = {
  title: "About",
  description:
    "MVP - Most Valued Programmer. A competitive terminal arcade for the time between coding-agent prompts.",
  alternates: { canonical: "/about" },
};

const games = [
  {
    number: "01",
    title: "Stack Overflow",
    blurb:
      "Drop moving call-stack frames, trim the overhang, and build as high as possible before a full miss.",
  },
  {
    number: "02",
    title: "The Daily PR",
    blurb:
      "Find the shared five-letter word in six guesses. Fewer guesses win; speed breaks ties.",
  },
  {
    number: "03",
    title: "The Daily Fix",
    blurb:
      "Correct the broken line in a shared daily snippet. Fast fixes win, while wrong submissions add penalties.",
  },
] as const;

const agents = ["Claude Code", "Codex", "Gemini CLI", "OpenCode"] as const;

function Row({
  index,
  title,
  children,
}: {
  index: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="border-t border-border py-8">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:gap-8">
        <span className="text-xs text-primary">{index}</span>
        <h3 className="text-xl font-medium text-foreground">{title}</h3>
      </div>
      <div className="mt-3 max-w-2xl text-sm text-muted-foreground sm:pl-[3.25rem]">
        {children}
      </div>
    </div>
  );
}

function Section({ eyebrow, title, children }: { eyebrow: string; title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2" aria-labelledby={title.toLowerCase().replace(/\s+/g, "-")}>
      <p className="text-xs tracking-widest text-primary uppercase">{eyebrow}</p>
      <h2 id={title.toLowerCase().replace(/\s+/g, "-")} className="text-2xl font-medium text-foreground">
        {title}
      </h2>
      {children}
    </section>
  );
}

export default function AboutPage() {
  return (
    <main id="main-content">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-16 px-4 py-16 sm:px-6">
        <header className="flex flex-col gap-2">
          <p className="text-xs tracking-widest text-primary uppercase">About</p>
          <h1 className="font-pixelify text-5xl leading-none text-foreground sm:text-6xl">
            About MVP
          </h1>
        </header>

        <p className="-mt-8 max-w-2xl text-base text-muted-foreground sm:text-lg">
          MVP turns the short waits while your coding agent works into quick
          terminal games. Agent lifecycle integrations pause the game the moment
          a prompt needs you, and resume it the moment you are back — no run lost.
        </p>

        <Section eyebrow="The games" title="Three ways to win">
          <div className="mt-4">
            {games.map((game) => (
              <Row key={game.number} index={game.number} title={game.title}>
                {game.blurb}
              </Row>
            ))}
          </div>
        </Section>

        <Section eyebrow="Built around your agents" title="Plays where you work">
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
            MVP watches the hooks and plugins of the coding agents you already
            run, and pauses automatically whenever one of them needs your input.
          </p>
          <ul className="mt-4 flex flex-wrap gap-x-8 gap-y-2 text-sm text-muted-foreground">
            {agents.map((agent) => (
              <li key={agent} className="flex items-baseline gap-2">
                <span aria-hidden="true" className="text-primary">/</span>
                {agent}
              </li>
            ))}
          </ul>
        </Section>

        <Section eyebrow="Compete daily" title="Race the leaderboard">
          <div className="mt-2 flex flex-col gap-4 text-sm text-muted-foreground sm:pl-0">
            <p className="max-w-2xl">
              Local play works without an account. Sign in with GitHub to submit
              scores, climb the Daily, Weekly, and All-Time leaderboards, and
              compete for Daily MVP.
            </p>
            <p className="max-w-2xl">
              Game traffic never includes source code, prompts, repositories,
              terminal contents, or coding-agent output.
            </p>
          </div>
        </Section>

        <section
          className="flex flex-col items-center gap-6 border-t border-border py-16 text-center"
          aria-labelledby="install-title"
        >
          <h2 id="install-title" className="font-pixelify text-3xl text-foreground sm:text-4xl">
            Your next wait starts now.
          </h2>
          <InstallPill />
          <p className="text-xs text-muted-foreground">
            Requires a recent stable Rust toolchain.
          </p>
        </section>
      </div>
    </main>
  );
}
