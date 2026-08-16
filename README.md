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
- [x] Claude Code integration
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
| `ENTER` | Start / play again / resume an agent-paused run |
| `SPACE` / `↑` | Jump |
| `P` | Pause / resume (manual) |
| `R` | Restart after game over |
| `ESC` | Back to menu |
| `Q` / `CTRL+C` | Quit |

## Claude Code integration

WaitState watches Claude Code's lifecycle and gets out of the way the moment Claude needs you:

```text
Claude works        → WaitState is playable
Claude needs input  → WaitState pauses automatically, run preserved
Claude works again  → WaitState resumes the same run
Claude finishes     → run pauses with a clear "CLAUDE FINISHED" panel
```

### Setup

```bash
waitstate claude install     # merge WaitState hooks into ~/.claude/settings.json (idempotent, backs up first)
waitstate claude status      # show integration status
waitstate claude uninstall   # remove only WaitState-owned hooks
```

Then just run `waitstate` (optionally `waitstate --agent claude` to show the Claude status indicator from the start) and use Claude Code as usual.

The installer merges into your existing `~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR/settings.json`) without touching unrelated settings, hooks, permissions or plugins. It refuses to modify unparseable files, creates a timestamped backup before writing, and is safe to run any number of times. Uninstalling removes only the hooks WaitState added.

A small status indicator (`Claude Code • Working`) appears in the menu, HUD, pause overlays and game-over panel. A manual pause (`P`) is never overridden by Claude events, and a run paused because Claude *finished* is only resumed by you (`ENTER`). Agent-paused runs freeze score, player position, obstacles and difficulty exactly where they were.

### How it works

Claude Code's official [hooks](https://docs.anthropic.com/en/docs/claude-code/hooks) run `waitstate agent-event <event>` — a fire-and-forget command that never blocks Claude (async, short timeout, exit 0, no output). Events travel over a local-only loopback IPC socket (per-user, token-protected, protocol-versioned) to the running game, where a generic agent-agnostic event model drives the state machine:

| Claude hook | WaitState event |
| --- | --- |
| `SessionStart` | `Started` |
| `UserPromptSubmit`, `PostToolUse` | `Working` |
| `Notification` (`permission_prompt`, `agent_needs_input`, `elicitation_*`) | `NeedsInput` |
| `Stop` | `Completed` |
| `SessionEnd` | `Stopped` |

Claude-specific knowledge lives only in the adapter; future Codex/Gemini CLI/OpenCode integrations map into the same generic events.

### Known limitations

- Claude's `permission_prompt` notification fires ~6 seconds after the prompt appears, so the pause is not instant for permission requests.
- `Stop` is per-turn and does not fire on user interrupts or API errors.
- Multiple Claude sessions are treated as one logical agent stream.
- `agent_needs_input`/`agent_completed` notifications require Claude Code v2.1.198+ and only fire for background sessions while the agent view is open.
- Packaging as an official Claude Code plugin (marketplace distribution) is planned but not implemented yet.

### Privacy

**WaitState knows when the agent is working, not what you are working on.** It receives lifecycle events only — it never reads your prompts, source code, tool output, repository contents or conversation history. The hook commands and IPC protocol carry nothing but an event name.

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
├── main.rs      entry point: CLI dispatch and event loop
├── cli.rs       subcommands (play, agent-event, claude install/status/uninstall)
├── tui.rs       terminal init/restore (raw mode, alternate screen, panic hook)
├── event.rs     crossterm events → application inputs
├── app.rs       application state machine (Menu / Playing / PausedManual /
│                PausedAgent / GameOver) and agent event handling
├── config.rs    platform-aware local high-score storage
├── ui.rs        Ratatui rendering (menu, HUD, overlays, resize handling)
├── agent/       generic agent lifecycle events and status
│   ├── event.rs     AgentEvent (started/working/needs-input/completed/stopped)
│   ├── status.rs    AgentKind/AgentStatus/AgentState
│   └── claude/      Claude-specific adapter: hook table + settings merge
├── ipc/         local-only agent event transport
│   ├── protocol.rs  versioned, token-checked message schema
│   ├── server.rs    loopback listener thread → channel → main loop
│   └── client.rs    one-shot event sender (used by Claude hooks)
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
- **All state transitions are programmatic methods** (`pause()`, `start_game()`, `handle_agent_event(...)`, …). Keyboard input and agent lifecycle events drive the same state machine without knowing about each other.
- **Agent events arrive on a channel and are applied on the main loop** — application state stays single-threaded, no locks.
- **The renderer consumes a plain-data `GameRenderState` snapshot** — the engine can run headlessly, which keeps the door open for tests, replays, and server-side simulation.
- **Manual pause and agent pause are distinct states**: agent events never override a manual pause, and auto-resume only applies to runs the agent paused (never to runs paused because Claude finished).
- **Obstacle spacing is guaranteed fair**: gaps are rolled as `speed × reaction time + jitter`, so every pattern is physically clearable as speed increases.
- The event loop is a simple 60 FPS poll loop (crossterm) plus one plain-thread IPC listener — `tokio` was intentionally not introduced: a blocking terminal game loop gains nothing from an async runtime, and one short-lived hook client at a time needs nothing more than a channel. It can be revisited for network/agent integrations.
- High-score storage lives in the platform config directory (e.g. `~/.config/waitstate/`, `~/Library/Application Support/WaitState/`, `%APPDATA%\WaitState\`) and degrades gracefully on any filesystem problem.

## Roadmap

```text
[x] Terminal application foundation
[x] First playable game (Stack Jump)
[x] Local scoring
[x] Claude Code integration
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
