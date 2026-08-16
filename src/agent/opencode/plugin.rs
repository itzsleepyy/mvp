//! Generation and version tracking of the WaitState OpenCode plugin file.
//!
//! OpenCode loads JavaScript plugins from `~/.config/opencode/plugins/`
//! automatically; no config edit is needed. The installer writes a single
//! WaitState-owned `waitstate.js` and never touches any other file.
//!
//! Version awareness: the first line carries a `WAITSTATE_OPENCODE_PLUGIN`
//! version marker, so `status` can distinguish current, outdated and
//! foreign files.

use std::path::{Path, PathBuf};

/// The plugin format version. Bump when the mapping or the bridge contract
/// changes; `install` upgrades older files, `status` reports them.
pub const PLUGIN_VERSION: u32 = 1;

/// The first-line marker identifying a WaitState-owned plugin file.
pub const MARKER_PREFIX: &str = "// WAITSTATE_OPENCODE_PLUGIN v";

/// The plugin file name inside the OpenCode plugin directory.
pub const PLUGIN_FILE_NAME: &str = "waitstate.js";

/// The OpenCode plugin directory: `$XDG_CONFIG_HOME/opencode/plugins` or
/// `~/.config/opencode/plugins`. OpenCode documents `~/.config` as its
/// global config root on all platforms.
pub fn plugin_dir() -> PathBuf {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".config")))
        .unwrap_or_else(|| PathBuf::from("~/.config"));
    root.join("opencode").join("plugins")
}

/// The plugin file path.
#[cfg(test)]
pub fn config_path() -> PathBuf {
    plugin_dir().join(PLUGIN_FILE_NAME)
}

/// Escapes a path for embedding in a double-quoted JavaScript string.
fn js_escape(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The plugin source for a given waitstate binary path. The plugin is
/// deliberately tiny: lifecycle mapping plus a fire-and-forget spawn. It
/// contains no game logic and never reads prompt/code/tool data.
pub fn plugin_source(binary: &Path) -> String {
    format!(
        r#"// WAITSTATE_OPENCODE_PLUGIN v{version}
// WaitState lifecycle bridge. Observational only: forwards lifecycle events
// to the local WaitState instance. Never reads or transmits prompts, code,
// tool arguments or model output.
const BINARY = "{binary}";

function send(event) {{
  try {{
    Bun.spawn([BINARY, "agent-event", "opencode", event], {{
      stdout: "null",
      stderr: "null",
      stdin: "null",
    }});
  }} catch (_) {{
    // A missing WaitState must never disturb OpenCode.
  }}
}}

export const WaitState = async () => {{
  return {{
    event: async ({{ event }}) => {{
      switch (event && event.type) {{
        case "session.created":
          send("started");
          break;
        case "session.status": {{
          const status = event.properties && event.properties.status;
          if (status && status.type === "busy") send("working");
          else if (status && status.type === "idle") send("completed");
          break;
        }}
        case "session.idle":
          send("completed");
          break;
        case "permission.replied":
          send("working");
          break;
        case "session.deleted":
          send("stopped");
          break;
      }}
    }},
    "permission.ask": async () => {{
      // Observational: the default "ask" decision is preserved.
      send("needs-input");
    }},
    "tool.execute.before": async () => {{
      send("working");
    }},
    "tool.execute.after": async () => {{
      send("working");
    }},
  }};
}};
"#,
        version = PLUGIN_VERSION,
        binary = js_escape(&binary.display().to_string()),
    )
}

/// The plugin version embedded in a file, or `None` for foreign files.
pub fn installed_version(content: &str) -> Option<u32> {
    let first_line = content.lines().next()?;
    let version = first_line.strip_prefix(MARKER_PREFIX)?;
    version.trim().parse().ok()
}

/// True when the file content belongs to WaitState.
pub fn is_waitstate_owned(content: &str) -> bool {
    installed_version(content).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_binary() -> PathBuf {
        PathBuf::from("/opt/waitstate")
    }

    #[test]
    fn source_carries_the_version_marker() {
        let source = plugin_source(&fake_binary());
        let first_line = source.lines().next().unwrap();
        assert_eq!(
            first_line,
            format!("// WAITSTATE_OPENCODE_PLUGIN v{PLUGIN_VERSION}")
        );
    }

    #[test]
    fn source_embeds_the_binary_and_the_bridge_command() {
        let source = plugin_source(&fake_binary());
        assert!(source.contains("const BINARY = \"/opt/waitstate\";"));
        assert!(source.contains("\"agent-event\", \"opencode\", event"));
    }

    #[test]
    fn source_maps_all_five_generic_events() {
        let source = plugin_source(&fake_binary());
        for event in ["started", "working", "needs-input", "completed", "stopped"] {
            assert!(
                source.contains(&format!("send(\"{event}\")")),
                "missing mapping for {event}"
            );
        }
    }

    #[test]
    fn source_escapes_windows_paths_and_quotes() {
        let binary = PathBuf::from("C:\\Program Files\\WaitState\\waitstate.exe");
        let source = plugin_source(&binary);
        assert!(source.contains("C:\\\\Program Files\\\\WaitState\\\\waitstate.exe"));
        assert!(!source.contains("C:\\Program"));
    }

    #[test]
    fn version_parsing_distinguishes_ours_from_foreign_files() {
        let ours = format!("// WAITSTATE_OPENCODE_PLUGIN v{}\nrest", PLUGIN_VERSION);
        assert_eq!(installed_version(&ours), Some(PLUGIN_VERSION));
        assert!(is_waitstate_owned(&ours));

        assert_eq!(
            installed_version("// WAITSTATE_OPENCODE_PLUGIN v0\n"),
            Some(0)
        );
        assert_eq!(installed_version("// WAITSTATE_OPENCODE_PLUGIN v\n"), None);
        assert_eq!(installed_version("// WAITSTATE_OPENCODE_PLUGIN 1\n"), None);
        assert_eq!(installed_version("// something else\n"), None);
        assert_eq!(installed_version(""), None);
        assert!(!is_waitstate_owned("export const Mine = async () => ({})"));
    }

    #[test]
    fn plugin_dir_honours_xdg_config_home() {
        let original = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: this test owns the process env; the guard restores the
        // original value before any other code runs.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/opencode-test-config");
        }
        assert_eq!(
            config_path(),
            PathBuf::from("/tmp/opencode-test-config/opencode/plugins/waitstate.js")
        );
        match original {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
    }
}
