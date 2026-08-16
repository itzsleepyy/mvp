//! Local-only IPC between a running WaitState instance and short-lived
//! clients such as Claude Code hook commands.
//!
//! Design (chosen over Unix sockets/Tokio):
//!
//! - **Transport:** loopback TCP (`127.0.0.1`), bound to an ephemeral port.
//!   One code path on every platform, and nothing is ever reachable from
//!   outside the machine.
//! - **Discovery:** the server writes a per-user socket file (runtime dir)
//!   holding the port and a per-run token; the client reads it, connects and
//!   sends one message. A stale file simply fails to connect — clean and fast.
//! - **Threading:** a plain `std::thread` accept loop pushing `AgentEvent`s
//!   through a `std::sync::mpsc` channel. The application loop drains the
//!   channel, so all app state stays single-threaded. No async runtime
//!   needed for one short-lived client at a time — `tokio` remains out.
//! - **Safety:** inputs are untrusted. Messages are length-capped, JSON is
//!   parsed into a small schema, versions and the per-run token are checked,
//!   and unknown events are dropped. The server never writes to clients and
//!   never executes anything.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::send_event;
pub use server::{IpcServer, socket_path};
