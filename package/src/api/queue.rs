use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ApiClient, ApiError, RunPayload, Session};

pub const MAX_PENDING_RUNS: usize = 500;
const MAX_BACKOFF_ATTEMPTS: u8 = 8;
const PROCESS_BATCH_SIZE: usize = 3;
const RESERVATION_SECONDS: i64 = 60;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("pending run queue is full")]
    Full,
    #[error("pending run queue I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("pending run queue serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid queued run: {0}")]
    InvalidRun(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingRun {
    run: RunPayload,
    user_id: uuid::Uuid,
    api_origin: String,
    attempts: u8,
    next_attempt_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueProcessResult {
    pub submitted: usize,
    pub retried: usize,
    pub dropped: usize,
    pub remaining: usize,
    pub paused_unauthorized: bool,
}

#[derive(Debug)]
pub struct PendingRunQueue {
    path: PathBuf,
    entries: Vec<PendingRun>,
}

impl PendingRunQueue {
    pub fn default_path() -> PathBuf {
        directories::ProjectDirs::from("", "", "MVP")
            .map(|dirs| dirs.config_dir().join("pending_runs.json"))
            .unwrap_or_else(|| PathBuf::from("pending_runs.json"))
    }

    pub fn path_for_user(user_id: uuid::Uuid) -> PathBuf {
        Self::default_path()
            .with_file_name("pending_runs")
            .join(format!("{user_id}.json"))
    }

    pub fn load(path: impl Into<PathBuf>) -> Result<Self, QueueError> {
        let path = path.into();
        let entries = read_entries(&path)?;
        Ok(Self { path, entries })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn enqueue(
        &mut self,
        run: RunPayload,
        user_id: uuid::Uuid,
        api_origin: &str,
    ) -> Result<bool, QueueError> {
        run.validate()
            .map_err(|error| QueueError::InvalidRun(error.to_string()))?;
        let lock = self.lock()?;
        self.entries = read_entries(&self.path)?;
        if self
            .entries
            .iter()
            .any(|entry| entry.run.client_run_id == run.client_run_id)
        {
            return Ok(false);
        }
        if self.entries.len() >= MAX_PENDING_RUNS {
            return Err(QueueError::Full);
        }
        self.entries.push(PendingRun {
            run,
            user_id,
            api_origin: api_origin.to_string(),
            attempts: 0,
            next_attempt_at: 0,
        });
        self.persist()?;
        FileExt::unlock(&lock)?;
        Ok(true)
    }

    pub async fn process_due(
        &mut self,
        client: &ApiClient,
        session: &Session,
    ) -> Result<QueueProcessResult, QueueError> {
        let mut result = QueueProcessResult::default();
        for _ in 0..PROCESS_BATCH_SIZE {
            let now = unix_seconds();
            let lock = self.lock()?;
            self.entries = read_entries(&self.path)?;
            let selected = self
                .entries
                .iter_mut()
                .find(|entry| {
                    entry.next_attempt_at <= now
                        && entry.user_id == session.user.id
                        && entry.api_origin == client.origin()
                })
                .map(|entry| {
                    entry.next_attempt_at = now + RESERVATION_SECONDS;
                    entry.clone()
                });
            self.persist()?;
            FileExt::unlock(&lock)?;
            let Some(selected) = selected else {
                break;
            };

            let outcome = client.submit_run(session, &selected.run).await;
            let lock = self.lock()?;
            self.entries = read_entries(&self.path)?;
            if let Some(index) = self.entries.iter().position(|entry| {
                entry.run.client_run_id == selected.run.client_run_id
                    && entry.user_id == selected.user_id
                    && entry.api_origin == selected.api_origin
            }) {
                match outcome {
                    Ok(_) => {
                        self.entries.remove(index);
                        result.submitted += 1;
                    }
                    Err(ApiError::Unauthorized) => {
                        self.entries[index].next_attempt_at = 0;
                        result.paused_unauthorized = true;
                    }
                    Err(ApiError::Validation { .. } | ApiError::Conflict { .. }) => {
                        self.entries.remove(index);
                        result.dropped += 1;
                    }
                    Err(_) => {
                        let entry = &mut self.entries[index];
                        entry.attempts = entry.attempts.saturating_add(1).min(MAX_BACKOFF_ATTEMPTS);
                        entry.next_attempt_at = unix_seconds() + retry_delay(entry.attempts);
                        result.retried += 1;
                    }
                }
            }
            self.persist()?;
            FileExt::unlock(&lock)?;
            if result.paused_unauthorized {
                break;
            }
        }

        let lock = self.lock()?;
        self.entries = read_entries(&self.path)?;
        result.remaining = self.entries.len();
        FileExt::unlock(&lock)?;
        Ok(result)
    }

    fn lock(&self) -> Result<fs::File, QueueError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let path = self.path.with_extension("json.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn persist(&self) -> Result<(), QueueError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(&self.entries)?;
        let temp = self
            .path
            .with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
        let mut file = fs::File::create(&temp)?;
        file.write_all(&data)?;
        file.sync_all()?;
        fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

fn retry_delay(attempts: u8) -> i64 {
    2_i64
        .pow(u32::from(attempts.min(MAX_BACKOFF_ATTEMPTS)))
        .min(300)
}

fn read_entries(path: &Path) -> Result<Vec<PendingRun>, QueueError> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Vec<PendingRun>>(&bytes) {
            Ok(entries)
                if entries.len() <= MAX_PENDING_RUNS
                    && entries.iter().all(|entry| {
                        entry.attempts <= MAX_BACKOFF_ATTEMPTS
                            && !entry.api_origin.is_empty()
                            && entry.run.validate().is_ok()
                    }) =>
            {
                Ok(entries)
            }
            Ok(_) | Err(_) => {
                quarantine(path);
                Ok(Vec::new())
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn quarantine(path: &Path) {
    let quarantine = path.with_extension(format!("json.corrupt-{}", unix_seconds()));
    let _ = fs::rename(path, quarantine);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::Map;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    fn path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "mvp-api-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn run() -> RunPayload {
        RunPayload::new(
            super::super::GameId::StackOverflow,
            50,
            1,
            "test",
            Map::new(),
            None,
        )
        .unwrap()
    }

    fn session() -> Session {
        serde_json::from_value(serde_json::json!({
            "token": "secret",
            "expires_at": (Utc::now() + Duration::hours(1)).to_rfc3339(),
            "user": {
                "id": uuid::Uuid::nil(),
                "username": "octocat",
                "display_name": "octocat",
                "avatar_url": null
            }
        }))
        .unwrap()
    }

    fn server(status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}")
    }

    fn submitted(run: &RunPayload) -> String {
        serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "client_run_id": run.client_run_id,
            "game_id": "stack_overflow",
            "challenge_date": "2026-08-20",
            "challenge_version": null,
            "challenge_id": null,
            "raw_score": 50,
            "normalized_score": 50,
            "result": {},
            "duration_ms": 1,
            "client_version": "test",
            "normalization_version": 1,
            "created_at": "2026-08-20T12:00:00Z"
        })
        .to_string()
    }

    #[test]
    fn corruption_degrades_to_empty_and_is_quarantined() {
        let file = path("pending_runs.json");
        fs::write(&file, "not json").unwrap();
        let queue = PendingRunQueue::load(&file).unwrap();
        assert!(queue.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn duplicate_enqueue_is_ignored_and_round_trips() {
        let file = path("pending_runs.json");
        let mut queue = PendingRunQueue::load(&file).unwrap();
        let run = run();
        assert!(
            queue
                .enqueue(run.clone(), uuid::Uuid::nil(), "https://api.example.com")
                .unwrap()
        );
        assert!(
            !queue
                .enqueue(run, uuid::Uuid::nil(), "https://api.example.com")
                .unwrap()
        );
        assert_eq!(PendingRunQueue::load(&file).unwrap().len(), 1);
        let _ = fs::remove_file(file);
    }

    #[test]
    fn retry_metadata_is_exponential_and_bounded() {
        assert_eq!(retry_delay(1), 2);
        assert_eq!(retry_delay(2), 4);
        assert_eq!(retry_delay(7), 128);
        assert_eq!(retry_delay(8), 256);
        assert_eq!(MAX_BACKOFF_ATTEMPTS, 8);
    }

    #[tokio::test]
    async fn success_removes_run_and_unavailable_schedules_retry() {
        let success_path = path("success.json");
        let mut success_queue = PendingRunQueue::load(&success_path).unwrap();
        let success_run = run();
        let client = ApiClient::new(&server("200 OK", &submitted(&success_run))).unwrap();
        success_queue
            .enqueue(success_run.clone(), uuid::Uuid::nil(), client.origin())
            .unwrap();
        let result = success_queue
            .process_due(&client, &session())
            .await
            .unwrap();
        assert_eq!(result.submitted, 1);
        assert!(success_queue.is_empty());

        let retry_path = path("retry.json");
        let mut retry_queue = PendingRunQueue::load(&retry_path).unwrap();
        let client = ApiClient::new(&server(
            "503 Unavailable",
            r#"{"error":{"code":"unavailable","message":"later"}}"#,
        ))
        .unwrap();
        retry_queue
            .enqueue(run(), uuid::Uuid::nil(), client.origin())
            .unwrap();
        let result = retry_queue.process_due(&client, &session()).await.unwrap();
        assert_eq!(result.retried, 1);
        assert_eq!(retry_queue.entries[0].attempts, 1);
        assert!(retry_queue.entries[0].next_attempt_at > unix_seconds());

        let _ = fs::remove_file(success_path);
        let _ = fs::remove_file(retry_path);
    }

    #[tokio::test]
    async fn unauthorized_pauses_without_deleting_or_incrementing() {
        let file = path("unauthorized.json");
        let mut queue = PendingRunQueue::load(&file).unwrap();
        let client = ApiClient::new(&server(
            "401 Unauthorized",
            r#"{"error":{"code":"unauthorized","message":"login"}}"#,
        ))
        .unwrap();
        queue
            .enqueue(run(), uuid::Uuid::nil(), client.origin())
            .unwrap();
        let result = queue.process_due(&client, &session()).await.unwrap();
        assert!(result.paused_unauthorized);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.entries[0].attempts, 0);
        let _ = fs::remove_file(file);
    }

    #[tokio::test]
    async fn pending_runs_stay_bound_to_their_account_and_origin() {
        let file = path("account.json");
        let mut queue = PendingRunQueue::load(&file).unwrap();
        let client = ApiClient::new("http://127.0.0.1:1").unwrap();
        queue
            .enqueue(run(), uuid::Uuid::new_v4(), client.origin())
            .unwrap();

        let result = queue.process_due(&client, &session()).await.unwrap();
        assert_eq!(result.remaining, 1);
        assert_eq!(result.retried, 0, "another account must not submit it");

        let _ = fs::remove_file(&file);
        let _ = fs::remove_file(file.with_extension("json.lock"));
    }
}
