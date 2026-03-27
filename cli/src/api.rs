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

        let build_req = |client: &reqwest::Client, token: &str| {
            let mut req = client
                .request(method_parsed.clone(), &url)
                .bearer_auth(token);

            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }

            if let Some(body) = body {
                if !has_content_type {
                    req = req.header("content-type", "application/json");
                }
                req = req.body(body.to_string());
            }

            req
        };

        let resp = build_req(&self.client, &self.access_token)
            .send()
            .await
            .with_context(|| format!("Proxy request to {path} failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.try_refresh_token().await {
            return build_req(&self.client, &self.access_token)
                .send()
                .await
                .with_context(|| format!("Proxy request to {path} failed (retry)"));
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
