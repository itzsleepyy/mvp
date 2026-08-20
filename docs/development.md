# Development

## Requirements

- Recent stable Rust toolchain
- Terminal at least 60 columns by 20 rows for gameplay

## Commands

Run all commands from the repository root:

```bash
cargo run
cargo test
cargo fmt --check
cargo clippy -- -D warnings
cargo install --path package
```

The root manifest is a workspace. The active package and all Rust source live under `package/`; no root `src/` implementation is expected.

## Change Guidelines

- Keep game logic independent of terminal rendering.
- Preserve unrelated user settings when changing integrations.
- Keep persistence failures non-fatal to gameplay.
- Add tests for state transitions, game rules, migrations, and integration ownership.
- Run formatting, strict Clippy, and the full test suite before submitting changes.

## Local Data

New installations use the platform config directory selected for the application name `MVP`. It contains:

- `highscore.json` for player name, per-game scores, and Daily PR progress
- `mvp_day.json` for the current daily MVP record

Compatibility migration copies files from the pre-rebrand application directory only when the corresponding MVP file does not exist. The source remains untouched, repeated migration is a no-op, and newer MVP data is never overwritten.
