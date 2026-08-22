//! Safe merging of MVP hooks into the Codex user hooks file.
//!
//! Codex discovers hooks next to its config layers; the dedicated
//! `~/.codex/hooks.json` file is chosen here because merging into it never
//! touches the user's `config.toml`. Codex runs hooks from *all* sources,
//! so a dedicated file is fully equivalent.
//!
//! Codex specifics:
//! - handler commands are shell strings (Codex has no exec-arg form), so
//!   the binary path is shell-quoted;
//! - every handler is `async` and returns `{}` on stdout (the `hook`
//!   bridge), which never blocks, approves or denies anything;
//! - Codex requires non-managed hooks to be reviewed and trusted via
//!   `/hooks` before they run the first time;
//! - plugin packaging later can ship the same table via
//!   `hooks/hooks.json` inside a plugin root or a `.codex-plugin/plugin.json`
//!   manifest `hooks` entry — no lifecycle logic changes needed.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::agent::codex::HOOKS;
use crate::agent::event::AgentEvent;
use crate::agent::hooks_json::{HookSpec, HooksJson, command_binary_name, shell_quote};

/// The user-level Codex hooks file, honouring `CODEX_HOME`.
pub fn config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(dir).join("hooks.json");
    }
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_default();
    home.join(".codex").join("hooks.json")
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

/// A parsed Codex hooks file, loaded for merging.
#[derive(Debug)]
pub struct CodexConfig {
    inner: HooksJson,
}

impl CodexConfig {
    /// Loads the hooks file. A missing file yields an empty object (the
    /// file will be created on save). A malformed file is an error and is
    /// never modified.
    pub fn load(path: &Path) -> Result<Self, String> {
        Ok(Self {
            inner: HooksJson::load(path)?,
        })
    }

    /// Merges the MVP hook set into the file. Returns the number of
    /// changes made (0 when already installed). Existing hooks and all
    /// other content are preserved.
    pub fn install_hooks(&mut self, binary: &Path) -> usize {
        self.inner.install(
            &hook_specs(),
            &binary.display().to_string(),
            codex_handler,
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

    /// Writes the file back, creating a timestamped backup of the previous
    /// file first. The write is atomic (temp file + rename).
    pub fn save(&self) -> Result<(), String> {
        self.inner.save()
    }
}

/// The Codex handler: a shell command string calling the `hook` bridge,
/// which sends the event and prints `{}` (valid, decision-free JSON).
///
/// Deliberately synchronous: Codex `async` hooks were verified not to run
/// in `codex exec` sessions (the session ends before background hooks get
/// to run). The bridge is one bounded local TCP connect, so the agent loop
/// is never held up noticeably.
fn codex_handler(binary: &str, event: AgentEvent) -> Value {
    let command = format!("{} hook codex {}", shell_quote(binary), event.as_str());
    json!({
        "type": "command",
        "command": command,
        "timeout": 2,
    })
}

/// True for any MVP hook handler in a Codex file, whatever event it
/// sends. Ownership is detected by the executable name (`mvp`, plus the
/// pre-rebrand `waitstate` so old hooks can still be upgraded or removed)
/// and the `hook codex` bridge arguments, so a moved binary is still
/// recognised.
fn is_mvp(handler: &Value) -> bool {
    let Some(command) = handler.get("command").and_then(Value::as_str) else {
        return false;
    };
    let binary_name = command_binary_name(first_token(command));
    crate::agent::owned_binary_name(binary_name) && command.contains("hook codex")
}

/// True for a MVP hook handler sending `event`.
fn is_mvp_for(handler: &Value, event: AgentEvent) -> bool {
    is_mvp(handler)
        && handler
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|c| c.contains(&format!("hook codex {}", event.as_str())))
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
            "mvp_codex_test_{}_{}/hooks.json",
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

    fn installed(config: &CodexConfig) -> bool {
        let (installed, total) = config.hook_status();
        installed == total && total > 0
    }

    #[test]
    fn install_creates_hooks_in_an_empty_config() {
        let path = temp_config("empty", "");
        let mut config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 6);
        assert!(installed(&config));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let hooks = raw["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 6);
        let handler = &hooks["PermissionRequest"][0]["hooks"][0];
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["command"], "'/opt/mvp' hook codex needs-input");
        assert_eq!(handler["timeout"], 2);
        // Verified against real Codex: async hooks never run in exec
        // sessions, so the handler must stay synchronous.
        assert!(handler.get("async").is_none());
        // Stop expects JSON on stdout when it exits 0: the bridge prints {}.
        let stop = &hooks["Stop"][0]["hooks"][0];
        assert_eq!(stop["command"], "'/opt/mvp' hook codex completed");
    }

    #[test]
    fn install_preserves_existing_hooks_and_metadata() {
        let path = temp_config(
            "existing",
            r#"{
                "description": "my hooks",
                "hooks": {
                    "PreToolUse": [
                        {
                            "matcher": "Bash",
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "python3 ~/.codex/hooks/check.py",
                                    "statusMessage": "Checking Bash command"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = CodexConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        assert_eq!(raw["description"], "my hooks");
        let pretooluse = raw["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pretooluse.len(), 1, "no duplicate groups for PreToolUse");
        assert_eq!(
            pretooluse[0]["hooks"][0]["command"],
            "python3 ~/.codex/hooks/check.py"
        );
        assert_eq!(pretooluse[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn install_is_idempotent() {
        let path = temp_config("idem", "");
        let mut config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 6);
        assert_eq!(config.install_hooks(Path::new(BINARY_A)), 0);

        config.save().unwrap();
        let mut reloaded = CodexConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_A)), 0);
        assert!(installed(&reloaded));
    }

    #[test]
    fn install_updates_a_stale_binary_path_without_duplicating() {
        let path = temp_config("move", "");
        let mut config = CodexConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let mut reloaded = CodexConfig::load(&path).unwrap();
        assert_eq!(reloaded.install_hooks(Path::new(BINARY_B)), 6);
        reloaded.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let handler = &raw["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(
            handler["command"],
            "'/usr/local/bin/mvp' hook codex completed"
        );
        let session_start = raw["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start[0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn install_updates_handlers_whose_shape_changed() {
        // Regression: the up-to-date check must compare the whole handler,
        // so a hook installed by an older MVP (e.g. with async) is
        // upgraded, not silently kept.
        let path = temp_config("shape", "");
        let mut config = CodexConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let mut raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let handlers = raw["hooks"]["SessionStart"][0]["hooks"]
            .as_array_mut()
            .unwrap();
        for handler in handlers {
            handler["async"] = Value::Bool(true); // old MVP shape
        }
        std::fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();

        let mut reloaded = CodexConfig::load(&path).unwrap();
        assert_eq!(
            reloaded.install_hooks(Path::new(BINARY_A)),
            1,
            "only the reshaped event must be rewritten"
        );
        reloaded.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let handler = &raw["hooks"]["SessionStart"][0]["hooks"][0];
        assert!(
            handler.get("async").is_none(),
            "stale async shape must be replaced"
        );
    }

    #[test]
    fn malformed_config_is_reported_and_never_modified() {
        let path = temp_config("malformed", "{ this is not json");
        let original = load_str(&path);
        assert!(CodexConfig::load(&path).is_err());
        assert_eq!(load_str(&path), original);
    }

    #[test]
    fn non_object_config_is_rejected() {
        let path = temp_config("array", "[1, 2, 3]");
        let err = CodexConfig::load(&path).unwrap_err();
        assert!(err.contains("JSON object"), "unexpected error: {err}");
    }

    #[test]
    fn uninstall_removes_only_mvp_hooks() {
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
                                    "command": "python3 ~/.codex/hooks/check.py"
                                }
                            ]
                        }
                    ],
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "python3 ~/.codex/hooks/notify.py"
                                },
                                {
                                    "type": "command",
                                    "command": "'/opt/mvp' hook codex completed",
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
        config.save().unwrap();

        let raw: Value = serde_json::from_str(&load_str(&path)).unwrap();
        let pretooluse = raw["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pretooluse.len(), 1, "unrelated hook must survive");
        let stop = raw["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(
            stop[0]["hooks"][0]["command"],
            "python3 ~/.codex/hooks/notify.py"
        );
    }

    #[test]
    fn uninstall_is_idempotent() {
        let path = temp_config("unidem", "");
        let mut config = CodexConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        assert_eq!(config.uninstall_hooks(), 6);
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
    fn uninstall_recognises_a_moved_binary_and_hand_edited_quotes() {
        let path = temp_config(
            "movedbin",
            r#"{
                "hooks": {
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "'C:\\Program Files\\MVP\\waitstate.exe' hook codex completed",
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 1);
    }

    #[test]
    fn similar_but_foreign_commands_are_untouched() {
        let path = temp_config(
            "foreign",
            r#"{
                "hooks": {
                    "Stop": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "'/opt/something-else' hook codex completed"
                                },
                                {
                                    "type": "command",
                                    "command": "'/opt/mvp' hook gemini completed"
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.uninstall_hooks(), 0);
        assert_eq!(config.hook_status().0, 0);
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
                                    "command": "'/opt/mvp' hook codex completed",
                                    "async": true
                                }
                            ]
                        }
                    ]
                }
            }"#,
        );
        let config = CodexConfig::load(&path).unwrap();
        assert_eq!(config.hook_status(), (1, 6));
    }

    #[test]
    fn save_creates_a_backup_before_modifying() {
        let path = temp_config("backup", r#"{"description": "mine"}"#);
        let mut config = CodexConfig::load(&path).unwrap();
        config.install_hooks(Path::new(BINARY_A));
        config.save().unwrap();

        let dir = path.parent().unwrap();
        let backups: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("hooks.json.mvp-backup-"))
            .collect();
        assert_eq!(backups.len(), 1, "exactly one backup expected");
    }

    #[test]
    fn config_path_honours_codex_home() {
        let original = std::env::var_os("CODEX_HOME");
        // SAFETY: this test owns the process env; the guard restores the
        // original value before any other code runs.
        unsafe {
            std::env::set_var("CODEX_HOME", "/tmp/codex-test-config");
        }
        let path = config_path();
        assert_eq!(path, PathBuf::from("/tmp/codex-test-config/hooks.json"));
        match original {
            Some(v) => unsafe { std::env::set_var("CODEX_HOME", v) },
            None => unsafe { std::env::remove_var("CODEX_HOME") },
        }
    }

    #[test]
    fn shell_quoting_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/opt/mvp"), "'/opt/mvp'");
        assert_eq!(shell_quote("it's here"), "'it'\\''s here'");
        assert_eq!(first_token("'/opt/mvp' hook codex working"), "/opt/mvp");
        assert_eq!(first_token("/opt/mvp hook codex working"), "/opt/mvp");
        assert_eq!(first_token("  \"/x y/z\" a"), "/x y/z");
    }
}
