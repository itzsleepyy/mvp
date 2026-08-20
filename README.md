# MVP — Most Valued Programmer

> A competitive terminal arcade for the time between prompts. Play quick games while your coding agent works, chase the **MVP of the day**, and jump straight back in when it needs you.

**Status: early development.** MVP ships three playable games — *Stack Overflow*, *The Daily PR* and *The Daily Fix* — with local per-game scoring, a daily **MVP of the day** board, and local coding-agent integrations for Claude Code, Codex, Gemini CLI and OpenCode. Everything else on the roadmap is still to come.

## What is MVP?

Coding agents (Claude Code, Codex, Gemini CLI, OpenCode) routinely work autonomously for seconds or minutes at a time. MVP turns that idle time into a lightweight terminal arcade:

```text
start agent → agent works → play MVP → agent needs you → game pauses → you get back to work
```

**Most Valued Programmer** is the prize: your best score on any game that day is the day's *MVP of the day*, displayed on the menu for everyone sharing the machine to see. The scoring board resets every day, so there's always a fresh crown to take — whoever is programming most deserves the title.

Games are designed for very short bursts — simple controls, instant restart, no tutorials.

## Current project status

- [x] Terminal application foundation
- [x] Three playable games (Stack Overflow, The Daily PR, The Daily Fix)
- [x] Per-game local high-score persistence
- [x] Daily MVP-of-the-day board with player names
- [x] Claude Code integration
- [x] Codex integration
- [x] Gemini CLI integration
- [x] OpenCode integration
- [ ] GitHub authentication
- [ ] Global leaderboards

## MVP of the day

On the first launch MVP asks who you are; the name is saved (change it any time with `N` on the main menu). Every finished run competes for today's board:

- The **best single score** of the day — across every game — crowns the MVP of the day.
- Daily challenges report their result in the game's own units: `MVP OF THE DAY  Alex  —  3 guesses  (THE DAILY PR)` or `MVP OF THE DAY  Sam  —  fixed in 0:42  (THE DAILY FIX)`.
- The current holder appears on the main menu: `MVP OF THE DAY  Alex  —  4,820  (STACK OVERFLOW)`.
- A run that dethrones the holder flashes **MVP OF THE DAY!** on the game-over panel.
- At midnight the board resets: a new day, a new title to claim.

The board lives locally (per machine). Player names and scores never leave your computer until global leaderboards ship.

## Games

Press `ENTER` on the main menu to choose a game. Selection is remembered between runs, and `ESC` steps back at any time.

### Stack Overflow

The arcade stacker, themed as a call stack: a block as wide as the tower slides side to side, and `SPACE` drops it. Overhang is trimmed from both block and tower — a perfect drop is a clean frame (+250), a full miss is a **stack overflow**. Slide speed climbs with the stack height and your perfect streak; your score is the stack you build before the overflow.

### The Daily PR

Wordle for developers — one shared 5-letter word per day, identical for everyone, six guesses. Correct letters `G`, wrong position `?`, absent `·`. **Fewest guesses wins**; a faster solve breaks the tie. The menu shows your best of the day in guesses (`BEST 3 GUESSES`); the board locks once the word is found (or revealed after six misses).

### The Daily Fix

One shared buggy snippet per day, identical for everyone. Spot the broken line (`BUG ▸`), type the corrected line, `ENTER` submits. **Fastest correct fix wins**. Wrong fixes cost 5s each; after two misses a hint appears (costing 15s). Solving shows the one-line lesson behind the bug.

## Controls

| Key | Action |
| --- | --- |
| Key | Action |
| --- | --- |
| `ENTER` | Open the game menu / start the selected game / resume an agent-paused run / confirm your name / submit a guess or fix |
| `↑` / `↓` | Select a game (game menu) |
| `SPACE` / `↑` | Drop the block (Stack Overflow) |
| `A–Z`, `0–9`, symbols | Type a guess (The Daily PR) or a fix (The Daily Fix) |
| `⌫` | Delete the last typed character |
| `N` | Change your name (main menu) |
| `P` | Pause / resume (manual) |
| `R` | Restart after game over |
| `ESC` | Back to menu / cancel a name change |
| `Q` / `CTRL+C` | Quit |

## Installation

Requires a recent stable Rust toolchain.

```bash
git clone https://github.com/itzsleepyy/waitstate
cd waitstate
cargo install --path package
mvp
```

Pre-rebrand `WaitState` installs upgrade in place: high scores are migrated into the new `MVP` config directory on first run, and hooks installed by the old binary are still recognised and upgraded.

## Supported coding agents

| Agent | Integration | Auto pause | Auto resume |
| --- | --- | --- | --- |
| Claude Code | Hooks | Yes | Yes |
| Codex | Hooks | Yes | Yes |
| Gemini CLI | Hooks | Yes | Yes |
| OpenCode | Plugin events | Yes | Yes |

One lifecycle platform, four adapters. Every agent maps its native lifecycle into the same generic event model, so the game behaves identically no matter which agent you use — including several agents working at once.

```text
                 ┌─ Claude hooks
                 ├─ Codex hooks
Agent adapters ◄─┼─ Gemini hooks
                 └─ OpenCode plugin
                       │
                       ▼
                  AgentEvent
                       │
                       ▼
                      IPC
                       │
                       ▼
                    MVP App
```

### Setup

```bash
mvp integrations          # overview of every integration
mvp claude install        # merge hooks into Claude settings (idempotent, backs up first)
mvp codex install         # merge hooks into ~/.codex/hooks.json
mvp gemini install        # merge hooks into ~/.gemini/settings.json
mvp opencode install      # install the MVP plugin into OpenCode
```

Or all at once:

```bash
mvp integrations install         # install for detected agents only
mvp integrations install --all   # install every supported agent
mvp integrations repair          # fix missing/outdated pieces
```

Each provider also has `status` and `uninstall` subcommands. Every installer:

- discovers the right configuration location (honouring `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `XDG_CONFIG_HOME`)
- merges without touching unrelated settings, hooks, plugins or files
- refuses to modify unparseable or foreign files
- creates a timestamped backup before writing
- is idempotent and safe to run any number of times
- removes only MVP-owned pieces on uninstall

Then just run `mvp` and use your agent as usual. `mvp --agent codex` pins the status indicator to one agent; `mvp --agent auto` follows whichever agent most recently emitted an event. With no flag, the indicator appears once the first event arrives.

A small status indicator (`Codex • Working`, or `2 agents • Working`) appears in the menu, HUD, pause overlays and game-over panel. A manual pause (`P`) is never overridden by agent events, and a run paused because an agent *finished* is only resumed by you (`ENTER`). Agent-paused runs freeze score, player position, obstacles and difficulty exactly where they were.

### Attention model (multiple agents)

Multiple agents can work at once. MVP keeps per-agent state and derives one aggregate status:

```text
if ANY active agent NeedsInput → pause ("CODEX NEEDS YOU", "2 AGENTS NEED YOU", …)
else if ANY active agent Working → play / resume
else if ANY agent Completed    → completion pause
else if ANY agent Stopped      → session-ended pause
```

A completion pause is sticky for the completing agent: that agent working again never silently resumes the run — only a *different* agent working, a fresh session, or your `ENTER` does.

### How it works

Each adapter's official lifecycle mechanism runs an MVP bridge command that never blocks the agent (exit 0, bounded timeout, no output — or exactly `{}` where the hook protocol requires JSON). Events travel over a local-only loopback IPC socket (per-user, token-protected, protocol-versioned, agent-identified) to the running game, where the generic event model drives the state machine:

| MVP event | Claude Code | Codex | Gemini CLI | OpenCode |
| --- | --- | --- | --- | --- |
| `Started` | `SessionStart` | `SessionStart` | `SessionStart` | `session.created` |
| `Working` | `UserPromptSubmit`, `PostToolUse` | `UserPromptSubmit`, `PostToolUse` | `BeforeAgent`, `BeforeTool`, `AfterTool` | `session.status` busy, `tool.execute.before/after`, `permission.replied` |
| `NeedsInput` | `Notification` (permission/input dialogs) | `PermissionRequest` | `Notification` (tool permissions) | `permission.ask` |
| `Completed` | `Stop` | `Stop` | `AfterAgent` | `session.status` idle |
| `Stopped` | `SessionEnd` | `SessionEnd` | `SessionEnd` | `session.deleted` |

Agent-specific knowledge lives only in each adapter (`package/src/agent/claude`, `codex`, `gemini`, `opencode`); the app, IPC and UI are agent-agnostic.

### Known limitations

Claude Code:

- `permission_prompt` fires ~6 seconds after the prompt appears, so the pause is not instant for permission requests.
- `Stop` is per-turn and does not fire on user interrupts or API errors.

Codex:

- Non-managed hooks must be reviewed and trusted via `/hooks` in Codex before they run the first time (`mvp codex status` reminds you). Untrusted hooks run sandboxed, where the MVP binary may be unreachable.
- Hooks are synchronous (verified: `async` hooks never run in `codex exec` sessions) — the bridge is one bounded local TCP connect, so the agent loop is not held up.
- `SessionEnd` fires when the conversation closes, is archived, or idles for 30 minutes — `Stop` is the primary completion signal.
- A failed *other* hook in the same group (e.g. a stale third-party hook) shows a hook-failure warning in Codex but does not affect MVP's hooks.

Gemini CLI:

- Hooks are always synchronous; the `timeout` (2000 ms) bounds a hung bridge.
- `Notification` currently only fires for tool-permission alerts.

OpenCode:

- `session.error` is deliberately unmapped (an errored session is not a clean completion).
- The plugin spawns the MVP binary per event; if MVP is not running, the spawn fails silently.

All agents:

- Multiple sessions of the same agent are treated as one logical agent stream.
- Plugin/extension marketplace distribution (official Codex plugin, Gemini extension, npm OpenCode plugin) is planned but not implemented yet.

### Privacy

**MVP knows when the agent is working, not what you are working on.** It receives lifecycle events only — it never reads your prompts, source code, tool output, repository contents or conversation history. The hook commands and IPC protocol carry nothing but an agent kind and an event name.

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
package/src/
├── main.rs      entry point: CLI dispatch and event loop
├── cli.rs       subcommands (play, agent-event, hook, per-agent install/
│                status/uninstall, integrations overview/install/repair)
├── tui.rs       terminal init/restore (raw mode, alternate screen, panic hook)
├── event.rs     crossterm events → application inputs
├── app.rs       application state machine (Menu / GameMenu / NamePrompt /
│                Playing / PausedManual / PausedAgent / GameOver), game
│                selection, the daily-MVP recording, per-agent state and
│                the aggregate attention model
├── config.rs    platform-aware per-game high scores, the daily MVP board
│                and the player name (legacy WaitState dir is migrated)
├── ui.rs        Ratatui rendering (menu, HUD, overlays, resize handling)
├── agent/       generic agent lifecycle events, status and adapters
│   ├── event.rs       AgentEvent (started/working/needs-input/completed/stopped)
│   ├── status.rs      AgentKind/AgentStatus/AgentState/AgentDisplay
│   ├── hooks_json.rs  shared non-destructive JSON hook merging (backups,
│   │                  idempotency, ownership checks)
│   ├── integrations.rs  registry, detection, overview, install-all, repair
│   ├── claude/        Claude-specific hook table + settings merge
│   ├── codex/         Codex hook table + hooks.json merge (sync `hook` bridge)
│   ├── gemini/        Gemini hook table + settings merge (JSON-stdout bridge)
│   └── opencode/      OpenCode plugin generation, versioning, install
├── ipc/         local-only agent event transport
│   ├── protocol.rs  versioned, token-checked, agent-identified messages
│   ├── server.rs    loopback listener thread → channel → main loop
│   └── client.rs    one-shot event sender (used by hook bridges)
└── game/        pure game logic, no terminal types
    ├── mod.rs       GameKind registry, the ActiveGame wrapper over live
    │                games, and the shared GameInput vocabulary
    ├── state.rs     run state
    ├── scoring.rs   score formatting helpers
    ├── stack_overflow.rs  the arcade stacker: tower layers, slide physics,
    │                     perfect streaks, overflow
    ├── daily_pr.rs  the daily Wordle: word selection, guess validation,
    │               marks, guess-count scoring
    ├── daily_fix.rs the daily bug: curated bug bank, fix matching,
    │               penalties and the auto-hint
    └── words.rs     the PR word lists (curated answers + guess dictionary)
```

Key decisions:

- **Simulation is delta-time based and clamped** (`MAX_FRAME_TIME`), so gameplay never depends on frame rate.
- **The app never talks to a specific game.** `ActiveGame` exposes a small shared surface (`handle_input`, `update`, `is_game_over`, `score`, `set_viewport`) and the renderer branches per `GameKind` — adding a game means adding a module plus two render/match arms, not reworking the state machine.
- **Turn-based games fit the same loop**: `update(dt)` is a no-op and input drives everything, so agent pauses freeze the table exactly like the physics.
- **`App::tick(dt)` advances the game only while playing** on an adequate terminal — pause and terminal-size handling live in exactly one place.
- **All state transitions are programmatic methods** (`pause()`, `start_game()`, `handle_agent_event(...)`, …). Keyboard input and agent lifecycle events drive the same state machine without knowing about each other.
- **Agent events arrive on a channel and are applied on the main loop** — application state stays single-threaded, no locks.
- **The engine is headless**: game logic never touches terminal types, so every game is unit-tested and simulated without a screen — the door stays open for tests, replays, and server-side simulation.
- **Manual pause and agent pause are distinct states**: agent events never override a manual pause, and auto-resume only applies to runs the agent paused (never to runs paused because an agent finished).
- **One state machine for all agents**: adapters normalize into `AgentEvent`; per-agent statuses aggregate into one attention decision (`NeedsInput > Working > Completed > Stopped > Idle`).
- **Daily challenges are seeded from the day**: the PR word and the bug are picked as `pool[today_ordinal % len]`, so everyone plays the same puzzle and replays keep it.
- **Daily scores normalize into the shared board**: fewest guesses / fastest fix are encoded as higher-is-better (`(7 − guesses) × 1e6 − seconds`, `1e9 − ms`), so one comparison serves every game and a zero (did-not-finish) is never a record.
- The event loop is a simple 60 FPS poll loop (crossterm) plus one plain-thread IPC listener — `tokio` was intentionally not introduced: a blocking terminal game loop gains nothing from an async runtime, and short-lived hook clients need nothing more than a channel. It can be revisited for network integrations.
- High-score storage, the daily MVP board and the player name live in the platform config directory (e.g. `~/.config/mvp/`, `~/Library/Application Support/MVP/`, `%APPDATA%\MVP\`) and degrade gracefully on any filesystem problem. The pre-rebrand `WaitState` directory is migrated from on first run.

## Roadmap

```text
[x] Terminal application foundation
[x] Three playable games (Stack Overflow, The Daily PR, The Daily Fix)
[x] Game selection menu
[x] Local per-game scoring
[x] Daily MVP-of-the-day board
[x] Claude Code integration
[x] Codex integration
[x] Gemini CLI integration
[x] OpenCode integration
[ ] GitHub authentication
[ ] Global leaderboards
[ ] Friend leaderboards
[ ] Additional games
[ ] Multiplayer
```

## Contributing

Pull requests are welcome. Keep gameplay logic independent of terminal rendering, keep the codebase `cargo fmt`/`cargo clippy` clean, and add tests for new game logic.

## License

MIT — see [LICENSE](LICENSE).
