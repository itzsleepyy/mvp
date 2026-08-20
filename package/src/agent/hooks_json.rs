//! Shared machinery for merging MVP hooks into a JSON settings file.
//!
//! Claude Code (`~/.claude/settings.json`), Codex (`~/.codex/hooks.json`)
//! and Gemini CLI (`~/.gemini/settings.json`) all configure hooks with the
//! same outer shape:
//!
//! ```json
//! {
//!   "hooks": {
//!     "EventName": [
//!       {
//!         "matcher": "optional filter",
//!         "hooks": [ { "type": "command", "command": "...", ... } ]
//!       }
//!     ]
//!   }
//! }
//! ```
//!
//! Provider-specific differences (handler fields, ownership detection,
//! command format) stay in each adapter; the destructive-edit-safety lives
//! here once:
//!
//! - the existing configuration is never replaced or reordered destructively
//! - unrelated hooks and settings stay untouched
//! - a backup is written before any modification
//! - an unparseable file is never modified
//! - install and uninstall are idempotent

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::agent::event::AgentEvent;

/// Quotes a value for safe embedding in a POSIX shell command string.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The file name of a shell command's first token, tolerating Windows
/// (`\`) and unix (`/`) separators: "C:\Tools\mvp.exe" becomes "mvp.exe".
pub fn command_binary_name(first_token: &str) -> &str {
    first_token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(first_token)
}

/// One hook the installer wants to own, independent of provider.
#[derive(Debug, Clone, Copy)]
pub struct HookSpec {
    /// Native hook event name, e.g. "SessionStart".
    pub event: &'static str,
    /// Hook matcher; `None` means "all" (matcher key omitted).
    pub matcher: Option<&'static str>,
    /// The agent event the hook command sends.
    pub agent_event: AgentEvent,
}

/// Builds the provider-specific handler value for `binary` sending
/// `agent_event`.
pub type HandlerBuilder = fn(binary: &str, agent_event: AgentEvent) -> Value;

/// True for any MVP-owned handler, whatever event it sends.
pub type OwnedCheck = fn(handler: &Value) -> bool;

/// True for a MVP-owned handler sending `agent_event`.
pub type HandlerMatch = fn(handler: &Value, agent_event: AgentEvent) -> bool;

/// A parsed JSON hooks file, loaded for merging.
#[derive(Debug)]
pub struct HooksJson {
    path: PathBuf,
    data: Value,
}

impl HooksJson {
    /// Loads the file. A missing file yields an empty object (the file will
    /// be created on save). A malformed file is an error and is never
    /// modified.
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

    /// Merges the hook set into the file. `binary` is the display form of
    ///    the mvp executable embedded into each handler. Returns the
    /// number of changes made (0 when already installed and up to date).
    /// Existing hooks and all other settings are preserved.
    pub fn install(
        &mut self,
        specs: &[HookSpec],
        binary: &str,
        build: HandlerBuilder,
        matches: HandlerMatch,
    ) -> usize {
        let mut changes = 0;
        for spec in specs {
            if self.merge_entry(spec, binary, build, matches) {
                changes += 1;
            }
        }
        changes
    }

    /// Removes every handler `owned` claims. Returns the number removed.
    pub fn uninstall(&mut self, owned: OwnedCheck) -> usize {
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
                removed += handlers.len() - handlers.iter().filter(|h| !owned(h)).count();
                handlers.retain(|h| !owned(h));
            }
            // Drop now-empty matcher groups so we leave no residue.
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| !handlers.is_empty())
            });
        }
        // Drop now-empty event arrays.
        hooks.retain(|_, groups| groups.as_array().is_some_and(|g| !g.is_empty()));
        // Drop the whole "hooks" key when nothing remains, restoring the
        // config to exactly its pre-install shape.
        if hooks.is_empty() {
            self.data
                .as_object_mut()
                .expect("config is an object")
                .remove("hooks");
        }
        removed
    }

    /// How many of the hook specs have an up-to-date MVP handler.
    pub fn status(&self, specs: &[HookSpec], matches: HandlerMatch) -> (usize, usize) {
        let installed = specs
            .iter()
            .filter(|spec| {
                self.find_handler(spec.event, spec.agent_event, matches)
                    .is_some()
            })
            .count();
        (installed, specs.len())
    }

    /// Writes the file back, creating a timestamped backup of the previous
    /// file first. The write is atomic (temp file + rename).
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
            let file_name = self
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("config");
            let backup = self
                .path
                .with_file_name(format!("{file_name}.mvp-backup-{stamp}"));
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
        let file_name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");
        let tmp = self.path.with_file_name(format!("{file_name}.mvp-tmp"));
        std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("cannot write {}: {e}", self.path.display()))?;
        Ok(())
    }

    /// Merges one hook entry: updates an existing MVP handler (stale
    /// binary path), or appends a handler to a matcher-compatible group, or
    /// creates a fresh group. Returns true when the file changed.
    fn merge_entry(
        &mut self,
        spec: &HookSpec,
        binary: &str,
        build: HandlerBuilder,
        matches: HandlerMatch,
    ) -> bool {
        if let Some(handler) = self.find_handler_mut(spec.event, spec.agent_event, matches) {
            let fresh = build(binary, spec.agent_event);
            if *handler == fresh {
                return false; // already installed and up to date
            }
            *handler = fresh;
            return true; // stale handler (moved binary, changed shape) updated
        }

        let handler = build(binary, spec.agent_event);
        let groups = self.ensure_event_groups(spec.event);
        for group in groups.iter_mut() {
            let group_matcher = group.get("matcher").and_then(Value::as_str);
            if group_matcher == spec.matcher {
                group["hooks"]
                    .as_array_mut()
                    .expect("groups carry hooks arrays")
                    .push(handler);
                return true;
            }
        }
        let mut group = json!({ "hooks": [handler] });
        if let Some(m) = spec.matcher {
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

    fn find_handler(
        &self,
        event: &str,
        agent_event: AgentEvent,
        matches: HandlerMatch,
    ) -> Option<&Value> {
        let groups = self.data.get("hooks")?.get(event)?.as_array()?;
        groups.iter().find_map(|group| {
            group
                .get("hooks")?
                .as_array()?
                .iter()
                .find(|h| matches(h, agent_event))
        })
    }

    fn find_handler_mut(
        &mut self,
        event: &str,
        agent_event: AgentEvent,
        matches: HandlerMatch,
    ) -> Option<&mut Value> {
        let groups = self.data.get_mut("hooks")?.get_mut(event)?.as_array_mut()?;
        groups.iter_mut().find_map(|group| {
            group
                .get_mut("hooks")?
                .as_array_mut()?
                .iter_mut()
                .find(|h| matches(h, agent_event))
        })
    }
}
