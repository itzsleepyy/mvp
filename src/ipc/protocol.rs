//! The tiny structured protocol spoken between the IPC client and server:
//! one JSON line with a version, the per-run token and an event name.

use serde::{Deserialize, Serialize};

use crate::agent::event::AgentEvent;

/// Current protocol version. Bump when the message shape changes and make
/// the server reject older/newer versions gracefully.
pub const PROTOCOL_VERSION: u8 = 1;

/// Hard cap on a message line. Anything longer is dropped before parsing.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// One message from a client to the server. `event` is kept as a string so
/// unknown events deserialize and can be rejected by validation instead of
/// failing parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessage {
    pub version: u8,
    pub token: String,
    pub event: String,
}

impl AgentMessage {
    pub fn new(token: &str, event: AgentEvent) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            token: token.to_string(),
            event: event.as_str().to_string(),
        }
    }

    /// Validates the message against the expected token and returns the
    /// parsed event. `Err` covers every way a hostile/broken message can
    /// look; callers simply drop the connection.
    pub fn validate(&self, expected_token: &str) -> Result<AgentEvent, &'static str> {
        if self.version != PROTOCOL_VERSION {
            return Err("unsupported protocol version");
        }
        if self.token != expected_token {
            return Err("token mismatch");
        }
        AgentEvent::from_wire(&self.event).ok_or("unknown event")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_roundtrips_through_json() {
        let msg = AgentMessage::new("tok123", AgentEvent::NeedsInput);
        let wire = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&wire).unwrap();
        assert_eq!(parsed, msg);
        assert_eq!(parsed.validate("tok123"), Ok(AgentEvent::NeedsInput));
    }

    #[test]
    fn wrong_protocol_version_is_rejected() {
        let mut msg = AgentMessage::new("tok", AgentEvent::Working);
        msg.version = 2;
        assert_eq!(msg.validate("tok"), Err("unsupported protocol version"));
        msg.version = 0;
        assert_eq!(msg.validate("tok"), Err("unsupported protocol version"));
    }

    #[test]
    fn wrong_token_is_rejected() {
        let msg = AgentMessage::new("tok", AgentEvent::Working);
        assert_eq!(msg.validate("other"), Err("token mismatch"));
        assert_eq!(msg.validate(""), Err("token mismatch"));
    }

    #[test]
    fn unknown_event_name_is_rejected() {
        let msg = AgentMessage {
            version: PROTOCOL_VERSION,
            token: "tok".into(),
            event: "deleting-system32".into(),
        };
        assert_eq!(msg.validate("tok"), Err("unknown event"));
    }

    #[test]
    fn malformed_json_fails_to_parse() {
        assert!(serde_json::from_str::<AgentMessage>("{not json}").is_err());
        assert!(serde_json::from_str::<AgentMessage>("[1,2,3]").is_err());
        assert!(serde_json::from_str::<AgentMessage>("{}").is_err());
    }

    #[test]
    fn version_constant_is_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
