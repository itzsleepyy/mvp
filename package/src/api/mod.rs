//! Isolated online API foundation. The game can opt into this module without
//! coupling its event loop to Tokio or network I/O.

mod auth;
mod client;
mod leaderboard;
mod queue;
mod runs;

pub use auth::{AuthFlow, KeyringCredentialStore, Session, SessionManager};
pub use client::{ApiClient, ApiError};
pub use leaderboard::{Leaderboard, LeaderboardRequest};
pub use queue::{PendingRunQueue, QueueProcessResult};
pub use runs::{GameId, RunPayload};

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Work accepted by the background owner. Sending is synchronous and never
/// performs filesystem or network I/O on the caller's thread.
#[derive(Debug)]
pub enum WorkerCommand {
    Submit(RunPayload),
    Leaderboard(LeaderboardRequest),
    RetryPending,
    Shutdown,
}

#[derive(Debug)]
pub enum WorkerEvent {
    RunQueued(uuid::Uuid),
    QueueProcessed(QueueProcessResult),
    Leaderboard(Result<Leaderboard, ApiError>),
    Error(String),
    Stopped,
}

pub struct ApiWorker {
    commands: Sender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    join: Option<thread::JoinHandle<()>>,
}

impl ApiWorker {
    pub fn spawn_default(client: ApiClient, session: Option<Session>) -> std::io::Result<Self> {
        let queue_path = session
            .as_ref()
            .map(|session| PendingRunQueue::path_for_user(session.user.id))
            .unwrap_or_else(PendingRunQueue::default_path);
        Self::spawn(client, session, queue_path)
    }

    pub fn spawn(
        client: ApiClient,
        session: Option<Session>,
        queue_path: PathBuf,
    ) -> std::io::Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("mvp-api".into())
            .spawn(move || {
                worker_main(client, session, queue_path, command_rx, event_tx, ready_tx)
            })?;
        ready_rx
            .recv()
            .map_err(|_| std::io::Error::other("online worker stopped during startup"))?
            .map_err(std::io::Error::other)?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            join: Some(join),
        })
    }

    pub fn commands(&self) -> Sender<WorkerCommand> {
        self.commands.clone()
    }

    pub fn try_event(&self) -> Result<WorkerEvent, mpsc::TryRecvError> {
        self.events.try_recv()
    }

    /// Flushes commands already sent to the worker, then joins it so a clean
    /// process exit cannot race durable queue persistence.
    pub fn shutdown(mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ApiWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn worker_main(
    client: ApiClient,
    session: Option<Session>,
    queue_path: PathBuf,
    commands: Receiver<WorkerCommand>,
    events: Sender<WorkerEvent>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(format!("Tokio runtime: {error}")));
            return;
        }
    };
    let mut queue = match PendingRunQueue::load(queue_path) {
        Ok(queue) => queue,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    loop {
        let command = match commands.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(session) = &session
                    && let Ok(result) = runtime.block_on(queue.process_due(&client, session))
                    && (result.submitted > 0 || result.retried > 0 || result.dropped > 0)
                {
                    let _ = events.send(WorkerEvent::QueueProcessed(result));
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match command {
            WorkerCommand::Submit(run) => {
                let Some(session) = &session else {
                    let _ = events.send(WorkerEvent::Error(
                        "sign in to submit runs online".to_string(),
                    ));
                    continue;
                };
                let id = run.client_run_id;
                match queue.enqueue(run, session.user.id, client.origin()) {
                    Ok(_) => {
                        let _ = events.send(WorkerEvent::RunQueued(id));
                        let result = runtime.block_on(queue.process_due(&client, session));
                        match result {
                            Ok(result) => {
                                let _ = events.send(WorkerEvent::QueueProcessed(result));
                            }
                            Err(error) => {
                                let _ = events.send(WorkerEvent::Error(error.to_string()));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = events.send(WorkerEvent::Error(error.to_string()));
                    }
                }
            }
            WorkerCommand::Leaderboard(request) => {
                let result = runtime.block_on(client.leaderboard(request));
                let _ = events.send(WorkerEvent::Leaderboard(result));
            }
            WorkerCommand::RetryPending => {
                let Some(session) = &session else {
                    continue;
                };
                match runtime.block_on(queue.process_due(&client, session)) {
                    Ok(result) => {
                        let _ = events.send(WorkerEvent::QueueProcessed(result));
                    }
                    Err(error) => {
                        let _ = events.send(WorkerEvent::Error(error.to_string()));
                    }
                }
            }
            WorkerCommand::Shutdown => break,
        }
    }
    let _ = events.send(WorkerEvent::Stopped);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flushes_submissions_to_the_durable_queue() {
        let path = std::env::temp_dir().join(format!(
            "mvp-worker-shutdown-{}-{}.json",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let client = ApiClient::new("http://127.0.0.1:1").unwrap();
        let session: Session = serde_json::from_value(serde_json::json!({
            "token": "secret",
            "expires_at": "2099-01-01T00:00:00Z",
            "user": {
                "id": uuid::Uuid::nil(),
                "username": "octocat",
                "display_name": "octocat",
                "avatar_url": null
            }
        }))
        .unwrap();
        let worker = ApiWorker::spawn(client, Some(session), path.clone()).unwrap();
        worker
            .commands()
            .send(WorkerCommand::Submit(
                RunPayload::new(
                    GameId::StackOverflow,
                    50,
                    1,
                    "test",
                    serde_json::Map::new(),
                    None,
                )
                .unwrap(),
            ))
            .unwrap();
        worker.shutdown();

        assert_eq!(PendingRunQueue::load(&path).unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.lock"));
    }
}
