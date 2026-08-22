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
//! Every hook runs `mvp agent-event <event>` in exec form
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
use crate::agent::integrations::ProviderStatus;
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

/// Installs the MVP hooks into the user's Claude Code settings.
/// Idempotent; never modifies the file if it cannot be parsed.
pub fn install() -> Result<(), String> {
    let path = config_path();
    let mut config = ClaudeConfig::load(&path)?;
    let binary =
        std::env::current_exe().map_err(|e| format!("cannot resolve the mvp binary path: {e}"))?;
    let changes = config.install_hooks(&binary);

    if changes == 0 {
        println!("MVP hooks are already installed and up to date.");
        println!("Config: {}", path.display());
        return Ok(());
    }
    config.save()?;
    println!("MVP hooks installed ({changes} change{})", plural(changes));
    println!("Config: {}", path.display());
    println!("A backup of the previous settings was saved next to it.");
    Ok(())
}

/// Removes only MVP-owned hooks. Idempotent.
pub fn uninstall() -> Result<(), String> {
    let path = config_path();
    let mut config = ClaudeConfig::load(&path)?;
    let removed = config.uninstall_hooks();
    if removed == 0 {
        println!("No MVP hooks found — nothing to remove.");
        return Ok(());
    }
    config.save()?;
    println!("Removed {removed} MVP hook{}.", plural(removed));
    println!("Config: {}", path.display());
    println!("A backup of the previous settings was saved next to it.");
    Ok(())
}

/// Prints the integration status.
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

    let mvp_running = ipc::client::running();
    println!(
        "MVP:     {}",
        if mvp_running {
            "running"
        } else {
            "not running"
        }
    );
    if mvp_running {
        println!("IPC:        connected");
    }

    println!();
    let overall = match (
        hooks_line.starts_with("installed"),
        hooks_line.starts_with("partial"),
        mvp_running,
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

/// Structured status used by the `integrations` overview.
pub fn status_state() -> ProviderStatus {
    match ClaudeConfig::load(&config_path()) {
        Ok(config) => {
            let status = config.hook_status();
            match status.completeness() {
                (0, _) => ProviderStatus::NotInstalled,
                (installed, total) if installed == total => ProviderStatus::Current,
                (installed, total) => {
                    ProviderStatus::Outdated(format!("{installed}/{total} hooks up to date"))
                }
            }
        }
        Err(err) => ProviderStatus::Broken(err),
    }
}

/// True when any MVP pieces exist but are missing/stale.
pub fn needs_repair() -> bool {
    matches!(status_state(), ProviderStatus::Outdated(_))
}

/// True when the Claude CLI or its config directory is present.
pub fn detected() -> bool {
    crate::agent::integrations::command_on_path("claude") || config_dir_present()
}

fn config_dir_present() -> bool {
    config_path().parent().is_some_and(|dir| dir.exists())
}

/// True when the executable at `path` is one MVP owns hooks with
/// (including the pre-rebrand `waitstate` names).
pub fn binary_name_matches(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    // `Path::file_name` does not split on `\` when running on unix; handle
    // Windows-style paths (tests, cross-machine configs) manually.
    let name = name.rsplit('\\').next().unwrap_or(name);
    crate::agent::owned_binary_name(name)
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
        assert!(binary_name_matches(Path::new("/usr/local/bin/mvp")));
        assert!(binary_name_matches(Path::new("C:\\tools\\mvp.exe")));
        assert!(
            binary_name_matches(Path::new("/usr/local/bin/waitstate")),
            "pre-rebrand hooks must still be owned"
        );
        assert!(!binary_name_matches(Path::new("/usr/bin/claude")));
        assert!(!binary_name_matches(Path::new("/opt/mvp2")));
    }
}
