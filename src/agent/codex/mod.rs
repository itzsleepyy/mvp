//! The Codex adapter.
//!
//! Codex-specific lifecycle knowledge lives only in this module: which
//! official hook events map to which [`AgentEvent`], and how to merge the
//! hook commands into the Codex hooks file. Everything upstream of the
//! IPC boundary is agent-agnostic.
//!
//! Chosen hook mapping (documented from the official hooks reference):
//!
//! | Codex hook event     | Matcher | AgentEvent  |
//! |----------------------|---------|-------------|
//! | `SessionStart`       | (all)   | `Started`   |
//! | `UserPromptSubmit`   | (all)   | `Working`   |
//! | `PostToolUse`        | (all)   | `Working`   |
//! | `PermissionRequest`  | (all)   | `NeedsInput`|
//! | `Stop`               | (all)   | `Completed` |
//! | `SessionEnd`         | (all)   | `Stopped`   |
//!
//! Every hook runs `waitstate hook codex <event>` — a shell command that
//! prints `{}` (valid, decision-free JSON) and exits 0. Codex therefore
//! treats every hook as observational: `PermissionRequest` never decides
//! (the normal approval flow continues) and `Stop` never asks to continue
//! a turn. Hooks are `async` so they can never block Codex.
//!
//! Known limitations:
//! - Codex requires non-managed hooks to be reviewed and trusted via
//!   `/hooks` before they run the first time; `waitstate codex status`
//!   reminds about this.
//! - `SessionEnd` only fires when the conversation closes, is archived, or
//!   has been idle for 30 minutes — not at the end of each turn. `Stop` is
//!   the primary completion signal.
//! - Multiple Codex sessions are treated as one logical agent stream.
//!
//! Plugin future: Phase 4/5 can publish the same hook table as an official
//! Codex plugin by bundling `hooks/hooks.json` (or a `.codex-plugin/plugin.json`
//! manifest `hooks` entry) — the lifecycle mapping in [`HOOKS`] is reused
//! unchanged.

mod config;

use std::path::Path;

use crate::agent::event::AgentEvent;
use crate::agent::integrations::ProviderStatus;
use crate::ipc;

pub use config::{CodexConfig, config_path};

/// One Codex hook entry used by the installer.
pub struct HookEntry {
    /// Codex hook event name.
    pub event: &'static str,
    /// Hook matcher; `None` means "all" (matcher key omitted).
    pub matcher: Option<&'static str>,
    /// The agent event the hook command sends.
    pub agent_event: AgentEvent,
}

/// The complete hook set. The installer and uninstaller walk this table, so
/// ownership tracking stays in one place.
pub const HOOKS: &[HookEntry] = &[
    HookEntry {
        event: "SessionStart",
        matcher: None,
        agent_event: AgentEvent::Started,
    },
    HookEntry {
        event: "UserPromptSubmit",
        matcher: None,
        agent_event: AgentEvent::Working,
    },
    HookEntry {
        event: "PostToolUse",
        matcher: None,
        agent_event: AgentEvent::Working,
    },
    HookEntry {
        event: "PermissionRequest",
        matcher: None,
        agent_event: AgentEvent::NeedsInput,
    },
    HookEntry {
        event: "Stop",
        matcher: None,
        agent_event: AgentEvent::Completed,
    },
    HookEntry {
        event: "SessionEnd",
        matcher: None,
        agent_event: AgentEvent::Stopped,
    },
];

fn current_binary() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot resolve the waitstate binary path: {e}"))
}

/// Installs the WaitState hooks into the user's Codex hooks file.
/// Idempotent; never modifies the file if it cannot be parsed.
pub fn install() -> Result<(), String> {
    install_with_binary(&current_binary()?)
}

fn install_with_binary(binary: &Path) -> Result<(), String> {
    let path = config_path();
    let mut config = CodexConfig::load(&path)?;
    let changes = config.install_hooks(binary);

    if changes == 0 {
        println!("WaitState hooks are already installed and up to date.");
        println!("Config: {}", path.display());
        println!("Remember: review and trust the hooks in Codex with /hooks.");
        return Ok(());
    }
    config.save()?;
    println!(
        "WaitState hooks installed ({changes} change{})",
        plural(changes)
    );
    println!("Config: {}", path.display());
    println!("A backup of the previous settings was saved next to it.");
    println!("Remember: review and trust the hooks in Codex with /hooks.");
    Ok(())
}

/// Removes only WaitState-owned hooks. Idempotent.
pub fn uninstall() -> Result<(), String> {
    let path = config_path();
    let mut config = CodexConfig::load(&path)?;
    let removed = config.uninstall_hooks();
    if removed == 0 {
        println!("No WaitState hooks found — nothing to remove.");
        return Ok(());
    }
    config.save()?;
    println!("Removed {removed} WaitState hook{}.", plural(removed));
    println!("Config: {}", path.display());
    println!("A backup of the previous settings was saved next to it.");
    Ok(())
}

/// Prints the integration status (see the README for the output shape).
pub fn status() {
    println!("Codex integration");
    println!();
    print_provider_status(&status_state());
}

/// Structured status used by the `integrations` overview.
pub fn status_state() -> ProviderStatus {
    match CodexConfig::load(&config_path()) {
        Ok(config) => {
            let (installed, total) = config.hook_status();
            match (installed, total) {
                (0, _) => ProviderStatus::NotInstalled,
                (i, t) if i == t => ProviderStatus::Current,
                (i, t) => ProviderStatus::Outdated(format!("{i}/{t} hooks up to date")),
            }
        }
        Err(err) => ProviderStatus::Broken(err),
    }
}

/// True when any WaitState pieces exist but are missing/stale.
pub fn needs_repair() -> bool {
    matches!(status_state(), ProviderStatus::Outdated(_))
}

/// True when the Codex CLI or its config directory is present.
pub fn detected() -> bool {
    crate::agent::integrations::command_on_path("codex") || config_dir_present()
}

fn config_dir_present() -> bool {
    config_path().parent().is_some_and(|dir| dir.exists())
}

fn print_provider_status(hooks: &ProviderStatus) {
    let hooks_line = match hooks {
        ProviderStatus::Current => "installed".to_string(),
        ProviderStatus::NotInstalled => "not installed".to_string(),
        ProviderStatus::Outdated(detail) => format!("outdated ({detail})"),
        ProviderStatus::Broken(err) => format!("unreadable ({err})"),
    };
    println!("Hooks:      {hooks_line}");
    if matches!(hooks, ProviderStatus::Current) {
        println!("Trust:      review with /hooks in Codex");
    }

    let waitstate_running = ipc::client::running();
    println!(
        "WaitState:  {}",
        if waitstate_running {
            "running"
        } else {
            "not running"
        }
    );
    if waitstate_running {
        println!("IPC:        connected");
    }

    println!();
    let overall = match (hooks, waitstate_running) {
        (ProviderStatus::Current, true) => "ready",
        (ProviderStatus::Current, false) => "hooks configured",
        (ProviderStatus::Outdated(_), _) => "outdated",
        (ProviderStatus::Broken(_), _) => "broken",
        (ProviderStatus::NotInstalled, _) => "not integrated",
    };
    println!("Status: {overall}");
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_table_covers_all_agent_events() {
        use AgentEvent::*;
        let mut events: Vec<AgentEvent> = HOOKS.iter().map(|h| h.agent_event).collect();
        events.sort_by_key(|e| *e as u8);
        events.dedup();
        assert_eq!(
            events,
            vec![Started, Working, NeedsInput, Completed, Stopped]
        );
    }

    #[test]
    fn hook_events_are_the_documented_codex_events() {
        let names: Vec<&str> = HOOKS.iter().map(|h| h.event).collect();
        assert_eq!(
            names,
            vec![
                "SessionStart",
                "UserPromptSubmit",
                "PostToolUse",
                "PermissionRequest",
                "Stop",
                "SessionEnd"
            ]
        );
    }

    #[test]
    fn stop_is_the_primary_completion_signal() {
        let stop = HOOKS.iter().find(|h| h.event == "Stop").unwrap();
        assert_eq!(stop.agent_event, AgentEvent::Completed);
        let end = HOOKS.iter().find(|h| h.event == "SessionEnd").unwrap();
        assert_eq!(end.agent_event, AgentEvent::Stopped);
    }
}
