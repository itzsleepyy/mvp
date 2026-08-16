//! Generic coding-agent lifecycle abstraction.
//!
//! Every agent integration (Claude Code, Codex, Gemini CLI, OpenCode, ...)
//! normalizes its lifecycle into [`AgentEvent`]. The game application knows
//! nothing about individual agents — provider-specific details stay inside
//! each adapter module (`claude`, `codex`, `gemini`, `opencode`).
//!
//! Privacy principle: adapters receive lifecycle events only. Managed Codex
//! mode relays opaque terminal bytes in memory but never interprets, logs,
//! persists or transmits them.

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
