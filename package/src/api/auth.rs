use std::fmt;
#[cfg(test)]
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ApiClient, ApiError};

const KEYRING_SERVICE: &str = "dev.mvp.cli";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct User {
    pub id: uuid::Uuid,
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProfileStats {
    pub daily_rank: Option<u32>,
    pub weekly_rank: Option<u32>,
    pub global_rank: Option<u32>,
    pub best_stack: i64,
    pub daily_pr_streak: u32,
    pub daily_fix_this_week: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OnlineProfile {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub stats: ProfileStats,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Session {
    token: String,
    pub expires_at: DateTime<Utc>,
    pub user: User,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("user", &self.user)
            .finish()
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct DeviceFlow {
    #[serde(rename = "flow_token")]
    poll_token: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct BrowserFlow {
    #[serde(rename = "flow_token")]
    poll_token: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl fmt::Debug for BrowserFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrowserFlow")
            .field("poll_token", &"[REDACTED]")
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct EmailFlow {
    #[serde(rename = "flow_token")]
    poll_token: String,
    #[serde(skip)]
    email: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl EmailFlow {
    pub fn email(&self) -> &str {
        &self.email
    }
}

impl fmt::Debug for EmailFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailFlow")
            .field("poll_token", &"[REDACTED]")
            .field("email", &"[REDACTED]")
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

pub trait PollingFlow {
    fn poll_token(&self) -> &str;
    fn poll_path(&self) -> &'static str;
    fn expires_in(&self) -> u64;
    fn interval(&self) -> u64;
}

impl PollingFlow for BrowserFlow {
    fn poll_token(&self) -> &str {
        &self.poll_token
    }

    fn poll_path(&self) -> &'static str {
        "/v1/auth/handoff/poll"
    }

    fn expires_in(&self) -> u64 {
        self.expires_in
    }

    fn interval(&self) -> u64 {
        self.interval
    }
}

impl PollingFlow for DeviceFlow {
    fn poll_token(&self) -> &str {
        &self.poll_token
    }

    fn poll_path(&self) -> &'static str {
        "/v1/auth/github/poll"
    }

    fn expires_in(&self) -> u64 {
        self.expires_in
    }

    fn interval(&self) -> u64 {
        self.interval
    }
}

impl PollingFlow for EmailFlow {
    fn poll_token(&self) -> &str {
        &self.poll_token
    }

    fn poll_path(&self) -> &'static str {
        "/v1/auth/email/poll"
    }

    fn expires_in(&self) -> u64 {
        self.expires_in
    }

    fn interval(&self) -> u64 {
        self.interval
    }
}

impl fmt::Debug for DeviceFlow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceFlow")
            .field("poll_token", &"[REDACTED]")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential is malformed")]
    Malformed,
    #[error("credential storage failed: {0}")]
    Storage(String),
}

pub trait CredentialStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, CredentialError>;
    fn set(&self, value: &str) -> Result<(), CredentialError>;
    fn delete(&self) -> Result<(), CredentialError>;
}

pub struct KeyringCredentialStore {
    account: String,
}

impl KeyringCredentialStore {
    pub fn for_origin(origin: &str) -> Self {
        Self {
            account: format!("api-session:{origin}"),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(KEYRING_SERVICE, &self.account)
            .map_err(|error| CredentialError::Storage(error.to_string()))
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn get(&self) -> Result<Option<String>, CredentialError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CredentialError::Storage(error.to_string())),
        }
    }

    fn set(&self, value: &str) -> Result<(), CredentialError> {
        self.entry()?
            .set_password(value)
            .map_err(|error| CredentialError::Storage(error.to_string()))
    }

    fn delete(&self) -> Result<(), CredentialError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(CredentialError::Storage(error.to_string())),
        }
    }
}

#[cfg(test)]
#[derive(Default, Clone)]
pub struct MemoryCredentialStore(Arc<Mutex<Option<String>>>);

#[cfg(test)]
impl CredentialStore for MemoryCredentialStore {
    fn get(&self) -> Result<Option<String>, CredentialError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| CredentialError::Storage("lock poisoned".into()))?
            .clone())
    }

    fn set(&self, value: &str) -> Result<(), CredentialError> {
        *self
            .0
            .lock()
            .map_err(|_| CredentialError::Storage("lock poisoned".into()))? = Some(value.into());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        *self
            .0
            .lock()
            .map_err(|_| CredentialError::Storage("lock poisoned".into()))? = None;
        Ok(())
    }
}

pub enum AuthFlow {
    Pending { retry_after: Duration },
    Complete(Session),
}

#[derive(Deserialize)]
struct PendingPoll {
    retry_after: u64,
}

pub struct SessionManager<S: CredentialStore> {
    client: ApiClient,
    store: S,
}

impl<S: CredentialStore> SessionManager<S> {
    pub fn new(client: ApiClient, store: S) -> Self {
        Self { client, store }
    }

    pub async fn start_github(&self, open_browser: bool) -> Result<DeviceFlow, ApiError> {
        let flow: DeviceFlow = self
            .client
            .post::<(), _>("/v1/auth/github/device", None, None)
            .await?;
        if open_browser {
            open::that(&flow.verification_uri).map_err(|error| {
                ApiError::Unavailable(format!("could not open browser: {error}"))
            })?;
        }
        Ok(flow)
    }

    pub async fn start_browser(&self) -> Result<BrowserFlow, ApiError> {
        self.client
            .post::<(), _>("/v1/auth/handoff/start", None, None)
            .await
    }

    pub async fn start_email(&self, email: &str) -> Result<EmailFlow, ApiError> {
        #[derive(Serialize)]
        struct StartEmail<'a> {
            email: &'a str,
        }

        let email = email.trim();
        if !valid_email_shape(email) {
            return Err(ApiError::Validation {
                code: "invalid_email".into(),
                message: "enter an email address such as user@example.com".into(),
            });
        }
        let mut flow: EmailFlow = self
            .client
            .post("/v1/auth/email/start", None, Some(&StartEmail { email }))
            .await?;
        flow.email = email.to_string();
        Ok(flow)
    }

    pub async fn poll_once<F: PollingFlow>(&self, flow: &F) -> Result<AuthFlow, ApiError> {
        #[derive(Serialize)]
        struct Poll<'a> {
            poll_token: &'a str,
        }

        let response = self
            .client
            .raw_post(
                flow.poll_path(),
                &Poll {
                    poll_token: flow.poll_token(),
                },
            )
            .await?;
        if response.status() == reqwest::StatusCode::ACCEPTED {
            let pending: PendingPoll = response
                .json()
                .await
                .map_err(|error| ApiError::MalformedResponse(error.to_string()))?;
            return Ok(AuthFlow::Pending {
                retry_after: Duration::from_secs(pending.retry_after),
            });
        }
        let session = self.client.parse_response(response).await?;
        self.save(&session).map_err(credential_api_error)?;
        Ok(AuthFlow::Complete(session))
    }

    pub fn load(&self) -> Result<Option<Session>, CredentialError> {
        let Some(value) = self.store.get()? else {
            return Ok(None);
        };
        let session: Session =
            serde_json::from_str(&value).map_err(|_| CredentialError::Malformed)?;
        if session.token.is_empty() {
            return Err(CredentialError::Malformed);
        }
        if session.is_expired() {
            self.store.delete()?;
            return Ok(None);
        }
        Ok(Some(session))
    }

    pub fn save(&self, session: &Session) -> Result<(), CredentialError> {
        let value = serde_json::to_string(session).map_err(|_| CredentialError::Malformed)?;
        self.store.set(&value)
    }

    pub async fn logout(&self, session: Option<&Session>) -> Result<bool, CredentialError> {
        let remote_revoked = match session {
            Some(session) => matches!(
                self.client
                    .post_empty("/v1/auth/logout", Some(session.token()))
                    .await,
                Ok(()) | Err(ApiError::Unauthorized)
            ),
            None => true,
        };
        self.store.delete()?;
        Ok(remote_revoked)
    }
}

fn valid_email_shape(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !email.chars().any(char::is_whitespace)
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.contains('.')
        && !domain.contains('@')
}

fn credential_api_error(error: CredentialError) -> ApiError {
    ApiError::Unavailable(error.to_string())
}

impl ApiClient {
    pub async fn me(&self, session: &Session) -> Result<OnlineProfile, ApiError> {
        self.get("/v1/me", Some(session.token())).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    fn server(status: &str, body: String) -> String {
        server_with_request(status, body).0
    }

    fn server_with_request(status: &str, body: String) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let (request_tx, request_rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let count = stream.read(&mut buffer).unwrap();
            let _ = request_tx.send(String::from_utf8_lossy(&buffer[..count]).into_owned());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        (format!("http://{address}"), request_rx)
    }

    fn session(expires_at: DateTime<Utc>) -> Session {
        Session {
            token: "secret-token".into(),
            expires_at,
            user: User {
                id: uuid::Uuid::new_v4(),
                username: "octocat".into(),
                display_name: "octocat".into(),
                avatar_url: None,
            },
        }
    }

    #[test]
    fn credential_missing_malformed_expiry_and_redaction() {
        let store = MemoryCredentialStore::default();
        let manager =
            SessionManager::new(ApiClient::new("http://localhost:1").unwrap(), store.clone());
        assert!(manager.load().unwrap().is_none());
        store.set("broken").unwrap();
        assert!(matches!(manager.load(), Err(CredentialError::Malformed)));
        let expired = session(Utc::now() - chrono::Duration::seconds(1));
        manager.save(&expired).unwrap();
        assert!(manager.load().unwrap().is_none());
        assert!(store.get().unwrap().is_none());
        assert!(
            !format!("{:?}", session(Utc::now() + chrono::Duration::hours(1)))
                .contains("secret-token")
        );
    }

    #[tokio::test]
    async fn logout_deletes_local_credential_when_server_is_unavailable() {
        let store = MemoryCredentialStore::default();
        let manager =
            SessionManager::new(ApiClient::new("http://127.0.0.1:1").unwrap(), store.clone());
        let active = session(Utc::now() + chrono::Duration::hours(1));
        manager.save(&active).unwrap();
        assert!(!manager.logout(Some(&active)).await.unwrap());
        assert!(store.get().unwrap().is_none());
    }

    #[tokio::test]
    async fn device_poll_completion_returns_and_persists_session() {
        let expected = session(Utc::now() + chrono::Duration::hours(1));
        let url = server("200 OK", serde_json::to_string(&expected).unwrap());
        let store = MemoryCredentialStore::default();
        let manager = SessionManager::new(ApiClient::new(&url).unwrap(), store.clone());
        let flow = DeviceFlow {
            poll_token: "opaque-poll-token-that-is-long-enough".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: 900,
            interval: 5,
        };

        let AuthFlow::Complete(actual) = manager.poll_once(&flow).await.unwrap() else {
            panic!("expected completed device flow");
        };
        assert_eq!(actual.user, expected.user);
        assert_eq!(manager.load().unwrap().unwrap().user, expected.user);
        assert!(!format!("{flow:?}").contains("opaque-poll-token"));
    }

    #[tokio::test]
    async fn browser_start_uses_handoff_endpoint_and_redacts_token() {
        let body = serde_json::json!({
            "flow_token": "browser-poll-token",
            "verification_uri": "https://mvp.dev/sign-in?flow=public",
            "expires_in": 600,
            "interval": 3
        });
        let (url, request) = server_with_request("201 Created", body.to_string());
        let manager = SessionManager::new(
            ApiClient::new(&url).unwrap(),
            MemoryCredentialStore::default(),
        );

        let flow = manager.start_browser().await.unwrap();

        assert_eq!(flow.verification_uri, "https://mvp.dev/sign-in?flow=public");
        assert_eq!(flow.expires_in, 600);
        let debug = format!("{flow:?}");
        assert!(debug.contains(&flow.verification_uri));
        assert!(!debug.contains("browser-poll-token"));
        assert!(
            request
                .recv()
                .unwrap()
                .starts_with("POST /v1/auth/handoff/start HTTP/1.1")
        );
    }

    #[tokio::test]
    async fn browser_poll_uses_handoff_endpoint_and_persists_session() {
        let expected = session(Utc::now() + chrono::Duration::hours(1));
        let (url, request) =
            server_with_request("200 OK", serde_json::to_string(&expected).unwrap());
        let store = MemoryCredentialStore::default();
        let manager = SessionManager::new(ApiClient::new(&url).unwrap(), store);
        let flow = BrowserFlow {
            poll_token: "browser-poll-token".into(),
            verification_uri: "https://mvp.dev/sign-in".into(),
            expires_in: 600,
            interval: 3,
        };

        let AuthFlow::Complete(actual) = manager.poll_once(&flow).await.unwrap() else {
            panic!("expected completed browser flow");
        };
        assert_eq!(actual.user, expected.user);
        assert_eq!(manager.load().unwrap().unwrap().user, expected.user);
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /v1/auth/handoff/poll HTTP/1.1"));
        assert!(request.contains(r#"{"poll_token":"browser-poll-token"}"#));
    }

    #[tokio::test]
    async fn email_start_validates_shape_and_redacts_sensitive_fields() {
        let manager = SessionManager::new(
            ApiClient::new("http://127.0.0.1:1").unwrap(),
            MemoryCredentialStore::default(),
        );
        for invalid in [
            "",
            "user",
            "@example.com",
            "user@localhost",
            "a b@example.com",
        ] {
            assert!(matches!(
                manager.start_email(invalid).await,
                Err(ApiError::Validation { code, .. }) if code == "invalid_email"
            ));
        }

        let body = serde_json::json!({
            "flow_token": "email-poll-token",
            "expires_in": 600,
            "interval": 3
        });
        let (url, request) = server_with_request("201 Created", body.to_string());
        let manager = SessionManager::new(
            ApiClient::new(&url).unwrap(),
            MemoryCredentialStore::default(),
        );
        let flow = manager.start_email(" user@example.com ").await.unwrap();
        assert_eq!(flow.email(), "user@example.com");
        assert_eq!(flow.expires_in, 600);
        let debug = format!("{flow:?}");
        assert!(!debug.contains("user@example.com"));
        assert!(!debug.contains("email-poll-token"));
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /v1/auth/email/start HTTP/1.1"));
        assert!(request.contains(r#"{"email":"user@example.com"}"#));
    }

    #[tokio::test]
    async fn email_poll_completion_uses_shared_polling_and_persists_session() {
        let expected = session(Utc::now() + chrono::Duration::hours(1));
        let (url, request) =
            server_with_request("200 OK", serde_json::to_string(&expected).unwrap());
        let store = MemoryCredentialStore::default();
        let manager = SessionManager::new(ApiClient::new(&url).unwrap(), store);
        let flow = EmailFlow {
            poll_token: "email-poll-token".into(),
            email: "user@example.com".into(),
            expires_in: 600,
            interval: 3,
        };

        let AuthFlow::Complete(actual) = manager.poll_once(&flow).await.unwrap() else {
            panic!("expected completed email flow");
        };
        assert_eq!(actual.user, expected.user);
        assert_eq!(manager.load().unwrap().unwrap().user, expected.user);
        let request = request.recv().unwrap();
        assert!(request.starts_with("POST /v1/auth/email/poll HTTP/1.1"));
        assert!(request.contains(r#"{"poll_token":"email-poll-token"}"#));
    }
}
