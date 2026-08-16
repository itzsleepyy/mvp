//! Lightweight agent session state: which agent is connected and what it is
//! doing right now. Deliberately minimal — no historical session tracking yet.

use crate::agent::event::AgentEvent;

/// The coding agent WaitState is listening to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    ClaudeCode,
}

impl AgentKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
        }
    }
}

impl std::str::FromStr for AgentKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Ok(Self::ClaudeCode),
            other => Err(format!("unknown agent {other:?}")),
        }
    }
}

/// What the agent is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// No agent has ever reported in.
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
}

/// Everything WaitState knows about the connected agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentState {
    pub agent: AgentKind,
    pub status: AgentStatus,
    pub last_event: Option<AgentEvent>,
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            agent: AgentKind::ClaudeCode,
            status: AgentStatus::Disconnected,
            last_event: None,
        }
    }
}

impl AgentState {
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
        assert_eq!(state.agent, AgentKind::ClaudeCode);
        assert_eq!(state.status, AgentStatus::Disconnected);
        assert_eq!(state.last_event, None);
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
    fn agent_kind_parses_claude_aliases() {
        assert_eq!("claude".parse::<AgentKind>(), Ok(AgentKind::ClaudeCode));
        assert_eq!(
            "claude-code".parse::<AgentKind>(),
            Ok(AgentKind::ClaudeCode)
        );
        assert!("codex".parse::<AgentKind>().is_err());
        assert_eq!(AgentKind::ClaudeCode.name(), "Claude Code");
    }
}
