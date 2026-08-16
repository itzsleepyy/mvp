//! Unified view over every agent integration: one overview table, one
//! install-all command and one repair command. Each provider operation is
//! independent — a broken Gemini config never affects Codex install or
//! Claude status (§37 failure isolation).
//!
//! Providers are registered once here; CLI dispatch and the overview walk
//! the registry instead of branching per agent.

use std::path::{Path, PathBuf};

use crate::agent::status::AgentKind;
use crate::ipc;

/// Structured integration state for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    /// No WaitState-owned pieces found.
    NotInstalled,
    /// Everything installed and up to date.
    Current,
    /// WaitState pieces exist but are missing/stale; `detail` explains.
    Outdated(String),
    /// The provider configuration cannot be read; `detail` explains.
    Broken(String),
}

impl ProviderStatus {
    /// The last-column label for the overview table.
    pub fn overall(&self, waitstate_running: bool) -> String {
        match self {
            Self::Current if waitstate_running => "ready".to_string(),
            Self::Current => "WaitState not running".to_string(),
            Self::Outdated(_) => "outdated".to_string(),
            Self::Broken(_) => "broken".to_string(),
            Self::NotInstalled => String::new(),
        }
    }

    /// The middle-column label for the overview table.
    pub fn label(&self, kind: AgentKind) -> String {
        match self {
            Self::Current => {
                if kind == AgentKind::Codex {
                    "installed · review /hooks".to_string()
                } else {
                    "installed".to_string()
                }
            }
            Self::NotInstalled => "not installed".to_string(),
            Self::Outdated(detail) => format!("outdated ({detail})"),
            Self::Broken(detail) => format!("broken ({detail})"),
        }
    }
}

/// Everything the unified commands need to know about one provider.
pub struct Provider {
    pub kind: AgentKind,
    pub install: fn() -> Result<(), String>,
    pub status_state: fn() -> ProviderStatus,
    pub detected: fn() -> bool,
    pub needs_repair: fn() -> bool,
}

/// All supported providers in display order.
pub fn providers() -> [&'static Provider; 4] {
    use crate::agent::{claude, codex, gemini, opencode};
    static CLAUDE: Provider = Provider {
        kind: AgentKind::ClaudeCode,
        install: claude::install,
        status_state: claude::status_state,
        detected: claude::detected,
        needs_repair: claude::needs_repair,
    };
    static CODEX: Provider = Provider {
        kind: AgentKind::Codex,
        install: codex::install,
        status_state: codex::status_state,
        detected: codex::detected,
        needs_repair: codex::needs_repair,
    };
    static GEMINI: Provider = Provider {
        kind: AgentKind::GeminiCli,
        install: gemini::install,
        status_state: gemini::status_state,
        detected: gemini::detected,
        needs_repair: gemini::needs_repair,
    };
    static OPENCODE: Provider = Provider {
        kind: AgentKind::OpenCode,
        install: opencode::install,
        status_state: opencode::status_state,
        detected: opencode::detected,
        needs_repair: opencode::needs_repair,
    };
    [&CLAUDE, &CODEX, &GEMINI, &OPENCODE]
}

/// Prints the overview table: one row per provider with its integration
/// state and whether it is ready to drive WaitState right now.
pub fn run_overview() {
    println!("WaitState integrations");
    println!();
    let running = ipc::client::running();
    for provider in providers() {
        let state = (provider.status_state)();
        println!(
            "{:<14} {:<28} {}",
            provider.kind.name(),
            state.label(provider.kind),
            state.overall(running)
        );
    }
    println!();
    println!("Detected agents: {}", detected_names().join(", "));
    println!();
    println!("Install:  waitstate <agent> install     (e.g. waitstate codex install)");
    println!("Overview: waitstate integrations");
    println!("All at once: waitstate integrations install [--all]");
    println!("Fix partial installs: waitstate integrations repair");
}

/// Installs integrations for detected agents (or every agent with `all`).
/// Agents that are clearly absent are skipped unless `--all` is given.
pub fn run_install(all: bool) {
    for provider in providers() {
        let detected = (provider.detected)();
        if !all && !detected {
            println!(
                "{:<14} skipped ({:?} not detected)",
                provider.kind.name(),
                provider.kind.id()
            );
            continue;
        }
        match (provider.install)() {
            Ok(()) => {}
            Err(err) => println!("{:<14} failed: {err}", provider.kind.name()),
        }
    }
}

/// Repairs missing or outdated WaitState-owned pieces for providers that
/// have any. Install is idempotent, so repair = reinstall for those.
pub fn run_repair() {
    for provider in providers() {
        if !(provider.needs_repair)() {
            println!("{:<14} nothing to repair", provider.kind.name());
            continue;
        }
        match (provider.install)() {
            Ok(()) => println!("{:<14} repaired", provider.kind.name()),
            Err(err) => println!("{:<14} repair failed: {err}", provider.kind.name()),
        }
    }
}

fn detected_names() -> Vec<&'static str> {
    providers()
        .iter()
        .filter(|p| (p.detected)())
        .map(|p| p.kind.id())
        .collect()
}

/// True when an executable named `name` exists on `PATH`.
pub fn command_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    command_in_dirs(name, &dirs)
}

/// Same as [`command_on_path`] with explicit directories (used by tests).
pub fn command_in_dirs(name: &str, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| executable_at(&dir.join(name)))
}

/// True when `candidate` is a runnable file.
fn executable_at(candidate: &Path) -> bool {
    let Ok(metadata) = candidate.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_in_dirs_finds_an_executable() {
        let dir = std::env::temp_dir().join(format!("waitstate_path_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("codex");
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(command_in_dirs("codex", std::slice::from_ref(&dir)));
        assert!(!command_in_dirs("codex", &[std::env::temp_dir()]));
        assert!(!command_in_dirs("claude", std::slice::from_ref(&dir)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_in_dirs_ignores_missing_dirs() {
        assert!(!command_in_dirs(
            "codex",
            &[PathBuf::from("/definitely/not/here")]
        ));
    }

    #[test]
    fn command_in_dirs_rejects_non_executable_files() {
        let dir = std::env::temp_dir().join(format!("waitstate_path_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gemini");
        std::fs::write(&file, "not executable").unwrap();
        #[cfg(unix)]
        assert!(!command_in_dirs("gemini", std::slice::from_ref(&dir)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn provider_status_labels_are_stable() {
        assert_eq!(ProviderStatus::Current.overall(true), "ready");
        assert_eq!(
            ProviderStatus::Current.overall(false),
            "WaitState not running"
        );
        assert_eq!(ProviderStatus::NotInstalled.overall(false), "");
        assert_eq!(
            ProviderStatus::Outdated("2/6 hooks".into()).overall(true),
            "outdated"
        );
        assert_eq!(ProviderStatus::Broken("x".into()).overall(true), "broken");
        assert_eq!(
            ProviderStatus::Current.label(AgentKind::Codex),
            "installed · review /hooks"
        );
        assert_eq!(
            ProviderStatus::Current.label(AgentKind::ClaudeCode),
            "installed"
        );
    }

    #[test]
    fn registry_covers_all_agent_kinds() {
        let kinds: Vec<AgentKind> = providers().iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            vec![
                AgentKind::ClaudeCode,
                AgentKind::Codex,
                AgentKind::GeminiCli,
                AgentKind::OpenCode
            ]
        );
    }
}
