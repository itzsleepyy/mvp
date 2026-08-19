//! The Gemini CLI adapter.
//!
//! Gemini-specific lifecycle knowledge lives only in this module: which
//! official hook events map to which [`AgentEvent`], and how to merge the
//! hook commands into Gemini's settings file. Everything upstream of the
//! IPC boundary is agent-agnostic.
//!
//! Chosen hook mapping (documented from the official hooks reference):
//!
//! | Gemini hook event   | Matcher | AgentEvent  |
//! |---------------------|---------|-------------|
//! | `SessionStart`      | (all)   | `Started`   |
//! | `BeforeAgent`       | (all)   | `Working`   |
//! | `BeforeTool`        | (all)   | `Working`   |
//! | `AfterTool`         | (all)   | `Working`   |
//! | `Notification`      | (all)   | `NeedsInput`|
//! | `AfterAgent`        | (all)   | `Completed` |
//! | `SessionEnd`        | (all)   | `Stopped`   |
//!
//! Why `Notification` → `NeedsInput`: Gemini fires it for system alerts
//! whose only current type is `ToolPermission` — exactly when the developer
//! must decide whether a tool may run. Other notifications would also map
//! here (they are advisory noise at worst), and the hook is purely
//! observational anyway.
//!
//! Performance contract: Gemini hooks run synchronously in the agent loop,
//! so the bridge is one local TCP connect with a hard timeout, always exits
//! 0, and prints exactly `{}` on stdout — the minimum valid JSON. MVP
//! therefore never slows Gemini down and never blocks, modifies, injects,
//! approves or rejects anything.
//!
//! Known limitations:
//! - Hooks are always synchronous; the `timeout` bounds a hung bridge.
//! - Multiple Gemini sessions are treated as one logical agent stream.
//!
//! Extension future: Gemini CLI extensions can bundle hooks; the same
//! [`HOOKS`] table can be shipped inside an extension's settings without
//! any lifecycle-logic changes.

mod config;

use std::path::Path;

use crate::agent::event::AgentEvent;
use crate::agent::integrations::ProviderStatus;
use crate::ipc;

pub use config::{GeminiConfig, config_path};

/// One Gemini hook entry used by the installer.
pub struct HookEntry {
    /// Gemini hook event name.
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
        event: "BeforeAgent",
        matcher: None,
        agent_event: AgentEvent::Working,
    },
    HookEntry {
        event: "BeforeTool",
        matcher: None,
        agent_event: AgentEvent::Working,
    },
    HookEntry {
        event: "AfterTool",
        matcher: None,
        agent_event: AgentEvent::Working,
    },
    HookEntry {
        event: "Notification",
        matcher: None,
        agent_event: AgentEvent::NeedsInput,
    },
    HookEntry {
        event: "AfterAgent",
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
    std::env::current_exe().map_err(|e| format!("cannot resolve the mvp binary path: {e}"))
}

/// Installs the MVP hooks into the user's Gemini CLI settings.
/// Idempotent; never modifies the file if it cannot be parsed.
pub fn install() -> Result<(), String> {
    install_with_binary(&current_binary()?)
}

fn install_with_binary(binary: &Path) -> Result<(), String> {
    let path = config_path();
    let mut config = GeminiConfig::load(&path)?;
    let changes = config.install_hooks(binary);

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
    let mut config = GeminiConfig::load(&path)?;
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

/// Prints the integration status (see the README for the output shape).
pub fn status() {
    println!("Gemini CLI integration");
    println!();
    print_provider_status(&status_state());
}

/// Structured status used by the `integrations` overview.
pub fn status_state() -> ProviderStatus {
    match GeminiConfig::load(&config_path()) {
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

/// True when any MVP pieces exist but are missing/stale.
pub fn needs_repair() -> bool {
    matches!(status_state(), ProviderStatus::Outdated(_))
}

/// True when the Gemini CLI or its config directory is present.
pub fn detected() -> bool {
    crate::agent::integrations::command_on_path("gemini") || config_dir_present()
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
    let overall = match (hooks, mvp_running) {
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
    fn hook_events_are_the_documented_gemini_events() {
        let names: Vec<&str> = HOOKS.iter().map(|h| h.event).collect();
        assert_eq!(
            names,
            vec![
                "SessionStart",
                "BeforeAgent",
                "BeforeTool",
                "AfterTool",
                "Notification",
                "AfterAgent",
                "SessionEnd"
            ]
        );
    }

    #[test]
    fn only_notification_maps_to_needs_input() {
        let notification = HOOKS.iter().find(|h| h.event == "Notification").unwrap();
        assert_eq!(notification.agent_event, AgentEvent::NeedsInput);
        for hook in HOOKS.iter().filter(|h| h.event != "Notification") {
            assert_ne!(hook.agent_event, AgentEvent::NeedsInput);
        }
    }

    #[test]
    fn after_agent_is_the_completion_signal() {
        let after_agent = HOOKS.iter().find(|h| h.event == "AfterAgent").unwrap();
        assert_eq!(after_agent.agent_event, AgentEvent::Completed);
    }
}
