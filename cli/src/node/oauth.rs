//! Node-native OAuth flow: device code and token refresh.
//!
//! Fetches OAuth config from NyxID catalog or uses CLI-provided URLs.
//! Runs the flow, stores tokens locally, never sends them to NyxID.

use std::collections::HashMap;

use serde::Deserialize;

use super::error::{Error, Result};

/// OAuth config fetched from NyxID catalog or provided via CLI.
#[allow(dead_code)]
pub struct OAuthConfig {
    pub authorization_url: Option<String>,
    pub token_url: String,
    pub device_code_url: Option<String>,
    pub device_verification_url: Option<String>,
    pub device_token_url: Option<String>,
    pub default_scopes: Vec<String>,
    pub supports_pkce: bool,
    pub device_code_format: String, // "rfc8628" | "openai"
    pub token_endpoint_auth_method: String,
    pub extra_auth_params: Option<HashMap<String, String>>,
}

/// Token response from OAuth token endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// Device code response (RFC 8628).
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: Option<String>,
    verification_url: Option<String>,
    expires_in: u64,
    #[serde(deserialize_with = "deserialize_interval")]
    interval: u64,
}

impl DeviceCodeResponse {
    fn verification_uri(&self) -> &str {
        self.verification_uri
            .as_deref()
            .or(self.verification_url.as_deref())
            .unwrap_or("(no verification URL provided)")
    }
}

fn deserialize_interval<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Number(u64),
        Str(String),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::Number(n) => Ok(n),
        StringOrNumber::Str(s) => s
            .parse()
            .map_err(|_| serde::de::Error::custom("interval must be a number")),
    }
}

/// Fetch OAuth config from NyxID catalog API.
pub async fn fetch_catalog_oauth_config(
    api_base_url: &str,
    access_token: Option<&str>,
    service_slug: &str,
) -> Result<OAuthConfig> {
    let client = reqwest::Client::new();
    let url = format!("{api_base_url}/api/v1/catalog/{service_slug}");

    let mut req = client.get(&url);
    if let Some(token) = access_token {
        req = req.bearer_auth(token);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| Error::Config(format!("Failed to fetch catalog: {e}")))?;

    if !resp.status().is_success() {
        return Err(Error::Config(format!(
            "Catalog returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Config(format!("Failed to parse catalog response: {e}")))?;

    let token_url = body["token_url"]
        .as_str()
        .ok_or_else(|| Error::Config("Catalog entry has no token_url".to_string()))?
        .to_string();

    Ok(OAuthConfig {
        authorization_url: body["authorization_url"].as_str().map(String::from),
        token_url,
        device_code_url: body["device_code_url"].as_str().map(String::from),
        device_verification_url: body["device_verification_url"].as_str().map(String::from),
        device_token_url: body["device_token_url"].as_str().map(String::from),
        default_scopes: body["default_scopes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        supports_pkce: body["supports_pkce"].as_bool().unwrap_or(false),
        device_code_format: body["device_code_format"]
            .as_str()
            .unwrap_or("rfc8628")
            .to_string(),
        token_endpoint_auth_method: body["token_endpoint_auth_method"]
            .as_str()
            .unwrap_or("client_secret_post")
            .to_string(),
        extra_auth_params: None,
    })
}

/// Run RFC 8628 device code flow.
pub async fn run_device_code_flow(
    config: &OAuthConfig,
    client_id: &str,
    client_secret: Option<&str>,
    scopes: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let device_code_url = config
        .device_code_url
        .as_deref()
        .ok_or_else(|| Error::Config("No device_code_url available".to_string()))?;

    // Step 1: Request device code
    let resp = client
        .post(device_code_url)
        .form(&[("client_id", client_id), ("scope", scopes)])
        .send()
        .await
        .map_err(|e| Error::Config(format!("Device code request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::Config(format!(
            "Device code request failed {status}: {text}"
        )));
    }

    let device_resp: DeviceCodeResponse = resp
        .json()
        .await
        .map_err(|e| Error::Config(format!("Failed to parse device code response: {e}")))?;

    // Step 2: Display code to user
    println!();
    println!("  Your code: {}", device_resp.user_code);
    println!("  Visit: {}", device_resp.verification_uri());
    println!();
    println!("  Waiting for authorization...");

    // Step 3: Poll for token
    let token_poll_url = config
        .device_token_url
        .as_deref()
        .unwrap_or(&config.token_url);

    let mut interval = std::time::Duration::from_secs(device_resp.interval.max(1));
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(device_resp.expires_in);

    loop {
        tokio::time::sleep(interval).await;

        if std::time::Instant::now() > deadline {
            return Err(Error::Config("Device code expired".to_string()));
        }

        let mut form = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &device_resp.device_code),
            ("client_id", client_id),
        ];
        if let Some(secret) = client_secret {
            form.push(("client_secret", secret));
        }

        let resp = client.post(token_poll_url).form(&form).send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let token: TokenResponse = r
                    .json()
                    .await
                    .map_err(|e| Error::Config(format!("Failed to parse token: {e}")))?;
                println!("  Authorization successful.");
                return Ok(token);
            }
            Ok(r) if r.status().as_u16() == 428 || r.status().as_u16() == 400 => {
                // authorization_pending or slow_down
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                let error = body["error"].as_str().unwrap_or("authorization_pending");
                match error {
                    "slow_down" => {
                        interval += std::time::Duration::from_secs(5);
                    }
                    "authorization_pending" => {}
                    "expired_token" => {
                        return Err(Error::Config("Device code expired".to_string()));
                    }
                    "access_denied" => {
                        return Err(Error::Config("Authorization denied".to_string()));
                    }
                    other => {
                        return Err(Error::Config(format!("OAuth error: {other}")));
                    }
                }
            }
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                return Err(Error::Config(format!("Token poll error {status}: {text}")));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token poll request failed, retrying");
            }
        }
    }
}

/// Refresh an OAuth token using refresh_token grant.
pub async fn refresh_token(
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_tok: &str,
    auth_method: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let mut req = client.post(token_url);

    match auth_method {
        "client_secret_basic" => {
            req = req.basic_auth(client_id, client_secret);
            req = req.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_tok),
            ]);
        }
        _ => {
            // client_secret_post (default)
            let mut form = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_tok),
                ("client_id", client_id),
            ];
            if let Some(secret) = client_secret {
                form.push(("client_secret", secret));
            }
            req = req.form(&form);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| Error::Config(format!("Token refresh failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::Config(format!(
            "Token refresh error {status}: {text}"
        )));
    }

    resp.json()
        .await
        .map_err(|e| Error::Config(format!("Failed to parse refresh response: {e}")))
}
