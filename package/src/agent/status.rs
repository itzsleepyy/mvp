//! Lightweight per-agent session state: which agents are connected and what
//! they are doing right now. Deliberately minimal — no historical session
//! tracking yet.
//!
//! Multiple agents can be active at once (Claude and Codex working in
//! parallel), so the application keeps one [`AgentState`] per [`AgentKind`]
//! and derives an aggregate status from the whole set. Attention precedence:
//! `NeedsInput` > `Working` > `Completed` > `Stopped` > `Idle`.

use crate::agent::event::AgentEvent;

/// The coding agents MVP can listen to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    GeminiCli,
    OpenCode,
}

impl AgentKind {
    /// Human display name, e.g. "Claude Code".
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::GeminiCli => "Gemini CLI",
            Self::OpenCode => "OpenCode",
        }
    }

    /// Uppercase name for pause overlays, e.g. "CODEX NEEDS YOU".
    pub fn upper_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "CLAUDE",
            Self::Codex => "CODEX",
            Self::GeminiCli => "GEMINI",
            Self::OpenCode => "OPENCODE",
        }
    }

    /// Canonical CLI/IPC identifier, e.g. "codex". Frozen on the wire.
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::GeminiCli => "gemini",
            Self::OpenCode => "opencode",
        }
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl std::str::FromStr for AgentKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "gemini" | "gemini-cli" => Ok(Self::GeminiCli),
            "opencode" | "open-code" => Ok(Self::OpenCode),
            other => Err(format!("unknown agent {other:?}")),
        }
    }
}

/// How the UI picks which agent to display when no events have arrived yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDisplay {
    /// Show whichever agent most recently emitted a lifecycle event.
    Auto,
    /// Always show this agent (announced as idle until events arrive).
    Specific(AgentKind),
}

/// What one agent is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// This agent has never reported in.
    Disconnected,
    /// Session open, waiting for the developer's prompt.
    Idle,
    Working,
    NeedsInput,
    Completed,
    Stopped,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Idle => "Idle",
            Self::Working => "Working",
            Self::NeedsInput => "Needs input",
            Self::Completed => "Finished",
            Self::Stopped => "Ended",
        }
    }

    /// Attention weight used to derive the aggregate status. A higher
    /// weight wins.
    pub fn attention_weight(self) -> u8 {
        match self {
            Self::NeedsInput => 5,
            Self::Working => 4,
            Self::Completed => 3,
            Self::Stopped => 2,
            Self::Idle => 1,
            Self::Disconnected => 0,
        }
    }
}

/// Everything MVP knows about one connected agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentState {
    pub status: AgentStatus,
    pub last_event: Option<AgentEvent>,
    /// Monotonic sequence of the last event from this agent; the largest
    /// value marks the most recently active agent.
    pub activity: u64,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            status: AgentStatus::Disconnected,
            last_event: None,
            activity: 0,
        }
    }

    /// Applies one lifecycle event to the status. Idempotent by design:
    /// duplicates simply re-assert the same status.
    pub fn apply(&mut self, event: AgentEvent) {
        self.last_event = Some(event);
        self.status = match event {
            AgentEvent::Started => AgentStatus::Idle,
            AgentEvent::Working => AgentStatus::Working,
            AgentEvent::NeedsInput => AgentStatus::NeedsInput,
            AgentEvent::Completed => AgentStatus::Completed,
            AgentEvent::Stopped => AgentStatus::Stopped,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_disconnected() {
        let state = AgentState::default();
        assert_eq!(state.status, AgentStatus::Disconnected);
        assert_eq!(state.last_event, None);
        assert_eq!(state.activity, 0);
    }

    #[test]
    fn events_update_status_and_last_event() {
        let mut state = AgentState::default();
        let sequence = [
            (AgentEvent::Started, AgentStatus::Idle),
            (AgentEvent::Working, AgentStatus::Working),
            (AgentEvent::NeedsInput, AgentStatus::NeedsInput),
            (AgentEvent::Working, AgentStatus::Working),
            (AgentEvent::Completed, AgentStatus::Completed),
            (AgentEvent::Stopped, AgentStatus::Stopped),
        ];
        for (event, status) in sequence {
            state.apply(event);
            assert_eq!(state.status, status);
            assert_eq!(state.last_event, Some(event));
        }
    }

    #[test]
    fn duplicate_events_are_stable() {
        let mut state = AgentState::default();
        state.apply(AgentEvent::Working);
        state.apply(AgentEvent::Working);
        assert_eq!(state.status, AgentStatus::Working);
        state.apply(AgentEvent::NeedsInput);
        state.apply(AgentEvent::NeedsInput);
        assert_eq!(state.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn agent_kind_parses_all_ids_and_aliases() {
        assert_eq!("claude".parse::<AgentKind>(), Ok(AgentKind::ClaudeCode));
        assert_eq!(
            "claude-code".parse::<AgentKind>(),
            Ok(AgentKind::ClaudeCode)
        );
        assert_eq!("codex".parse::<AgentKind>(), Ok(AgentKind::Codex));
        assert_eq!("gemini".parse::<AgentKind>(), Ok(AgentKind::GeminiCli));
        assert_eq!("gemini-cli".parse::<AgentKind>(), Ok(AgentKind::GeminiCli));
        assert_eq!("opencode".parse::<AgentKind>(), Ok(AgentKind::OpenCode));
        assert_eq!("open-code".parse::<AgentKind>(), Ok(AgentKind::OpenCode));
        assert!("warp".parse::<AgentKind>().is_err());
    }

    #[test]
    fn display_names_are_stable() {
        assert_eq!(AgentKind::ClaudeCode.name(), "Claude Code");
        assert_eq!(AgentKind::Codex.name(), "Codex");
        assert_eq!(AgentKind::GeminiCli.name(), "Gemini CLI");
        assert_eq!(AgentKind::OpenCode.name(), "OpenCode");
        assert_eq!(AgentKind::ClaudeCode.upper_name(), "CLAUDE");
        assert_eq!(AgentKind::Codex.upper_name(), "CODEX");
        assert_eq!(AgentKind::GeminiCli.upper_name(), "GEMINI");
        assert_eq!(AgentKind::OpenCode.upper_name(), "OPENCODE");
    }

    #[test]
    fn wire_ids_are_stable() {
        assert_eq!(AgentKind::ClaudeCode.id(), "claude");
        assert_eq!(AgentKind::Codex.id(), "codex");
        assert_eq!(AgentKind::GeminiCli.id(), "gemini");
        assert_eq!(AgentKind::OpenCode.id(), "opencode");
    }

    #[test]
    fn attention_weights_follow_the_documented_precedence() {
        assert!(
            AgentStatus::NeedsInput.attention_weight() > AgentStatus::Working.attention_weight()
        );
        assert!(
            AgentStatus::Working.attention_weight() > AgentStatus::Completed.attention_weight()
        );
        assert!(
            AgentStatus::Completed.attention_weight() > AgentStatus::Stopped.attention_weight()
        );
        assert!(AgentStatus::Stopped.attention_weight() > AgentStatus::Idle.attention_weight());
        assert!(
            AgentStatus::Idle.attention_weight() > AgentStatus::Disconnected.attention_weight()
        );
    }
}
