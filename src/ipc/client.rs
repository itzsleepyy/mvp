//! The IPC client: sends one lifecycle event to the running WaitState
//! instance and exits. Used by coding-agent hook commands, which must never
//! hang or produce output — every failure mode is a fast, silent exit.

use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use crate::agent::event::AgentEvent;
use crate::agent::status::AgentKind;
use crate::ipc::protocol::AgentMessage;
use crate::ipc::server::{read_socket_info, socket_path};

/// Sends `event` for `kind` to the running instance discovered via the
/// default socket file. Errors mean "WaitState is not running" and are safe
/// to ignore.
pub fn send_event(kind: AgentKind, event: AgentEvent) -> Result<(), String> {
    send_event_to(socket_path(), kind, event)
}

/// Same as [`send_event`], with an explicit socket path (used by tests).
pub fn send_event_to(path: PathBuf, kind: AgentKind, event: AgentEvent) -> Result<(), String> {
    let info = read_socket_info(&path).ok_or("WaitState is not running")?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))
        .map_err(|_| "WaitState is not reachable")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    let message = AgentMessage::new(&info.token, kind, event);
    let wire = serde_json::to_string(&message).map_err(|e| e.to_string())?;
    stream
        .write_all(wire.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .map_err(|e| e.to_string())
}

/// True when a WaitState instance is currently reachable. Does not send an
/// event — a pure connectivity probe.
pub fn running() -> bool {
    match read_socket_info(&socket_path()) {
        Some(info) => {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
            TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_event_to_missing_socket_file_fails_cleanly() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "waitstate_client_test_{}_missing.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let start = std::time::Instant::now();
        let result = send_event_to(path, AgentKind::ClaudeCode, AgentEvent::Working);
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn send_event_to_garbage_socket_file_fails_cleanly() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "waitstate_client_test_{}_garbage.sock",
            std::process::id()
        ));
        std::fs::write(&path, b"this is not json").unwrap();
        let result = send_event_to(path, AgentKind::ClaudeCode, AgentEvent::Working);
        assert!(result.is_err());
    }
}
