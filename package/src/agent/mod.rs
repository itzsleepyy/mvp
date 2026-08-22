//! Generic coding-agent lifecycle abstraction.
//!
//! Every agent integration (Claude Code, Codex, Gemini CLI, OpenCode, ...)
//! normalizes its lifecycle into [`AgentEvent`]. The game application knows
//! nothing about individual agents — provider-specific details stay inside
//! each adapter module (`claude`, `codex`, `gemini`, `opencode`).
//!
//! Privacy principle: MVP receives lifecycle events only. It never
//! sees prompts, tool inputs, source code or agent output.

pub mod claude;
pub mod codex;
pub mod event;
pub mod gemini;
pub mod hooks_json;
pub mod integrations;
pub mod opencode;
pub mod status;

pub use event::AgentEvent;
pub use status::{AgentDisplay, AgentKind, AgentState, AgentStatus};

/// True when an executable name is one MVP owns hooks with. Accepts the
/// pre-rebrand `waitstate` names too, so hooks installed by the old binary
/// are still recognized (and upgraded or removed) cleanly.
pub fn owned_binary_name(name: &str) -> bool {
    matches!(name, "mvp" | "mvp.exe" | "waitstate" | "waitstate.exe")
}
