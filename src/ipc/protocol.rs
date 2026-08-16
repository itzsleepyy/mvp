//! The tiny structured protocol spoken between the IPC client and server:
//! one JSON line with a version, the per-run token, an optional agent kind
//! and an event name.
//!
//! Protocol history:
//! - v1: `{"version":1,"token":...,"event":...}` — no agent field; the
//!   server assumes Claude Code (only agent that existed then). Old
//!   installed Claude hooks still send this shape.
//! - v2: `{"version":2,"token":...,"agent":...,"event":...}` — explicit
//!   agent identity. New clients always send v2; the server accepts both.

use serde::{Deserialize, Serialize};

use crate::agent::event::AgentEvent;
use crate::agent::status::AgentKind;

/// Current protocol version. The server accepts v1 (assumed Claude) and v2
/// (explicit agent); anything else is rejected.
pub const PROTOCOL_VERSION: u8 = 2;

/// Hard cap on a message line. Anything longer is dropped before parsing.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// One message from a client to the server. `event` is kept as a string so
/// unknown events deserialize and can be rejected by validation instead of
/// failing parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessage {
    pub version: u8,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub event: String,
}

impl AgentMessage {
    pub fn new(token: &str, kind: AgentKind, event: AgentEvent) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            token: token.to_string(),
            agent: Some(kind.id().to_string()),
            event: event.as_str().to_string(),
        }
    }

    /// Validates the message against the expected token and returns the
    /// parsed event together with the originating agent. `Err` covers every
    /// way a hostile/broken message can look; callers simply drop the
    /// connection.
    pub fn validate(&self, expected_token: &str) -> Result<(AgentKind, AgentEvent), &'static str> {
        if self.version != 1 && self.version != PROTOCOL_VERSION {
            return Err("unsupported protocol version");
        }
        if self.token != expected_token {
            return Err("token mismatch");
        }
        let kind = match &self.agent {
            // v1 messages carry no agent: they come from Claude hooks
            // installed before multi-agent support existed.
            None => AgentKind::ClaudeCode,
            Some(name) => name.parse().map_err(|_| "unknown agent")?,
        };
        let event = AgentEvent::from_wire(&self.event).ok_or("unknown event")?;
        Ok((kind, event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_roundtrips_through_json_with_agent() {
        let msg = AgentMessage::new("tok123", AgentKind::Codex, AgentEvent::NeedsInput);
        let wire = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&wire).unwrap();
        assert_eq!(parsed, msg);
        assert_eq!(
            parsed.validate("tok123"),
            Ok((AgentKind::Codex, AgentEvent::NeedsInput))
        );
    }

    #[test]
    fn v1_messages_without_agent_mean_claude() {
        let msg = AgentMessage {
            version: 1,
            token: "tok".into(),
            agent: None,
            event: "working".into(),
        };
        assert_eq!(
            msg.validate("tok"),
            Ok((AgentKind::ClaudeCode, AgentEvent::Working))
        );
    }

    #[test]
    fn all_agent_kinds_roundtrip() {
        for (kind, id) in [
            (AgentKind::ClaudeCode, "claude"),
            (AgentKind::Codex, "codex"),
            (AgentKind::GeminiCli, "gemini"),
            (AgentKind::OpenCode, "opencode"),
        ] {
            let msg = AgentMessage::new("tok", kind, AgentEvent::Started);
            assert_eq!(msg.agent.as_deref(), Some(id));
            let wire = serde_json::to_string(&msg).unwrap();
            let parsed: AgentMessage = serde_json::from_str(&wire).unwrap();
            assert_eq!(parsed.validate("tok"), Ok((kind, AgentEvent::Started)));
        }
    }

    #[test]
    fn wrong_protocol_version_is_rejected() {
        let mut msg = AgentMessage::new("tok", AgentKind::ClaudeCode, AgentEvent::Working);
        msg.version = 3;
        assert_eq!(msg.validate("tok"), Err("unsupported protocol version"));
        msg.version = 0;
        assert_eq!(msg.validate("tok"), Err("unsupported protocol version"));
    }

    #[test]
    fn wrong_token_is_rejected() {
        let msg = AgentMessage::new("tok", AgentKind::ClaudeCode, AgentEvent::Working);
        assert_eq!(msg.validate("other"), Err("token mismatch"));
        assert_eq!(msg.validate(""), Err("token mismatch"));
    }

    #[test]
    fn unknown_event_name_is_rejected() {
        let msg = AgentMessage {
            version: PROTOCOL_VERSION,
            token: "tok".into(),
            agent: Some("codex".into()),
            event: "deleting-system32".into(),
        };
        assert_eq!(msg.validate("tok"), Err("unknown event"));
    }

    #[test]
    fn unknown_agent_is_rejected() {
        let msg = AgentMessage {
            version: PROTOCOL_VERSION,
            token: "tok".into(),
            agent: Some("warp".into()),
            event: "working".into(),
        };
        assert_eq!(msg.validate("tok"), Err("unknown agent"));
    }

    #[test]
    fn malformed_json_fails_to_parse() {
        assert!(serde_json::from_str::<AgentMessage>("{not json}").is_err());
        assert!(serde_json::from_str::<AgentMessage>("[1,2,3]").is_err());
        assert!(serde_json::from_str::<AgentMessage>("{}").is_err());
    }

    #[test]
    fn version_constant_is_two() {
        assert_eq!(PROTOCOL_VERSION, 2);
    }
}
