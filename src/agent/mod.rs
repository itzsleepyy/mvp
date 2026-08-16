//! Generic coding-agent lifecycle abstraction.
//!
//! Every agent integration (Claude Code, Codex, Gemini CLI, OpenCode, ...)
//! normalizes its lifecycle into [`AgentEvent`]. The game application knows
//! nothing about individual agents — Claude-specific details stay inside
//! [`crate::agent::claude`].
//!
//! Privacy principle: WaitState receives lifecycle events only. It never
//! sees prompts, tool inputs, source code or agent output.

pub mod claude;
pub mod event;
pub mod status;

pub use event::AgentEvent;
pub use status::{AgentKind, AgentState, AgentStatus};
