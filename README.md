# WaitState

> A competitive terminal arcade for the time between prompts. Play quick games while your coding agent works, compete on leaderboards, and jump straight back in when it needs you.

**Status: early development.** WaitState currently ships its first playable game — *Stack Jump* — with local scoring. Everything else on the roadmap is still to come.

## What is WaitState?

Coding agents (Claude Code, Codex, Gemini CLI, OpenCode) routinely work autonomously for seconds or minutes at a time. WaitState turns that idle time into a lightweight terminal arcade:

```text
start agent → agent works → play WaitState → agent needs you → game pauses → you get back to work
```

Games are designed for very short bursts — one button, instant restart, no tutorials.

## Current project status

- [x] Terminal application foundation
- [x] First playable game (Stack Jump)
- [x] Local high-score persistence
- [ ] Agent integrations (automatic pause/resume)
- [ ] GitHub authentication
- [ ] Global leaderboards

## Installation

Requires a recent stable Rust toolchain.

```bash
git clone https://github.com/itzsleepyy/waitstate
cd waitstate
cargo run --release
```

## Controls

| Key | Action |
| --- | --- |
| `ENTER` | Start / play again |
| `SPACE` / `↑` | Jump |
| `P` | Pause / resume |
| `R` | Restart after game over |
| `ESC` | Back to menu |
| `Q` / `CTRL+C` | Quit |

## Development setup

```bash
cargo run              # run the game
cargo test             # unit tests
cargo fmt --check      # formatting
cargo clippy -- -D warnings   # lint
```

The game requires a terminal of at least 60×20 characters. Smaller terminals show a warning instead of crashing; gameplay resumes once the terminal is resized.

## Architecture

The game engine is deliberately independent of terminal rendering:

```text
Input → Application → Game State → Game Simulation → Rendering
```

```
src/
├── main.rs      entry point and event loop
├── tui.rs       terminal init/restore (raw mode, alternate screen, panic hook)
├── event.rs     crossterm events → application inputs
├── app.rs       application state machine (Menu / Playing / Paused / GameOver)
├── config.rs    platform-aware local high-score storage
├── ui.rs        Ratatui rendering (menu, HUD, overlays, resize handling)
└── game/        pure game logic, no terminal types
    ├── mod.rs       StackJump game and the headless update loop
    ├── state.rs     run state
    ├── player.rs    jumping physics
    ├── obstacle.rs  obstacle kinds
    ├── world.rs     obstacle spawning, speed/difficulty scaling
    ├── collision.rs AABB collision
    └── scoring.rs   survival scoring and formatting
```

Key decisions:

- **Simulation is delta-time based and clamped** (`MAX_FRAME_TIME`), so gameplay never depends on frame rate.
- **`App::tick(dt)` advances the game only while playing** on an adequate terminal — pause and terminal-size handling live in exactly one place.
- **All state transitions are programmatic methods** (`pause()`, `start_game()`, …). Future agent events (`AgentNeedsInput`, …) can drive them without touching keyboard handling.
- **The renderer consumes a plain-data `GameRenderState` snapshot** — the engine can run headlessly, which keeps the door open for tests, replays, and server-side simulation.
- **Obstacle spacing is guaranteed fair**: gaps are rolled as `speed × reaction time + jitter`, so every pattern is physically clearable as speed increases.
- The event loop is a simple 60 FPS poll loop (crossterm) — `tokio` was intentionally not introduced for Phase 1, since a blocking terminal game loop gains nothing from an async runtime. It can be added later for network/agent integrations.
- High-score storage lives in the platform config directory (e.g. `~/.config/waitstate/`, `~/Library/Application Support/WaitState/`, `%APPDATA%\WaitState\`) and degrades gracefully on any filesystem problem.

## Roadmap

```text
[x] Terminal application foundation
[x] First playable game (Stack Jump)
[x] Local scoring
[ ] Claude Code integration
[ ] GitHub authentication
[ ] Global leaderboards
[ ] Friend leaderboards
[ ] Additional games
[ ] Codex integration
[ ] Gemini CLI integration
[ ] OpenCode integration
[ ] Multiplayer
```

## Contributing

Pull requests are welcome. Keep gameplay logic independent of terminal rendering, keep the codebase `cargo fmt`/`cargo clippy` clean, and add tests for new game logic.

## License

MIT — see [LICENSE](LICENSE).
