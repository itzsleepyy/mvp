//! The Claude Code adapter.
//!
//! Claude-specific lifecycle knowledge lives only in this module: which
//! official hook events map to which [`AgentEvent`], and how to merge the
//! hook commands into Claude's settings file. Everything upstream of the
//! IPC boundary is agent-agnostic.
//!
//! Chosen hook mapping (documented from the official hooks reference):
//!
//! | Claude hook event      | Matcher                                  | AgentEvent |
//! |------------------------|------------------------------------------|------------|
//! | `SessionStart`         | (all)                                    | `Started`  |
//! | `UserPromptSubmit`     | (all)                                    | `Working`  |
//! | `PostToolUse`          | (all)                                    | `Working`  |
//! | `Notification`         | `permission_prompt\|agent_needs_input\|elicitation_dialog\|elicitation_url_dialog` | `NeedsInput` |
//! | `Stop`                 | (all)                                    | `Completed`|
//! | `SessionEnd`           | (all)                                    | `Stopped`  |
//!
//! Every hook runs `waitstate agent-event <event>` in exec form
//! (`command` + `args`, no shell) with `async: true` and a short timeout,
//! so it can never block Claude Code. `PostToolUse` is included because a
//! tool call after a permission approval is the only signal that Claude
//! resumed working inside the same turn; duplicates are idempotent
//! downstream.
//!
//! Known limitations:
//! - `permission_prompt` fires ~6 seconds after the prompt appears.
//! - `Stop` fires per turn and not on user interrupts or API errors.
//! - Multiple Claude sessions are treated as one logical agent stream.

mod config;

use std::path::Path;

use crate::agent::event::AgentEvent;
use crate::ipc;

pub use config::{ClaudeConfig, config_path};

/// One Claude hook entry used by the installer.
pub struct HookEntry {
    /// Claude Code hook event name.
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
        event: "Notification",
        matcher: Some(
            "permission_prompt|agent_needs_input|elicitation_dialog|elicitation_url_dialog",
        ),
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

/// Installs the WaitState hooks into the user's Claude Code settings.
/// Idempotent; never modifies the file if it cannot be parsed.
pub fn install() -> Result<(), String> {
    let path = config_path();
    let mut config = ClaudeConfig::load(&path)?;
    let binary = std::env::current_exe()
        .map_err(|e| format!("cannot resolve the waitstate binary path: {e}"))?;
    let changes = config.install_hooks(&binary);

    if changes == 0 {
        println!("WaitState hooks are already installed and up to date.");
        println!("Config: {}", path.display());
        return Ok(());
    }
    config.save()?;
    println!(
        "WaitState hooks installed ({changes} change{})",
        plural(changes)
    );
    println!("Config: {}", path.display());
    println!("A backup of the previous settings was saved next to it.");
    Ok(())
}

/// Removes only WaitState-owned hooks. Idempotent.
pub fn uninstall() -> Result<(), String> {
    let path = config_path();
    let mut config = ClaudeConfig::load(&path)?;
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
    println!("Claude Code integration");
    println!();

    let hooks_line = match ClaudeConfig::load(&config_path()) {
        Ok(config) => {
            let status = config.hook_status();
            match status.completeness() {
                (0, _) => "not installed".to_string(),
                (installed, total) if installed == total => "installed".to_string(),
                (installed, total) => {
                    format!("partial ({installed}/{total} events)")
                }
            }
        }
        Err(err) => format!("unreadable ({err})"),
    };
    println!("Hooks:      {hooks_line}");

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
    let overall = match (
        hooks_line.starts_with("installed"),
        hooks_line.starts_with("partial"),
        waitstate_running,
    ) {
        (true, _, true) => "ready",
        (true, _, false) => "hooks configured",
        (false, true, _) => "partially configured",
        _ => "not integrated",
    };
    println!("Status: {overall}");
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The user-level Claude settings file, honouring `CLAUDE_CONFIG_DIR`.
pub fn binary_name_matches(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    // `Path::file_name` does not split on `\` when running on unix; handle
    // Windows-style paths (tests, cross-machine configs) manually.
    let name = name.rsplit('\\').next().unwrap_or(name);
    matches!(name, "waitstate" | "waitstate.exe")
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
    fn hook_events_are_the_documented_claude_events() {
        let names: Vec<&str> = HOOKS.iter().map(|h| h.event).collect();
        assert_eq!(
            names,
            vec![
                "SessionStart",
                "UserPromptSubmit",
                "PostToolUse",
                "Notification",
                "Stop",
                "SessionEnd"
            ]
        );
    }

    #[test]
    fn binary_name_matches_our_executable() {
        assert!(binary_name_matches(Path::new("/usr/local/bin/waitstate")));
        assert!(binary_name_matches(Path::new("C:\\tools\\waitstate.exe")));
        assert!(!binary_name_matches(Path::new("/usr/bin/claude")));
        assert!(!binary_name_matches(Path::new("/opt/waitstate2")));
    }
}
