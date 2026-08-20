# MVP — Most Valued Programmer

A competitive terminal arcade for the time between coding-agent prompts.

## What is MVP?

MVP turns the short waits while Claude Code, Codex, Gemini CLI, or OpenCode works into quick terminal games. Agent lifecycle integrations pause the game when an agent needs attention and let you return to work without losing a run.

Local play works without an account. GitHub sign-in adds global score submission, ranks, and the Daily MVP competition; game traffic never includes source code, prompts, repositories, terminal contents, or coding-agent output.

## Games

- **Stack Overflow** - Drop moving call-stack frames, trim overhang, and build as high as possible before a full miss.
- **The Daily PR** - Find the shared five-letter word in six guesses. Fewer guesses wins; speed breaks ties.
- **The Daily Fix** - Correct the broken line in a shared daily snippet. Fast fixes win, while wrong submissions add penalties.

## Coding Agent Integrations

MVP supports Claude Code, Codex, Gemini CLI, and OpenCode through local hooks or plugins.

```bash
mvp integrations
mvp integrations install
mvp integrations repair

mvp claude install
mvp codex install
mvp gemini install
mvp opencode install
```

Each provider also supports `status` and `uninstall`. Installers preserve unrelated configuration and only remove MVP-owned entries.

See [Agent Integrations](docs/agent-integrations.md) for lifecycle mappings, behavior, and limitations.

## Online Competition

```bash
mvp login
mvp whoami
mvp profile
mvp leaderboard
mvp leaderboard weekly
mvp leaderboard stack-overflow
mvp logout
```

GitHub login uses Device Flow and stores the MVP session in the operating system credential store. Failed authenticated run submissions are queued locally and retried without interrupting play. Press `L` on the main menu to open Daily MVP; local games remain available when the service is offline.

## Installation

Requires a recent stable Rust toolchain.

```bash
git clone https://github.com/itzsleepyy/waitstate mvp
cd mvp
cargo install --path package
mvp
```

Legacy compatibility: pre-rebrand `WaitState` score files are copied into the MVP config directory on first discovery without modifying the originals or replacing newer MVP data. Previously installed hooks and owned OpenCode plugins are still recognized for safe upgrade or removal.

## Controls

| Key | Action |
| --- | --- |
| `ENTER` | Select, start, resume, confirm, or submit |
| `Up` / `Down` | Select a game |
| `SPACE` / `Up` | Drop a Stack Overflow frame |
| Printable characters | Type a Daily PR guess or Daily Fix answer |
| `Backspace` | Delete the last character |
| `N` | Change player name from the main menu |
| `L` | View the online Daily MVP leaderboard |
| `P` | Pause or resume manually |
| `R` | Restart after game over |
| `ESC` | Go back or cancel a rename; the first-run name prompt is required |
| `Q` / `Ctrl+C` | Quit |

## Development

From the repository root:

```bash
cargo run
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

The root is a Cargo workspace and `package/src/` is the authoritative Rust implementation. See [Development](docs/development.md) and [Architecture](docs/architecture.md) for technical details.

## License

MIT - see [LICENSE](LICENSE).
