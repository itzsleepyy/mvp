//! Safe merging of WaitState hooks into the Claude Code settings file.
//!
//! Hard requirements honoured here:
//! - the existing configuration is never replaced or reordered destructively
//! - unrelated hooks, permissions, plugins and settings stay untouched
//! - a backup is written before any modification
//! - an unparseable file is never modified
//! - install and uninstall are idempotent

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::agent::claude::HOOKS;
use crate::agent::event::AgentEvent;

/// The user-level Claude Code settings file, honouring `CLAUDE_CONFIG_DIR`.
pub fn config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir).join("settings.json");
    }
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    home.join(".claude").join("settings.json")
}

/// How many of the six hook events have an up-to-date WaitState hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookStatus {
    pub installed: usize,
    pub total: usize,
}

impl HookStatus {
    pub fn completeness(self) -> (usize, usize) {
        (self.installed, self.total)
    }
}

/// A parsed Claude settings file, loaded for merging.
#[derive(Debug)]
pub struct ClaudeConfig {
    path: PathBuf,
    data: Value,
}

impl ClaudeConfig {
    /// Loads the settings file. A missing file yields an empty object (the
    /// file will be created on save). A malformed file is an error and is
    /// never modified.
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = match std::fs::read(path) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
                if !value.is_object() {
                    return Err(format!("{} does not contain a JSON object", path.display()));
                }
                value
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        Ok(Self {
            path: path.to_path_buf(),
            data,
        })
    }

    /// Merges the WaitState hook set into the settings. Returns the number
    /// of changes made (0 when already installed). Existing hooks and all
    /// other settings are preserved.
    pub fn install_hooks(&mut self, binary: &Path) -> usize {
        let binary = binary.display().to_string();
        let mut changes = 0;
        for entry in HOOKS {
            if self.merge_entry(entry.event, entry.matcher, entry.agent_event, &binary) {
                changes += 1;
            }
        }
        changes
    }

    /// Removes every WaitState-owned hook. Returns the number removed.
    /// Handlers are identified by the executable name (`waitstate`) plus
    /// the `agent-event` argument, so a moved binary is still recognised.
    pub fn uninstall_hooks(&mut self) -> usize {
        let mut removed = 0;
        let Some(hooks) = self.data.get_mut("hooks").and_then(Value::as_object_mut) else {
            return 0;
        };
        for groups in hooks.values_mut() {
            let Some(groups) = groups.as_array_mut() else {
                continue;
            };
            for group in groups.iter_mut() {
                let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                    continue;
                };
                removed += handlers.len() - handlers.iter().filter(|h| !is_waitstate(h)).count();
                handlers.retain(|h| !is_waitstate(h));
            }
            // Drop now-empty matcher groups so we leave no WaitState residue.
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| !handlers.is_empty())
            });
        }
        // Drop now-empty event arrays.
        hooks.retain(|_, groups| groups.as_array().is_some_and(|g| !g.is_empty()));
        removed
    }

    /// True when all hook events have an up-to-date WaitState hook.
    pub fn hook_status(&self) -> HookStatus {
        let installed = HOOKS
            .iter()
            .filter(|entry| self.find_handler(entry.event, entry.agent_event).is_some())
            .count();
        HookStatus {
            installed,
            total: HOOKS.len(),
        }
    }

    /// Writes the settings back, creating a timestamped backup of the
    /// previous file first. The write is atomic (temp file + rename).
    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        if self.path.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = self
                .path
                .with_file_name(format!("settings.json.ws-backup-{stamp}"));
            std::fs::copy(&self.path, &backup).map_err(|e| {
                format!(
                    "cannot back up {} to {}: {e}",
                    self.path.display(),
                    backup.display()
                )
            })?;
        }
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("cannot serialise settings: {e}"))?;
        let tmp = self.path.with_file_name("settings.json.ws-tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("cannot write {}: {e}", self.path.display()))?;
        Ok(())
    }

    /// Merges one hook entry: updates an existing WaitState handler (stale
    /// binary path), or appends a handler to a matcher-compatible group, or
    /// creates a fresh group. Returns true when the file changed.
    fn merge_entry(
        &mut self,
        event: &str,
        matcher: Option<&str>,
        agent_event: AgentEvent,
        binary: &str,
    ) -> bool {
        if let Some(handler) = self.find_handler_mut(event, agent_event) {
            if handler.get("command").and_then(Value::as_str) == Some(binary) {
                return false; // already installed and up to date
            }
            handler["command"] = Value::String(binary.to_string());
            return true; // stale binary path updated
        }

        let handler = hook_handler(binary, agent_event);
        let groups = self.ensure_event_groups(event);
        for group in groups.iter_mut() {
            let group_matcher = group.get("matcher").and_then(Value::as_str);
            if group_matcher == matcher {
                group["hooks"]
                    .as_array_mut()
                    .expect("groups carry hooks arrays")
                    .push(handler);
                return true;
            }
        }
        let mut group = json!({ "hooks": [handler] });
        if let Some(m) = matcher {
            group["matcher"] = Value::String(m.to_string());
        }
        groups.push(group);
        true
    }

    fn ensure_event_groups(&mut self, event: &str) -> &mut Vec<Value> {
        let hooks = self
            .data
            .as_object_mut()
            .expect("config is an object")
            .entry("hooks")
            .or_insert_with(|| json!({}));
        let hooks = hooks.as_object_mut().expect("hooks is an object");
        hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("hook events hold arrays")
    }

    fn find_handler(&self, event: &str, agent_event: AgentEvent) -> Option<&Value> {
        let groups = self.data.get("hooks")?.get(event)?.as_array()?;
        groups.iter().find_map(|group| {
            group
                .get("hooks")?
                .as_array()?
                .iter()
                .find(|h| is_waitstate_for(h, agent_event))
        })
    }

    fn find_handler_mut(&mut self, event: &str, agent_event: AgentEvent) -> Option<&mut Value> {
        let groups = self.data.get_mut("hooks")?.get_mut(event)?.as_array_mut()?;
        groups.iter_mut().find_map(|group| {
            group
                .get_mut("hooks")?
                .as_array_mut()?
                .iter_mut()
                .find(|h| is_waitstate_for(h, agent_event))
        })
    }
}

/// The exec-form command handler that sends one agent event.
fn hook_handler(binary: &str, event: AgentEvent) -> Value {
    json!({
        "type": "command",
        "command": binary,
        "args": ["agent-event", event.as_str()],
        "async": true,
        "timeout": 5,
    })
}

/// True for any WaitState hook handler, whatever event it sends.
fn is_waitstate(handler: &Value) -> bool {
    let is_binary = handler
        .get("command")
        .and_then(Value::as_str)
        .map(|c| crate::agent::claude::binary_name_matches(Path::new(c)))
        .unwrap_or(false);
    let is_event_command = handler
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|args| args.first().and_then(Value::as_str) == Some("agent-event"));
    is_binary && is_event_command
}

/// True for a WaitState hook handler sending `event`.
fn is_waitstate_for(handler: &Value, event: AgentEvent) -> bool {
    is_waitstate(handler)
        && handler
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|args| args.get(1).and_then(Value::as_str) == Some(event.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINARY_A: &str = "/opt/waitstate";
    const BINARY_B: &str = "/usr/local/bin/waitstate";

    fn temp_config(name: &str, content: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "waitstate_claude_test_{}_{}/settings.json",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        if !content.is_empty() {
            std::fs::write(&path, content).unwrap();
        }
        path
    }

    fn load_str(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn installed(config: &ClaudeConfig) -> bool {
        config.hook_status().installed == config.hook_status().total
    }

    #[test]
    fn install_creates_hooks_in_an_empty_config() {
        let path = temp_config("empty", "");
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 6);
        assert!(installed(&config));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let hooks = raw["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 6);
        let notification = hooks["Notification"].as_array().unwrap();
        assert_eq!(notification.len(), 1);
        assert_eq!(
            notification[0]["matcher"].as_str(),
            Some("permission_prompt|agent_needs_input|elicitation_dialog|elicitation_url_dialog")
        );
        let handler = &notification[0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], BINARY_A);
        assert_eq!(handler["args"][0], "agent-event");
        assert_eq!(handler["args"][1], "needs-input");
        assert_eq!(handler["async"], true);
    }

    #[test]
    fn install_preserves_existing_settings() {
        let path = temp_config(
            "existing",
            r#"{
                "permissions": {"allow": ["Bash"]},
                "model": "sonnet",
                "enableAllProjectMcpServers": true
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        assert_eq!(raw["permissions"]["allow"][0], "Bash");
        assert_eq!(raw["model"], "sonnet");
        assert_eq!(raw["enableAllProjectMcpServers"], true);
        assert!(raw["hooks"].is_object());
    }

    #[test]
    fn install_preserves_existing_unrelated_hooks() {
        let path = temp_config(
            "hooks",
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/home/user/block-rm.sh"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let pretooluse = raw["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pretooluse.len(), 1);
        assert_eq!(
            pretooluse[0]["hooks"][0]["command"],
            "/home/user/block-rm.sh"
        );
        assert_eq!(pretooluse[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn install_is_idempotent() {
        let path = temp_config("idem", "");
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 6);
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 0);

        config.save().unwrap();
        let mut reloaded = ClaudeConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_A)), 0);
        assert!(installed(&reloaded));
    }

    #[test]
    fn install_updates_a_stale_binary_path_without_duplicating() {
        let path = temp_config("move", "");
        let mut config = ClaudeConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let mut reloaded = ClaudeConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_B)), 6);
        reloaded.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let handler = &raw["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(handler["command"], BINARY_B);
        let session_start = raw["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn malformed_config_is_reported_and_never_modified() {
        let path = temp_config("malformed", "{ this is not json");
        let original = load_str(&path);
        assert!(ClaudeConfig::load(&path).is_err());
        assert_eq!(load_str(&path), original);
    }

    #[test]
    fn non_object_config_is_rejected() {
        let path = temp_config("array", "[1, 2, 3]");
        let err = ClaudeConfig::load(&path).unwrap_err();
        assert!(err.contains("JSON object"), "unexpected error: {err}");
    }

    #[test]
    fn uninstall_removes_only_waitstate_hooks() {
        let path = temp_config(
            "uninstall",
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/home/user/block-rm.sh"
                                }
                            ]
                        }
                    ],
                    "Stop": [
                        {
                            "matcher": "*",
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/home/user/notify.sh"
                                },
                                {
                                    "type": "command",
                                    "command": "/opt/waitstate",
                                    "args": ["agent-event", "completed"],
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let pretooluse = raw["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pretooluse.len(), 1, "unrelated hook must survive");
        assert_eq!(
            pretooluse[0]["hooks"][0]["command"],
            "/home/user/block-rm.sh"
        );
        let stop = raw["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(stop[0]["hooks"][0]["command"], "/home/user/notify.sh");
    }

    #[test]
    fn uninstall_is_idempotent() {
        let path = temp_config("unidem", "");
        let mut config = ClaudeConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        assert_eq!(config.uninstall_hooks(), 6);
        assert_eq!(config.uninstall_hooks(), 0);
        assert_eq!(config.hook_status().installed, 0);
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        assert_eq!(
            raw["hooks"].as_object().unwrap().len(),
            0,
            "empty event arrays must be removed"
        );
    }

    #[test]
    fn uninstall_removes_moved_binary_by_name() {
        // Ownership is detected by executable name, not path.
        let path = temp_config(
            "movedbin",
            r#"{
                "hooks": {
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "C:\\Program Files\\WaitState\\waitstate.exe",
                                    "args": ["agent-event", "completed"],
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
    }

    #[test]
    fn save_creates_a_backup_before_modifying() {
        let path = temp_config("backup", r#"{"model": "sonnet"}"#);
        let mut config = ClaudeConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let dir = path.parent().unwrap();
        let backups: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("settings.json.ws-backup-"))
            .collect();
        assert_eq!(backups.len(), 1, "exactly one backup expected");
        assert!(!load_str(&path).contains("ws-backup"));
    }

    #[test]
    fn hook_status_reports_partial_installs() {
        let path = temp_config(
            "partial",
            r#"{
                "hooks": {
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/opt/waitstate",
                                    "args": ["agent-event", "completed"],
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let config = ClaudeConfig::load(&path).unwrap();
        let status = config.hook_status();
        assert_eq!(status.installed, 1);
        assert_eq!(status.total, 6);
        assert_eq!(status.completeness(), (1, 6));
    }

    #[test]
    fn handlers_sending_other_agent_events_count_as_waitstate() {
        let path = temp_config(
            "other",
            r#"{
                "hooks": {
                    "PostToolUse": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/opt/waitstate",
                                    "args": ["agent-event", "working"],
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.hook_status().installed, 1);
        assert_eq!(config.uninstall_hooks(), 1);
    }

    #[test]
    fn similar_but_foreign_commands_are_untouched() {
        // Same args shape but not our binary: leave it alone.
        let path = temp_config(
            "foreign",
            r#"{
                "hooks": {
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "/opt/something-else",
                                    "args": ["agent-event", "completed"]
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = ClaudeConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 0);
        assert_eq!(config.hook_status().installed, 0);
    }

    #[test]
    fn config_path_honours_claude_config_dir() {
        let original = std::env::var_os("CLAUDE_CONFIG_DIR");
        // SAFETY: this test owns the process env; the guard restores the
        // original value before any other code runs.
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", "/tmp/claude-test-config");
        }
        let path = config_path();
        assert_eq!(path, PathBuf::from("/tmp/claude-test-config/settings.json"));
        match original {
            Some(v) => unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", v) },
            None => unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") },
        }
    }
}
