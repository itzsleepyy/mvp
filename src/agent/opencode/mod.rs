//! The OpenCode adapter.
//!
//! OpenCode extends via JavaScript plugins (Bun runtime) rather than
//! command hooks, so this adapter ships a tiny plugin file into OpenCode's
//! global plugin directory. Only the normalized output matches the other
//! adapters; the mechanism is OpenCode's own.
//!
//! Chosen lifecycle mapping (validated against the official plugin event
//! types and the plugin SDK):
//!
//! | OpenCode signal                 | AgentEvent  |
//! |---------------------------------|-------------|
//! | `session.created` (event)       | `Started`   |
//! | `session.status` = busy (event) | `Working`   |
//! | `session.status` = idle (event) | `Completed` |
//! | `session.idle` (deprecated)     | `Completed` |
//! | `permission.ask` (hook)         | `NeedsInput`|
//! | `permission.replied` (event)    | `Working`   |
//! | `tool.execute.before/after`     | `Working`   |
//! | `session.deleted` (event)       | `Stopped`   |
//!
//! `permission.ask` is observational: the plugin sends the event and leaves
//! the default `ask` decision untouched, so OpenCode's normal permission
//! prompt continues exactly as before.
//!
//! Known limitations:
//! - `session.error` is deliberately unmapped (an errored session is not a
//!   clean completion).
//! - Multiple OpenCode sessions are treated as one logical agent stream.
//! - The plugin spawns the waitstate binary per event via `Bun.spawn`
//!   (fire-and-forget); if WaitState is not running the spawn fails
//!   silently.

mod plugin;

use std::path::Path;

use crate::agent::integrations::ProviderStatus;
use crate::ipc;

pub use plugin::{
    PLUGIN_FILE_NAME, PLUGIN_VERSION, installed_version, is_waitstate_owned, plugin_dir,
    plugin_source,
};

fn current_binary() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot resolve the waitstate binary path: {e}"))
}

/// Installs the WaitState plugin into OpenCode's global plugin directory.
/// Idempotent; older WaitState plugin files are upgraded in place. A file
/// without our marker is never overwritten.
pub fn install() -> Result<(), String> {
    install_with_binary(&current_binary()?)
}

fn install_with_binary(binary: &Path) -> Result<(), String> {
    install_into(&plugin_dir(), binary)
}

fn install_into(dir: &Path, binary: &Path) -> Result<(), String> {
    let path = dir.join(PLUGIN_FILE_NAME);
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let source = plugin_source(binary);
    match std::fs::read_to_string(&path) {
        Ok(existing) => {
            if !is_waitstate_owned(&existing) {
                return Err(format!(
                    "{} exists but is not WaitState-owned; remove it manually first",
                    path.display()
                ));
            }
            if existing == source {
                println!("WaitState plugin is already installed and up to date.");
                println!("Plugin: {}", path.display());
                return Ok(());
            }
            let version = installed_version(&existing).unwrap_or(0);
            let backup = path.with_file_name(format!("{PLUGIN_FILE_NAME}.ws-backup-{version}"));
            std::fs::copy(&path, &backup).map_err(|e| {
                format!(
                    "cannot back up {} to {}: {e}",
                    path.display(),
                    backup.display()
                )
            })?;
            std::fs::write(&path, source)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            println!("WaitState plugin updated (v{version} → v{PLUGIN_VERSION}).");
            println!("Plugin: {}", path.display());
            println!("A backup of the previous plugin was saved next to it.");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::write(&path, source)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            println!("WaitState plugin installed.");
            println!("Plugin: {}", path.display());
            Ok(())
        }
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Removes the WaitState plugin file — and only that file.
pub fn uninstall() -> Result<(), String> {
    uninstall_from(&plugin_dir())
}

fn uninstall_from(dir: &Path) -> Result<(), String> {
    let path = dir.join(PLUGIN_FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("No WaitState plugin found — nothing to remove.");
            return Ok(());
        }
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    if !is_waitstate_owned(&content) {
        println!(
            "{} is not a WaitState plugin — left untouched.",
            path.display()
        );
        return Ok(());
    }
    std::fs::remove_file(&path).map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
    println!("Removed WaitState plugin.");
    println!("Plugin: {}", path.display());
    Ok(())
}

/// Prints the integration status (see the README for the output shape).
pub fn status() {
    println!("OpenCode integration");
    println!();
    print_provider_status(&status_state());
}

/// Structured status used by the `integrations` overview.
pub fn status_state() -> ProviderStatus {
    status_state_at(&plugin_dir())
}

fn status_state_at(dir: &Path) -> ProviderStatus {
    let path = dir.join(PLUGIN_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(content) => match installed_version(&content) {
            Some(version) if version == PLUGIN_VERSION => ProviderStatus::Current,
            Some(version) => ProviderStatus::Outdated(format!(
                "plugin v{version} found, v{PLUGIN_VERSION} expected"
            )),
            None => ProviderStatus::Broken(format!("{} is not a WaitState plugin", path.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ProviderStatus::NotInstalled,
        Err(e) => ProviderStatus::Broken(format!("cannot read {}: {e}", path.display())),
    }
}

/// True when the plugin exists but is outdated.
pub fn needs_repair() -> bool {
    matches!(status_state(), ProviderStatus::Outdated(_))
}

/// True when the OpenCode CLI or its config directory is present.
pub fn detected() -> bool {
    crate::agent::integrations::command_on_path("opencode") || plugin_dir().exists()
}

fn print_provider_status(plugin: &ProviderStatus) {
    let plugin_line = match plugin {
        ProviderStatus::Current => "installed".to_string(),
        ProviderStatus::NotInstalled => "not installed".to_string(),
        ProviderStatus::Outdated(detail) => format!("outdated ({detail})"),
        ProviderStatus::Broken(err) => format!("unreadable ({err})"),
    };
    println!("Plugin:     {plugin_line}");

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
    let overall = match (plugin, waitstate_running) {
        (ProviderStatus::Current, true) => "ready",
        (ProviderStatus::Current, false) => "plugin installed",
        (ProviderStatus::Outdated(_), _) => "outdated",
        (ProviderStatus::Broken(_), _) => "broken",
        (ProviderStatus::NotInstalled, _) => "not integrated",
    };
    println!("Status: {overall}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINARY_A: &str = "/opt/waitstate";

    fn temp_plugin_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "waitstate_opencode_test_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_writes_an_owned_plugin_file() {
        let dir = temp_plugin_dir("install");
        install_into(&dir, Path::new(BINARY_A)).unwrap();
        let path = dir.join(PLUGIN_FILE_NAME);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(is_waitstate_owned(&content));
        assert!(content.contains("Bun.spawn"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_is_idempotent_and_updates_older_versions() {
        let dir = temp_plugin_dir("idem");

        // First install creates the file.
        install_into(&dir, Path::new(BINARY_A)).unwrap();
        let path = dir.join(PLUGIN_FILE_NAME);
        let first = std::fs::read_to_string(&path).unwrap();

        // Second install with the same binary is a no-op (no new backup).
        install_into(&dir, Path::new(BINARY_A)).unwrap();
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("ws-backup"))
            .collect();
        assert_eq!(backups.len(), 0, "idempotent installs write no backups");

        // An older version is upgraded with a backup.
        let older = first.replace(
            format!("// WAITSTATE_OPENCODE_PLUGIN v{PLUGIN_VERSION}").as_str(),
            "// WAITSTATE_OPENCODE_PLUGIN v0",
        );
        std::fs::write(&path, &older).unwrap();
        install_into(&dir, Path::new(BINARY_A)).unwrap();
        let upgraded = std::fs::read_to_string(&path).unwrap();
        assert_eq!(installed_version(&upgraded), Some(PLUGIN_VERSION));
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("ws-backup"))
            .collect();
        assert_eq!(backups.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_never_overwrites_a_foreign_plugin_file() {
        let dir = temp_plugin_dir("foreign");
        let path = dir.join(PLUGIN_FILE_NAME);
        let foreign = "export const Mine = async () => ({})";
        std::fs::write(&path, foreign).unwrap();

        let err = install_into(&dir, Path::new(BINARY_A)).unwrap_err();
        assert!(err.contains("not WaitState-owned"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), foreign);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uninstall_removes_only_our_plugin_and_preserves_others() {
        let dir = temp_plugin_dir("uninstall");
        let sibling = dir.join("theirs.js");
        std::fs::write(&sibling, "export const Theirs = async () => ({})").unwrap();

        install_into(&dir, Path::new(BINARY_A)).unwrap();
        assert!(dir.join(PLUGIN_FILE_NAME).exists());
        uninstall_from(&dir).unwrap();
        assert!(
            !dir.join(PLUGIN_FILE_NAME).exists(),
            "our plugin must be removed"
        );
        assert!(sibling.exists(), "other plugins must survive");

        // Uninstalling again is a quiet no-op.
        uninstall_from(&dir).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uninstall_refuses_to_remove_foreign_files() {
        let dir = temp_plugin_dir("foreignun");
        let path = dir.join(PLUGIN_FILE_NAME);
        let foreign = "export const Mine = async () => ({})";
        std::fs::write(&path, foreign).unwrap();

        uninstall_from(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), foreign);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_distinguishes_states() {
        let dir = temp_plugin_dir("status");

        assert_eq!(status_state_at(&dir), ProviderStatus::NotInstalled);

        install_into(&dir, Path::new(BINARY_A)).unwrap();
        assert_eq!(status_state_at(&dir), ProviderStatus::Current);
        assert!(!needs_repair_state(&dir));

        let path = dir.join(PLUGIN_FILE_NAME);
        let older = std::fs::read_to_string(&path).unwrap().replace(
            format!("// WAITSTATE_OPENCODE_PLUGIN v{PLUGIN_VERSION}").as_str(),
            "// WAITSTATE_OPENCODE_PLUGIN v0",
        );
        std::fs::write(&path, older).unwrap();
        assert!(matches!(status_state_at(&dir), ProviderStatus::Outdated(_)));
        assert!(needs_repair_state(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn needs_repair_state(dir: &Path) -> bool {
        matches!(status_state_at(dir), ProviderStatus::Outdated(_))
    }
}
