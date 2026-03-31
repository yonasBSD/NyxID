use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// User-Agent string sent on all CLI HTTP requests.
pub const CLI_USER_AGENT: &str = concat!("nyxid-cli/", env!("CARGO_PKG_VERSION"));

pub fn build_cli_http_client() -> Result<Client> {
    Client::builder()
        .user_agent(CLI_USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")
}

pub struct ApiClient {
    client: Client,
    base_url: String,
    access_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, access_token: String) -> Result<Self> {
        let client = build_cli_http_client()?;

        Ok(Self {
            client,
            base_url: format!("{}/api/v1", base_url.trim_end_matches('/')),
            access_token,
        })
    }

    pub fn from_auth(auth: &crate::cli::AuthArgs) -> Result<Self> {
        let base_url = auth.resolved_base_url()?;
        let token = crate::auth::resolve_access_token(auth)?;
        Self::new(&base_url, token)
    }

    pub fn base_url_root(&self) -> &str {
        self.base_url
            .strip_suffix("/api/v1")
            .unwrap_or(&self.base_url)
    }

    /// Attempt to refresh the access token using the saved refresh token.
    /// Returns `true` if the token was refreshed successfully.
    async fn try_refresh_token(&mut self) -> bool {
        let refresh_token = match crate::auth::read_saved_refresh_token() {
            Some(rt) => rt,
            None => return false,
        };

        let url = format!("{}/auth/refresh", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()
            .await;

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            _ => return false,
        };

        let tokens: RefreshResponse = match resp.json().await {
            Ok(t) => t,
            Err(_) => return false,
        };

        if crate::auth::save_tokens(&tokens.access_token, Some(&tokens.refresh_token)).is_err() {
            return false;
        }

        self.access_token = tokens.access_token;
        true
    }

    pub async fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .with_context(|| format!("GET {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .get(&url)
                .bearer_auth(&self.access_token)
                .send()
                .await
                .with_context(|| format!("GET {path} failed (retry)"))?;
            return Self::handle_response(resp, path).await;
        }

        Self::handle_response(resp, path).await
    }

    pub async fn get_value(&mut self, path: &str) -> Result<serde_json::Value> {
        self.get(path).await
    }

    /// GET that returns `Ok(None)` on 404 instead of an error.
    pub async fn get_optional<T: DeserializeOwned>(&mut self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{path}", self.base_url);
        let mut resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .with_context(|| format!("GET {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            resp = self
                .client
                .get(&url)
                .bearer_auth(&self.access_token)
                .send()
                .await
                .with_context(|| format!("GET {path} failed (retry)"))?;
        }

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::handle_response(resp, path).await.map(Some)
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.access_token)
                .json(body)
                .send()
                .await
                .with_context(|| format!("POST {path} failed (retry)"))?;
            return Self::handle_response(resp, path).await;
        }

        Self::handle_response(resp, path).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&self.access_token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("PUT {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .put(&url)
                .bearer_auth(&self.access_token)
                .json(body)
                .send()
                .await
                .with_context(|| format!("PUT {path} failed (retry)"))?;
            return Self::handle_response(resp, path).await;
        }

        Self::handle_response(resp, path).await
    }

    #[allow(dead_code)]
    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&self.access_token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("PATCH {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .patch(&url)
                .bearer_auth(&self.access_token)
                .json(body)
                .send()
                .await
                .with_context(|| format!("PATCH {path} failed (retry)"))?;
            return Self::handle_response(resp, path).await;
        }

        Self::handle_response(resp, path).await
    }

    pub async fn delete_empty(&mut self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .with_context(|| format!("DELETE {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .delete(&url)
                .bearer_auth(&self.access_token)
                .send()
                .await
                .with_context(|| format!("DELETE {path} failed (retry)"))?;
            if resp.status().is_success() {
                return Ok(());
            }
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("DELETE {path} failed (HTTP {status}): {body}");
        }

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("DELETE {path} failed (HTTP {status}): {body}");
        }
    }

    #[allow(dead_code)]
    pub async fn post_empty<B: Serialize>(&mut self, path: &str, body: &B) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.access_token)
                .json(body)
                .send()
                .await
                .with_context(|| format!("POST {path} failed (retry)"))?;
            if resp.status().is_success() {
                return Ok(());
            }
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {path} failed (HTTP {status}): {body}");
        }

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {path} failed (HTTP {status}): {body}");
        }
    }

    pub async fn proxy_request(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&str>,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{path}", self.base_url);
        let method_parsed = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .with_context(|| format!("Invalid HTTP method: {method}"))?;

        let has_content_type = headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        let client = self.client.clone();
        let build_req = |token: &str| {
            let mut req = client
                .request(method_parsed.clone(), &url)
                .bearer_auth(token);
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
            if let Some(b) = body {
                if !has_content_type {
                    req = req.header("content-type", "application/json");
                }
                req = req.body(b.to_string());
            }
            req
        };

        let original_token = self.access_token.clone();

        let resp = build_req(&original_token)
            .send()
            .await
            .with_context(|| format!("Proxy request to {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Drain the auth error body before refreshing so reqwest can reuse
            // the connection for the refresh + retry sequence.
            let _ = resp.bytes().await;

            if self.try_refresh_token().await {
                return build_req(&self.access_token)
                    .send()
                    .await
                    .with_context(|| format!("Proxy request to {path} failed (retry)"));
            }

            // If refresh failed, replay the original unauthorized request so
            // callers still receive the backend's 401 payload.
            return build_req(&original_token)
                .send()
                .await
                .with_context(|| format!("Proxy request to {path} failed (unauthorized replay)"));
        }

        Ok(resp)
    }

    async fn handle_response<T: DeserializeOwned>(
        resp: reqwest::Response,
        path: &str,
    ) -> Result<T> {
        if resp.status().is_success() {
            resp.json()
                .await
                .with_context(|| format!("Failed to parse response from {path}"))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("{path} failed (HTTP {status}): {body}");
        }
    }
}

/// Make an unauthenticated POST request and return parsed JSON.
pub async fn anonymous_post(
    base_url: &str,
    path: &str,
    body: &impl Serialize,
) -> Result<serde_json::Value> {
    let base = format!("{}/api/v1", base_url.trim_end_matches('/'));
    let url = format!("{base}{path}");
    let client = build_cli_http_client()?;

    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {path} failed"))?;

    if resp.status().is_success() {
        resp.json()
            .await
            .with_context(|| format!("Failed to parse response from {path}"))
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("{path} failed (HTTP {status}): {body}");
    }
}

/// Make an unauthenticated POST request that returns no body.
pub async fn anonymous_post_empty(base_url: &str, path: &str, body: &impl Serialize) -> Result<()> {
    let base = format!("{}/api/v1", base_url.trim_end_matches('/'));
    let url = format!("{base}{path}");
    let client = build_cli_http_client()?;

    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {path} failed"))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("{path} failed (HTTP {status}): {body}");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::sync::{Mutex, OnceLock};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::ApiClient;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct HomeGuard {
        previous: Option<String>,
    }

    impl HomeGuard {
        fn set(path: &std::path::Path) -> Self {
            let previous = std::env::var("HOME").ok();
            // SAFETY: these tests serialize HOME mutations with `env_lock`.
            unsafe {
                std::env::set_var("HOME", path);
            }
            Self { previous }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            // SAFETY: these tests serialize HOME mutations with `env_lock`.
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var("HOME", previous);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // intentional: serialises HOME mutations across tests
    async fn proxy_request_refreshes_expired_token_and_retries() {
        let _env_guard = env_lock().lock().expect("env lock");
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = HomeGuard::set(temp.path());

        crate::auth::save_tokens("expired-access-token", Some("valid-refresh-token"))
            .expect("save initial tokens");

        let base_url = start_proxy_refresh_test_server()
            .await
            .expect("test server");
        let mut api =
            ApiClient::new(&base_url, "expired-access-token".to_string()).expect("api client");

        let resp = api
            .proxy_request("GET", "/proxy/s/home-assistant/api/", &[], None)
            .await
            .expect("proxy response");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        assert_eq!(
            resp.text().await.expect("response body"),
            r#"{"message":"API running."}"#
        );
        assert_eq!(
            crate::auth::read_saved_token().as_deref(),
            Some("fresh-access-token")
        );
        assert_eq!(
            crate::auth::read_saved_refresh_token().as_deref(),
            Some("fresh-refresh-token")
        );
    }

    async fn start_proxy_refresh_test_server() -> io::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };

            handle_proxy_refresh_sequence(&mut socket)
                .await
                .expect("proxy refresh sequence");
        });

        Ok(format!("http://{addr}"))
    }

    async fn handle_proxy_refresh_sequence(socket: &mut TcpStream) -> io::Result<()> {
        let first = read_request(socket).await?;
        assert_eq!(first.method, "GET");
        assert_eq!(first.path, "/api/v1/proxy/s/home-assistant/api/");
        assert_eq!(
            first.headers.get("authorization").map(String::as_str),
            Some("Bearer expired-access-token")
        );
        write_response(
            socket,
            401,
            "Unauthorized",
            r#"{"error":"token_expired","error_code":2001,"message":"Token expired"}"#,
        )
        .await?;

        let second = read_request(socket).await?;
        assert_eq!(second.method, "POST");
        assert_eq!(second.path, "/api/v1/auth/refresh");
        assert!(
            second
                .body
                .contains(r#""refresh_token":"valid-refresh-token""#),
            "refresh body: {}",
            second.body
        );
        write_response(
            socket,
            200,
            "OK",
            r#"{"access_token":"fresh-access-token","expires_in":900,"refresh_token":"fresh-refresh-token"}"#,
        )
        .await?;

        let third = read_request(socket).await?;
        assert_eq!(third.method, "GET");
        assert_eq!(third.path, "/api/v1/proxy/s/home-assistant/api/");
        assert_eq!(
            third.headers.get("authorization").map(String::as_str),
            Some("Bearer fresh-access-token")
        );
        write_response(socket, 200, "OK", r#"{"message":"API running."}"#).await
    }

    struct ParsedRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: String,
    }

    async fn read_request(socket: &mut TcpStream) -> io::Result<ParsedRequest> {
        let mut buf = Vec::new();
        let header_end = loop {
            let mut chunk = [0u8; 1024];
            let n = socket.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "socket closed before request completed",
                ));
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = find_header_end(&buf) {
                break end;
            }
        };

        let header_text = String::from_utf8_lossy(&buf[..header_end]);
        let mut lines = header_text.split("\r\n");
        let request_line = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
            .to_string();
        let path = request_parts
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing path"))?
            .to_string();

        let mut headers = HashMap::new();
        let mut content_length = 0usize;
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(name, value);
        }

        while buf.len() - header_end < content_length {
            let mut chunk = vec![0u8; content_length - (buf.len() - header_end)];
            let n = socket.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "socket closed before request body completed",
                ));
            }
            buf.extend_from_slice(&chunk[..n]);
        }

        let body =
            String::from_utf8_lossy(&buf[header_end..header_end + content_length]).to_string();

        Ok(ParsedRequest {
            method,
            path,
            headers,
            body,
        })
    }

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|idx| idx + 4)
    }

    async fn write_response(
        socket: &mut TcpStream,
        status: u16,
        reason: &str,
        body: &str,
    ) -> io::Result<()> {
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: keep-alive\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await
    }
}
