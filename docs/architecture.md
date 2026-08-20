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
website/                reserved for the future website application
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

## Design Constraints

- Game simulation is delta-time based and clamps long frames.
- `ActiveGame` gives the app one shared interface across real-time and turn-based games.
- Only the playing state advances game time; pauses and undersized terminals freeze runs.
- Agent adapters normalize provider signals into one `AgentEvent` model.
- Agent events reach the single-threaded app through a channel.
- Local IPC uses loopback transport, a per-user token, and a versioned protocol.
- Daily challenges use the UTC epoch day so all users receive the same puzzle.
- Daily score metrics normalize to a higher-is-better value for one comparison path.

MVP currently uses a blocking terminal loop and a small IPC listener thread. No async runtime is needed for the local application.
