import Link from "next/link";

const REPOSITORY = "https://github.com/itzsleepyy/waitstate";

export function Navbar() {
  return (
    <>
      <a className="skip-link" href="#main-content">
        Skip to content
      </a>
      <header className="site-header">
        <nav className="nav shell" aria-label="Primary navigation">
        <Link className="wordmark" href="/" aria-label="MVP home">
          MVP<span className="cursor" aria-hidden="true" />
        </Link>
        <div className="nav-links">
          <Link href="/leaderboard">Leaderboard</Link>
          <a href={REPOSITORY}>GitHub</a>
          <Link className="nav-download" href="/#install">
            Download
          </Link>
        </div>
        </nav>
      </header>
    </>
  );
}
