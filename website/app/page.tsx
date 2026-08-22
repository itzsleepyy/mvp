import Link from "next/link";
import { InstallCommand } from "@/components/install-command";
import { TerminalPreview } from "@/components/terminal-preview";

const games = [
  ["01", "STACK OVERFLOW", "Stack clean. Score high."],
  ["02", "THE DAILY PR", "Five letters. Six attempts. One developer word."],
  ["03", "THE DAILY FIX", "Find the bug. Fix the line."],
] as const;

const agents = ["Claude Code", "Codex", "Gemini CLI", "OpenCode"];

export default function Home() {
  return (
    <main id="main-content">
      <section className="hero shell">
        <p className="eyebrow">MOST VALUED PROGRAMMER / 0.1.0</p>
        <h1>The arcade for programmers.</h1>
        <p className="hero-copy">
          Quick terminal games for the time between prompts. Compete for Daily MVP,
          then jump back when your coding agent needs you.
        </p>
        <div className="hero-actions">
          <Link className="button primary" href="#install">
            Download MVP
          </Link>
          <Link className="button secondary" href="/leaderboard">
            View leaderboard
          </Link>
        </div>
      </section>

      <section className="preview-section shell" aria-labelledby="preview-title">
        <div className="section-heading">
          <p>RUNS WHERE YOU WORK</p>
          <h2 id="preview-title">No browser tab required.</h2>
        </div>
        <TerminalPreview />
      </section>

      <section className="games shell" aria-labelledby="games-title">
        <div className="section-heading">
          <p>THREE WAYS TO WIN</p>
          <h2 id="games-title">Built for short waits.</h2>
        </div>
        <div className="game-list">
          {games.map(([number, title, description]) => (
            <article key={title}>
              <span>{number}</span>
              <h3>{title}</h3>
              <p>{description}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="agents shell" aria-labelledby="agents-title">
        <p className="eyebrow">PLAY WHILE THEY WORK</p>
        <h2 id="agents-title">MVP pauses when your agent needs you.</h2>
        <ul aria-label="Supported coding agents">
          {agents.map((agent) => (
            <li key={agent}>{agent}</li>
          ))}
        </ul>
      </section>

      <section className="install shell" id="install" aria-labelledby="install-title">
        <p className="eyebrow">INSTALL FROM SOURCE</p>
        <h2 id="install-title">Your next wait starts now.</h2>
        <p>
          MVP currently installs from its public Cargo workspace. A recent stable Rust
          toolchain is required.
        </p>
        <InstallCommand />
      </section>
    </main>
  );
}
