# Architecture

MVP keeps game simulation independent of terminal rendering:

```text
Input -> Application -> Game State -> Simulation -> Rendering
```

## Repository Layout

```text
Cargo.toml              workspace manifest
package/Cargo.toml      mvp package manifest
package/src/            authoritative Rust implementation
backend/                versioned API and PostgreSQL migrations
website/                independently deployable Next.js public site
```

There is one workspace package and one user-facing binary, both named `mvp`.

## Application Layers

- `main.rs` dispatches CLI commands and runs the event loop.
- `cli.rs` defines game, integration, and internal hook commands.
- `tui.rs` initializes and restores the terminal.
- `event.rs` translates terminal events into application input.
- `app.rs` owns screen transitions, game selection, scores, and agent state.
- `ui.rs` renders menus, games, HUDs, and pause panels with Ratatui.
- `config.rs` stores local names, scores, daily results, and Daily PR progress.
- `game/` contains terminal-independent game logic.
- `agent/` contains the generic lifecycle model and provider adapters.
- `ipc/` transports local agent events to the running application.
- `api/` owns authentication, HTTP DTOs, the durable run queue, and the
  background network worker.

## Online Architecture

```text
headless game result -> application -> durable outbox -> API worker
                                                       |
GitHub Device Flow -> MVP API -> users/runs/rankings -> PostgreSQL
```

The game layer produces typed completion records and never imports HTTP,
authentication, or database types. The synchronous TUI sends owned records to
a worker thread; that worker owns its Tokio runtime, API client, and retry
queue. Leaderboard requests use the same channel and therefore never block
input or rendering.

GitHub authentication is backend-mediated Device Flow. The CLI receives an
MVP session, not a GitHub token; the backend discards GitHub's token after
fetching the public account identity. MVP sessions are stored in the operating
system credential store. Local player names remain independent from the
GitHub-ID-backed online account.

PostgreSQL stores immutable game runs. The client supplies raw result metrics
and an idempotent UUID, while the server supplies identity, timestamps, and
versioned normalized MVP points. No source code, prompts, terminal contents,
repository data, or coding-agent output crosses this boundary.

Daily challenge boundaries use UTC. Version 1 daily identities are
`daily_pr:v1:YYYY-MM-DD` and `daily_fix:v1:YYYY-MM-DD`; submissions must match
the server's current UTC date. The server accepts one Daily PR result and one
Daily Fix result per account and date. Daily MVP counts the best Stack Overflow
run plus those two daily results. Weekly totals start on Monday UTC, and
all-time totals sum the same daily contributions. Normalization rules and the
initial anti-cheat boundary are specified in [Backend](backend.md).

## Design Constraints

- Game simulation is delta-time based and clamps long frames.
- `ActiveGame` gives the app one shared interface across real-time and turn-based games.
- Only the playing state advances game time; pauses and undersized terminals freeze runs.
- Agent adapters normalize provider signals into one `AgentEvent` model.
- Agent events reach the single-threaded app through a channel.
- Local IPC uses loopback transport, a per-user token, and a versioned protocol.
- Daily challenges use the UTC epoch day and a versioned identity so all users
  receive the same puzzle.
- Daily score metrics normalize to a higher-is-better value for one comparison path.

MVP keeps its blocking terminal loop. Tokio is restricted to online commands
and the background API worker; stable game simulation remains synchronous.
