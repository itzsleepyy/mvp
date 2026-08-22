use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_API_URL: &str = "https://api.mostvaluedprogrammer.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct ClientVersion {
    pub minimum_version: String,
    pub latest_version: String,
    pub update_command: String,
}

impl ClientVersion {
    pub fn is_obsolete(&self, installed: &str) -> bool {
        let Ok(installed) = semver::Version::parse(installed) else {
            return false;
        };
        let Ok(minimum) = semver::Version::parse(&self.minimum_version) else {
            return false;
        };
        installed < minimum
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("authentication is required or has expired")]
    Unauthorized,
    #[error("API is unavailable: {0}")]
    Unavailable(String),
    #[error("API request timed out")]
    Timeout,
    #[error("API returned a malformed response: {0}")]
    MalformedResponse(String),
    #[error("request was rejected ({code}): {message}")]
    Validation { code: String, message: String },
    #[error("request conflicts with existing data ({code}): {message}")]
    Conflict { code: String, message: String },
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
}

impl ApiClient {
    pub fn from_env() -> Result<Self, ApiError> {
        let base = std::env::var("MVP_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.into());
        Self::new(&base)
    }

    pub fn new(base_url: &str) -> Result<Self, ApiError> {
        let normalized = base_url.trim_end_matches('/');
        let parsed = Url::parse(normalized).map_err(|error| ApiError::Validation {
            code: "invalid_base_url".into(),
            message: error.to_string(),
        })?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.cannot_be_a_base()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(ApiError::Validation {
                code: "invalid_base_url".into(),
                message: "MVP_API_URL must be an HTTP(S) base URL without query or fragment".into(),
            });
        }
        if parsed.scheme() == "http" && !is_loopback(&parsed) {
            return Err(ApiError::Validation {
                code: "insecure_base_url".into(),
                message: "HTTP is only allowed for loopback development APIs".into(),
            });
        }
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| ApiError::Unavailable(error.to_string()))?;
        Ok(Self {
            base_url: normalized.into(),
            http,
        })
    }

    pub fn origin(&self) -> &str {
        &self.base_url
    }

    pub async fn client_version(&self) -> Result<ClientVersion, ApiError> {
        self.get("/v1/client-version", None).await
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(base_url: &str, timeout: Duration) -> Result<Self, ApiError> {
        let mut client = Self::new(base_url)?;
        client.http = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .map_err(|error| ApiError::Unavailable(error.to_string()))?;
        Ok(client)
    }

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path_and_query: &str,
        token: Option<&str>,
    ) -> Result<T, ApiError> {
        self.send::<(), T>(Method::GET, path_and_query, token, None)
            .await
    }

    pub(crate) async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        self.send(Method::POST, path, token, body).await
    }

    pub(crate) async fn post_empty(&self, path: &str, token: Option<&str>) -> Result<(), ApiError> {
        let url = self.url(path)?;
        let mut request = self.http.post(url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(map_reqwest)?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(map_status(
            response.status(),
            response.bytes().await.map_err(map_reqwest)?.as_ref(),
        ))
    }

    pub(crate) async fn raw_post<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<reqwest::Response, ApiError> {
        self.http
            .post(self.url(path)?)
            .json(body)
            .send()
            .await
            .map_err(map_reqwest)
    }

    pub(crate) async fn parse_response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();
        let bytes = response.bytes().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(map_status(status, &bytes));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| ApiError::MalformedResponse(error.to_string()))
    }

    async fn send<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        let url = self.url(path)?;
        let mut request = self.http.request(method, url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(map_reqwest)?;
        self.parse_response(response).await
    }

    fn url(&self, path: &str) -> Result<Url, ApiError> {
        if !path.starts_with('/') {
            return Err(ApiError::Validation {
                code: "invalid_path".into(),
                message: "API paths must begin with /".into(),
            });
        }
        Url::parse(&format!("{}{path}", self.base_url)).map_err(|error| ApiError::Validation {
            code: "invalid_url".into(),
            message: error.to_string(),
        })
    }
}

fn is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn map_reqwest(error: reqwest::Error) -> ApiError {
    if error.is_timeout() {
        ApiError::Timeout
    } else {
        ApiError::Unavailable(error.to_string())
    }
}

fn map_status(status: StatusCode, bytes: &[u8]) -> ApiError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return ApiError::Unauthorized;
    }
    let envelope = serde_json::from_slice::<ErrorEnvelope>(bytes).ok();
    let (code, message) = envelope
        .map(|value| (value.error.code, value.error.message))
        .unwrap_or_else(|| {
            (
                format!("http_{}", status.as_u16()),
                "API returned an error without a valid error envelope".into(),
            )
        });
    match status {
        StatusCode::CONFLICT => ApiError::Conflict { code, message },
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => {
            ApiError::Validation { code, message }
        }
        status if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS => {
            ApiError::Unavailable(format!("{code}: {message}"))
        }
        _ => ApiError::Unavailable(format!("HTTP {}: {code}: {message}", status.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn server(status: &str, body: &str, delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            thread::sleep(delay);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn classifies_success_unauthorized_unavailable_and_malformed() {
        let url = server("200 OK", r#"{"value":1}"#, Duration::ZERO);
        let value: serde_json::Value = ApiClient::new(&format!("{url}/"))
            .unwrap()
            .get("/ok", None)
            .await
            .unwrap();
        assert_eq!(value["value"], 1);

        let url = server(
            "401 Unauthorized",
            r#"{"error":{"code":"unauthorized","message":"no"}}"#,
            Duration::ZERO,
        );
        assert!(matches!(
            ApiClient::new(&url)
                .unwrap()
                .get::<serde_json::Value>("/x", None)
                .await,
            Err(ApiError::Unauthorized)
        ));

        let url = server(
            "503 Unavailable",
            r#"{"error":{"code":"down","message":"later"}}"#,
            Duration::ZERO,
        );
        assert!(matches!(
            ApiClient::new(&url)
                .unwrap()
                .get::<serde_json::Value>("/x", None)
                .await,
            Err(ApiError::Unavailable(_))
        ));

        let url = server("200 OK", "not-json", Duration::ZERO);
        assert!(matches!(
            ApiClient::new(&url)
                .unwrap()
                .get::<serde_json::Value>("/x", None)
                .await,
            Err(ApiError::MalformedResponse(_))
        ));
    }

    #[tokio::test]
    async fn classifies_timeout() {
        let url = server("200 OK", "{}", Duration::from_millis(100));
        let client = ApiClient::with_timeout(&url, Duration::from_millis(20)).unwrap();
        assert!(matches!(
            client.get::<serde_json::Value>("/x", None).await,
            Err(ApiError::Timeout)
        ));
    }

    #[test]
    fn rejects_cleartext_non_loopback_api_urls() {
        assert!(matches!(
            ApiClient::new("http://example.com"),
            Err(ApiError::Validation { code, .. }) if code == "insecure_base_url"
        ));
        assert!(ApiClient::new("http://127.0.0.1:3000").is_ok());
        assert!(ApiClient::new("http://localhost:3000").is_ok());
    }

    #[test]
    fn production_api_origin_matches_the_public_deployment() {
        assert_eq!(DEFAULT_API_URL, "https://api.mostvaluedprogrammer.com");
    }

    #[test]
    fn client_version_blocks_only_versions_below_the_minimum() {
        let policy = ClientVersion {
            minimum_version: "1.2.0".into(),
            latest_version: "1.4.0".into(),
            update_command: "update".into(),
        };
        assert!(policy.is_obsolete("1.1.9"));
        assert!(!policy.is_obsolete("1.2.0"));
        assert!(!policy.is_obsolete("2.0.0"));
        assert!(!policy.is_obsolete("unknown"));
    }
}
