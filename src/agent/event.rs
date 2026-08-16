//! Agent-agnostic lifecycle events.
//!
//! Every coding-agent integration maps its lifecycle into these events, so
//! the application state machine never has to know which agent sent them.

use std::fmt;
use std::str::FromStr;

/// One lifecycle event from a coding agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEvent {
    /// The agent started a session (or resumed one).
    Started,
    /// The agent is actively working (processing a prompt, calling tools).
    Working,
    /// The agent needs the developer (permission prompt, input request, ...).
    NeedsInput,
    /// The agent finished responding.
    Completed,
    /// The agent session ended.
    Stopped,
}

impl AgentEvent {
    /// The canonical wire name, used by the IPC protocol and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Working => "working",
            Self::NeedsInput => "needs-input",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
        }
    }

    /// Parses a wire name. Unknown names yield `None` so untrusted IPC
    /// input can be rejected without error handling fan-out.
    pub fn from_wire(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

impl fmt::Display for AgentEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a string is not a known agent event name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEventError(pub String);

impl fmt::Display for UnknownEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown agent event {:?}", self.0)
    }
}

impl std::error::Error for UnknownEventError {}

impl FromStr for AgentEvent {
    type Err = UnknownEventError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "started" => Ok(Self::Started),
            "working" => Ok(Self::Working),
            "needs-input" => Ok(Self::NeedsInput),
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            other => Err(UnknownEventError(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_roundtrips_through_its_wire_name() {
        for event in [
            AgentEvent::Started,
            AgentEvent::Working,
            AgentEvent::NeedsInput,
            AgentEvent::Completed,
            AgentEvent::Stopped,
        ] {
            assert_eq!(AgentEvent::from_wire(event.as_str()), Some(event));
            assert_eq!(event.to_string().parse::<AgentEvent>(), Ok(event));
        }
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(AgentEvent::from_wire(""), None);
        assert_eq!(AgentEvent::from_wire("thinking"), None);
        assert_eq!(AgentEvent::from_wire("Working"), None);
        assert_eq!(AgentEvent::from_wire("needs_input"), None);
        assert_eq!(AgentEvent::from_wire("{\"event\":1}"), None);
        assert!("unknown".parse::<AgentEvent>().is_err());
    }

    #[test]
    fn unknown_event_error_displays_the_input() {
        let err: UnknownEventError = "bogus".parse::<AgentEvent>().unwrap_err();
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn event_names_are_stable() {
        // The wire format is part of the hook contract; keep it frozen.
        assert_eq!(AgentEvent::Started.as_str(), "started");
        assert_eq!(AgentEvent::Working.as_str(), "working");
        assert_eq!(AgentEvent::NeedsInput.as_str(), "needs-input");
        assert_eq!(AgentEvent::Completed.as_str(), "completed");
        assert_eq!(AgentEvent::Stopped.as_str(), "stopped");
    }
}
