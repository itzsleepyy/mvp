//! Safe merging of MVP hooks into the Gemini CLI settings file
//! (`~/.gemini/settings.json`).
//!
//! Gemini specifics:
//! - hooks run **synchronously** as part of the agent loop, so the MVP
//!   bridge must be extremely fast (one local TCP connect, hard timeout)
//!   and never fail loudly: it always exits 0;
//! - the "golden rule" of Gemini hooks: stdout may contain only one JSON
//!   object. The `hook` bridge prints `{}` — valid, decision-free JSON —
//!   so MVP can never block, modify, approve or reject anything;
//! - debug output, if any, goes to stderr only;
//! - extensions can later bundle the same hook table (the `hooks` section
//!   of settings.json) without any lifecycle-logic changes.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::agent::event::AgentEvent;
use crate::agent::gemini::HOOKS;
use crate::agent::hooks_json::{HookSpec, HooksJson, command_binary_name, shell_quote};

/// The user-level Gemini CLI settings file.
pub fn config_path() -> PathBuf {
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    home.join(".gemini").join("settings.json")
}

/// The hook specs for the shared merge machinery.
pub fn hook_specs() -> Vec<HookSpec> {
    HOOKS
        .iter()
        .map(|entry| HookSpec {
            event: entry.event,
            matcher: entry.matcher,
            agent_event: entry.agent_event,
        })
        .collect()
}

/// A parsed Gemini settings file, loaded for merging.
#[derive(Debug)]
pub struct GeminiConfig {
    inner: HooksJson,
}

impl GeminiConfig {
    /// Loads the settings file. A missing file yields an empty object (the
    /// file will be created on save). A malformed file is an error and is
    /// never modified.
    pub fn load(path: &Path) -> Result<Self, String> {
        Ok(Self {
            inner: HooksJson::load(path)?,
        })
    }

    /// Merges the MVP hook set into the settings. Returns the number
    /// of changes made (0 when already installed). Existing hooks and all
    /// other settings are preserved.
    pub fn install_hooks(&mut self, binary: &Path) -> usize {
        self.inner.install(
            &hook_specs(),
            &binary.display().to_string(),
            gemini_handler,
            is_mvp_for,
        )
    }

    /// Removes every MVP-owned hook. Returns the number removed.
    pub fn uninstall_hooks(&mut self) -> usize {
        self.inner.uninstall(is_mvp)
    }

    /// True when all hook events have an up-to-date MVP hook.
    pub fn hook_status(&self) -> (usize, usize) {
        self.inner.status(&hook_specs(), is_mvp_for)
    }

    /// Writes the settings back, creating a timestamped backup of the
    /// previous file first. The write is atomic (temp file + rename).
    pub fn save(&self) -> Result<(), String> {
        self.inner.save()
    }
}

/// The Gemini handler: a shell command string calling the `hook` bridge,
/// which sends the event and prints `{}` (the minimum valid JSON). The
/// timeout is in milliseconds and must stay low: Gemini waits for hooks
/// synchronously.
fn gemini_handler(binary: &str, event: AgentEvent) -> Value {
    let command = format!("{} hook gemini {}", shell_quote(binary), event.as_str());
    json!({
        "name": "mvp",
        "type": "command",
        "command": command,
        "timeout": 2000,
    })
}

/// True for any MVP hook handler in a Gemini file, whatever event it
/// sends. Ownership is detected by the executable name (`mvp`, plus the
/// pre-rebrand `waitstate` so old hooks can still be upgraded or removed)
/// and the `hook gemini` bridge arguments, so a moved binary is still
/// recognised.
fn is_mvp(handler: &Value) -> bool {
    let Some(command) = handler.get("command").and_then(Value::as_str) else {
        return false;
    };
    let binary_name = command_binary_name(first_token(command));
    crate::agent::owned_binary_name(binary_name) && command.contains("hook gemini")
}

/// True for a MVP hook handler sending `event`.
fn is_mvp_for(handler: &Value, event: AgentEvent) -> bool {
    is_mvp(handler)
        && handler
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|c| c.contains(&format!("hook gemini {}", event.as_str())))
}

/// Extracts the first shell token of a command string, honouring single
/// and double quotes. Our installer writes `'<binary>' hook ...`, but
/// ownership checks must also tolerate hand-edited quoting.
fn first_token(command: &str) -> &str {
    let s = command.trim_start();
    for quote in ['\'', '"'] {
        if let Some(rest) = s.strip_prefix(quote)
            && let Some(end) = rest.find(quote)
        {
            return &rest[..end];
        }
    }
    match s.find(char::is_whitespace) {
        Some(end) => &s[..end],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINARY_A: &str = "/opt/mvp";
    const BINARY_B: &str = "/usr/local/bin/mvp";

    fn temp_config(name: &str, content: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "mvp_gemini_test_{}_{}/settings.json",
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

    fn installed(config: &GeminiConfig) -> bool {
        let (installed, total) = config.hook_status();
        installed == total && total > 0
    }

    #[test]
    fn install_creates_hooks_in_an_empty_config() {
        let path = temp_config("empty", "");
        let mut config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 7);
        assert!(installed(&config));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let hooks = raw["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 7);
        let handler = &hooks["Notification"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["name"], "mvp");
        assert_eq!(handler["command"], "'/opt/mvp' hook gemini needs-input");
        assert_eq!(handler["timeout"], 2000);
        let before_agent = &hooks["BeforeAgent"][0]["hooks"][0];
        assert_eq!(before_agent["command"], "'/opt/mvp' hook gemini working");
    }

    #[test]
    fn install_preserves_existing_settings_and_hooks() {
        let path = temp_config(
            "existing",
            r#"{
                "model": {"name": "gemini-2.5-pro"},
                "hooks": {
                    "BeforeTool": [
                        {
                            "matcher": "run_shell_command",
                            "hooks": [
                                {
                                    "name": "security-check",
                                    "type": "command",
                                    "command": "$GEMINI_PROJECT_DIR/.gemini/hooks/security.sh",
                                    "timeout": 5000
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = GeminiConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        assert_eq!(raw["model"]["name"], "gemini-2.5-pro");
        // The user's matcher-scoped group stays untouched; our matcher-less
        // "match all" group is a separate entry.
        let before_tool = raw["hooks"]["BeforeTool"].as_array().unwrap();
        assert_eq!(before_tool.len(), 2, "user group + MVP group");
        let user_group = before_tool
            .iter()
            .find(|g| g.get("matcher").and_then(Value::as_str) == Some("run_shell_command"))
            .expect("user group must survive");
        let handlers = user_group["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1, "no handlers added to the user group");
        assert_eq!(handlers[0]["name"], "security-check");
    }

    #[test]
    fn install_is_idempotent() {
        let path = temp_config("idem", "");
        let mut config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 7);
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 0);

        config.save().unwrap();
        let mut reloaded = GeminiConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_A)), 0);
        assert!(installed(&reloaded));
    }

    #[test]
    fn install_updates_a_stale_binary_path_without_duplicating() {
        let path = temp_config("move", "");
        let mut config = GeminiConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let mut reloaded = GeminiConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_B)), 7);
        reloaded.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let handler = &raw["hooks"]["AfterAgent"][0]["hooks"][0];
        assert_eq!(
            handler["command"],
            "'/usr/local/bin/mvp' hook gemini completed"
        );
        let groups = raw["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn malformed_config_is_reported_and_never_modified() {
        let path = temp_config("malformed", "{ this is not json");
        let original = load_str(&path);
        assert!(GeminiConfig::load(&path).is_err());
        assert_eq!(load_str(&path), original);
    }

    #[test]
    fn non_object_config_is_rejected() {
        let path = temp_config("array", "[1, 2, 3]");
        let err = GeminiConfig::load(&path).unwrap_err();
        assert!(err.contains("JSON object"), "unexpected error: {err}");
    }

    #[test]
    fn uninstall_removes_only_mvp_hooks() {
        let path = temp_config(
            "uninstall",
            r#"{
                "hooks": {
                    "BeforeTool": [
                        {
                            "matcher": "run_shell_command",
                            "hooks": [
                                {
                                    "name": "security-check",
                                    "type": "command",
                                    "command": "/home/user/security.sh"
                                }
                            ]
                        }
                    ],
                    "AfterAgent": [
                        {
                            "hooks": [
                                {
                                    "name": "audit",
                                    "type": "command",
                                    "command": "/home/user/audit.sh"
                                },
                                {
                                    "name": "mvp",
                                    "type": "command",
                                    "command": "'/opt/mvp' hook gemini completed",
                                    "timeout": 2000
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let before_tool = raw["hooks"]["BeforeTool"].as_array().unwrap();
        assert_eq!(before_tool.len(), 1, "unrelated hook must survive");
        let after_agent = raw["hooks"]["AfterAgent"].as_array().unwrap();
        assert_eq!(after_agent[0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(after_agent[0]["hooks"][0]["command"], "/home/user/audit.sh");
    }

    #[test]
    fn uninstall_is_idempotent() {
        let path = temp_config("unidem", "");
        let mut config = GeminiConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        assert_eq!(config.uninstall_hooks(), 7);
        assert_eq!(config.uninstall_hooks(), 0);
        assert_eq!(config.hook_status().0, 0);
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        assert!(
            raw.get("hooks").is_none(),
            "the hooks key must be removed entirely when empty"
        );
    }

    #[test]
    fn uninstall_recognises_a_moved_binary() {
        let path = temp_config(
            "movedbin",
            r#"{
                "hooks": {
                    "SessionEnd": [
                        {
                            "hooks": [
                                {
                                    "name": "mvp",
                                    "type": "command",
                                    "command": "'C:\\Program Files\\MVP\\waitstate.exe' hook gemini stopped"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
    }

    #[test]
    fn similar_but_foreign_commands_are_untouched() {
        let path = temp_config(
            "foreign",
            r#"{
                "hooks": {
                    "AfterAgent": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "'/opt/something-else' hook gemini completed"
                                },
                                {
                                    "type": "command",
                                    "command": "'/opt/mvp' hook codex completed"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 0);
        assert_eq!(config.hook_status().0, 0);
    }

    #[test]
    fn hook_status_reports_partial_installs() {
        let path = temp_config(
            "partial",
            r#"{
                "hooks": {
                    "AfterAgent": [
                        {
                            "hooks": [
                                {
                                    "name": "mvp",
                                    "type": "command",
                                    "command": "'/opt/mvp' hook gemini completed"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let config = GeminiConfig::load(&path).unwrap();
        assert_eq!(config.hook_status(), (1, 7));
    }

    #[test]
    fn save_creates_a_backup_before_modifying() {
        let path = temp_config("backup", r#"{"model": {"name": "x"}}"#);
        let mut config = GeminiConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let dir = path.parent().unwrap();
        let backups: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("settings.json.mvp-backup-"))
            .collect();
        assert_eq!(backups.len(), 1, "exactly one backup expected");
    }
}
