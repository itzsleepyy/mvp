//! The IPC server: a background accept loop that turns incoming messages
//! into `AgentEvent`s delivered over a channel to the application loop.

use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agent::event::AgentEvent;
use crate::ipc::protocol::{AgentMessage, MAX_MESSAGE_BYTES};

/// Contents of the socket file clients read to reach the server.
#[derive(Debug, Serialize, Deserialize)]
pub struct SocketInfo {
    pub version: u8,
    pub port: u16,
    pub token: String,
}

/// A bound, listening IPC server. Dropping it removes the socket file and
/// stops the background thread.
pub struct IpcServer {
    receiver: Receiver<AgentEvent>,
    running: Arc<AtomicBool>,
    port: u16,
    socket_path: PathBuf,
}

impl IpcServer {
    /// Binds loopback and starts the accept thread.
    ///
    /// Returns `Ok(None)` when the socket path is owned by another live
    /// WaitState instance (checked by connecting to it), so a second
    /// instance never steals the first one's agent events.
    pub fn start(socket_path: PathBuf) -> std::io::Result<Option<Self>> {
        if let Some(owner) = read_socket_info(&socket_path)
            && server_answers(&owner)
        {
            return Ok(None);
        }

        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let info = SocketInfo {
            version: 1,
            port,
            token,
        };
        write_socket_file(&socket_path, &info)?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let thread_token = info.token.clone();
        let thread_path = socket_path.clone();
        thread::Builder::new()
            .name("waitstate-ipc".into())
            .spawn(move || {
                accept_loop(listener, thread_running, thread_token, thread_path, sender)
            })?;

        Ok(Some(Self {
            receiver,
            running,
            port,
            socket_path,
        }))
    }

    /// Drains all currently queued events. Called once per frame from the
    /// application loop so state changes stay single-threaded.
    pub fn try_recv(&self) -> Option<AgentEvent> {
        self.receiver.try_recv().ok()
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.socket_path);
        // Self-connect to wake the nonblocking accept loop so the thread
        // can observe the shutdown flag and exit.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// True when a WaitState instance can be reached at the given socket info.
/// A bounded probe: any failure means "not running".
fn server_answers(info: &SocketInfo) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
}

fn accept_loop(
    listener: TcpListener,
    running: Arc<AtomicBool>,
    token: String,
    socket_path: PathBuf,
    sender: Sender<AgentEvent>,
) {
    // Any accept error (WouldBlock, EMFILE under load, ...) is transient:
    // sleep and retry. The loop only exits when the shutdown flag flips;
    // a permanently dead listener still terminates because `drop` sets the
    // flag and self-connects to wake this thread.
    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => handle_connection(stream, &token, &sender),
            Err(_) => thread::sleep(Duration::from_millis(20)),
        }
    }
    let _ = std::fs::remove_file(&socket_path);
}

/// Reads one message from a connection, validates it and forwards the event.
/// Everything invalid is dropped silently — the client is untrusted and the
/// server must survive arbitrary garbage.
fn handle_connection(stream: TcpStream, token: &str, sender: &Sender<AgentEvent>) {
    // On BSD-derived systems (macOS) accepted sockets inherit the
    // listener's non-blocking flag: without this, the first read can race
    // the client's write and return EAGAIN before any data arrives.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut reader = BufReader::new(stream);

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.len() > MAX_MESSAGE_BYTES {
        return;
    }
    let Ok(message) = serde_json::from_str::<AgentMessage>(line.trim()) else {
        return;
    };
    let Ok(event) = message.validate(token) else {
        return;
    };
    // Best-effort delivery; a full channel only loses a duplicate event.
    let _ = sender.send(event);
}

/// The per-user socket file used for discovery.
pub fn socket_path() -> PathBuf {
    let dir =
        match directories::BaseDirs::new().and_then(|b| b.runtime_dir().map(Path::to_path_buf)) {
            Some(dir) => dir,
            None => {
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_else(|_| "user".into());
                std::env::temp_dir().join(format!("waitstate-{user}"))
            }
        };
    dir.join("waitstate.sock")
}

pub(crate) fn read_socket_info(path: &Path) -> Option<SocketInfo> {
    let bytes = std::fs::read(path).ok()?;
    let info: SocketInfo = serde_json::from_slice(&bytes).ok()?;
    (info.version == 1).then_some(info)
}

fn write_socket_file(path: &Path, info: &SocketInfo) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        // Only tighten permissions on directories we just created; never
        // touch pre-existing (possibly shared) directories.
        let created = !dir.exists();
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        if created {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let json = serde_json::to_vec(info).expect("SocketInfo serialization cannot fail");
    std::fs::write(path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn random_token() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let token: u128 = rng.random();
    format!("{token:032x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn temp_socket_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "waitstate_ipc_test_{}_{}.sock",
            std::process::id(),
            name
        ))
    }

    fn wait_for<T>(mut poll: impl FnMut() -> Option<T>, timeout: Duration) -> Option<T> {
        let start = std::time::Instant::now();
        loop {
            if let Some(value) = poll() {
                return Some(value);
            }
            if start.elapsed() > timeout {
                return None;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn server_starts_and_writes_a_socket_file() {
        let path = temp_socket_path("starts");
        let _ = std::fs::remove_file(&path);
        let server = IpcServer::start(path.clone()).unwrap().unwrap();
        assert!(path.exists());
        let info = read_socket_info(&path).unwrap();
        assert_eq!(info.version, 1);
        assert!(!info.token.is_empty());
        assert!(server.socket_path() == path);
    }

    #[test]
    fn client_event_reaches_the_server() {
        let path = temp_socket_path("event");
        let _ = std::fs::remove_file(&path);
        let server = IpcServer::start(path.clone()).unwrap().unwrap();

        crate::ipc::client::send_event_to(path, AgentEvent::Working).unwrap();
        let received = wait_for(|| server.try_recv(), Duration::from_secs(2));
        assert_eq!(received, Some(AgentEvent::Working));
    }

    #[test]
    fn all_events_are_transmitted() {
        let path = temp_socket_path("all");
        let _ = std::fs::remove_file(&path);
        let server = IpcServer::start(path.clone()).unwrap().unwrap();

        for event in [
            AgentEvent::Started,
            AgentEvent::Working,
            AgentEvent::NeedsInput,
            AgentEvent::Completed,
            AgentEvent::Stopped,
        ] {
            crate::ipc::client::send_event_to(path.clone(), event).unwrap();
            let received = wait_for(|| server.try_recv(), Duration::from_secs(2));
            assert_eq!(received, Some(event));
        }
    }

    fn raw_send(path: &Path, payload: &str) -> std::io::Result<()> {
        let info = read_socket_info(path).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], info.port));
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
        std::io::Write::write_all(&mut stream, payload.as_bytes())
    }

    #[test]
    fn malformed_messages_are_dropped_and_server_survives() {
        let path = temp_socket_path("malformed");
        let _ = std::fs::remove_file(&path);
        let server = IpcServer::start(path.clone()).unwrap().unwrap();
        let info = read_socket_info(&path).unwrap();

        raw_send(&path, "{not json}\n").unwrap();
        raw_send(&path, "[1,2,3]\n").unwrap();
        raw_send(&path, "").unwrap();
        raw_send(&path, &format!("{}\n", "x".repeat(MAX_MESSAGE_BYTES + 1))).unwrap();
        // Valid JSON, valid shape, but the wrong token.
        raw_send(
            &path,
            "{\"version\":1,\"token\":\"wrong\",\"event\":\"working\"}\n",
        )
        .unwrap();
        // Valid JSON, valid token, unsupported version.
        raw_send(
            &path,
            &format!(
                "{{\"version\":99,\"token\":\"{}\",\"event\":\"working\"}}\n",
                info.token
            ),
        )
        .unwrap();
        // Valid JSON, valid token, unknown event.
        raw_send(
            &path,
            &format!(
                "{{\"version\":1,\"token\":\"{}\",\"event\":\"explode\"}}\n",
                info.token
            ),
        )
        .unwrap();

        thread::sleep(Duration::from_millis(150));
        assert!(server.try_recv().is_none());

        // The server is still healthy: a valid message still arrives.
        crate::ipc::client::send_event_to(path, AgentEvent::NeedsInput).unwrap();
        let received = wait_for(|| server.try_recv(), Duration::from_secs(2));
        assert_eq!(received, Some(AgentEvent::NeedsInput));
    }

    #[test]
    fn dropping_the_server_removes_the_socket_file() {
        let path = temp_socket_path("drop");
        let _ = std::fs::remove_file(&path);
        {
            let _server = IpcServer::start(path.clone()).unwrap().unwrap();
            assert!(path.exists());
        }
        thread::sleep(Duration::from_millis(100));
        assert!(!path.exists());
    }

    #[test]
    fn second_instance_yields_to_the_owner_of_the_socket_file() {
        let path = temp_socket_path("second");
        let _ = std::fs::remove_file(&path);
        let first = IpcServer::start(path.clone()).unwrap().unwrap();
        let second = IpcServer::start(path.clone()).unwrap();
        assert!(second.is_none(), "second instance must not steal events");
        assert!(first.try_recv().is_none());
        assert!(path.exists());
    }

    #[test]
    fn stale_socket_file_is_reclaimed() {
        let path = temp_socket_path("stale");
        let _ = std::fs::remove_file(&path);
        // Pretend an old instance died without cleanup: write a socket file
        // pointing at a port with nothing listening.
        let stale = SocketInfo {
            version: 1,
            port: 9,
            token: "dead".into(),
        };
        write_socket_file(&path, &stale).unwrap();
        let server = IpcServer::start(path.clone()).unwrap();
        assert!(server.is_some(), "stale socket file must be reclaimed");
    }

    #[test]
    fn client_fails_fast_when_nothing_is_running() {
        let path = temp_socket_path("gone");
        let _ = std::fs::remove_file(&path);
        let start = std::time::Instant::now();
        let result = crate::ipc::client::send_event_to(path, AgentEvent::Working);
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn socket_path_is_shared_across_calls() {
        assert_eq!(socket_path(), socket_path());
    }
}
