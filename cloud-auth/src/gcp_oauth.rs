//! GCP service-account JWT-bearer OAuth flow with an in-process token cache.
//!
//! Spec: <https://datatracker.ietf.org/doc/html/rfc7523> plus Google's specific
//! flow at <https://developers.google.com/identity/protocols/oauth2/service-account>.
//!
//! Both NyxID backend and the node agent embed one [`GcpTokenCache`] and call
//! [`GcpTokenCache::access_token`] per outbound request. Cached tokens are
//! refreshed in-place a few minutes before expiry; concurrent callers on a
//! cold-miss key all wait on the same mint via a per-key mutex.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::error::{CloudAuthError, CloudAuthResult};

/// Default OAuth scope for proxying GCP Cloud Billing + BigQuery requests.
///
/// Cloud Billing API accepts the broad `cloud-platform` scope; BigQuery
/// requires the more specific `bigquery` scope. We request both so a single
/// cached token can serve either downstream — the alternative is two
/// separate cache entries per credential, and these requests share the same
/// service account anyway.
pub const DEFAULT_GCP_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/bigquery",
];

/// Decoded `gcp_service_account` credential payload.
///
/// Stored on `UserApiKey.credential_encrypted` as the raw service-account
/// JSON file content emitted by `gcloud iam service-accounts keys create`
/// (or the Cloud Console "Create Key" flow).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GcpServiceAccountKey {
    #[serde(rename = "type")]
    pub key_type: String,
    pub project_id: String,
    pub private_key_id: String,
    /// PEM-encoded RSA private key. Newlines come through as `\n` in the
    /// JSON; `jsonwebtoken::EncodingKey::from_rsa_pem` handles that.
    pub private_key: String,
    pub client_email: String,
    /// OAuth token endpoint, usually `https://oauth2.googleapis.com/token`.
    /// Falls back to the standard endpoint if the JSON omits it.
    #[serde(default)]
    pub token_uri: Option<String>,
}

impl GcpServiceAccountKey {
    pub fn from_json(raw: &str) -> CloudAuthResult<Self> {
        let trimmed = raw.trim();
        let key: Self = serde_json::from_str(trimmed).map_err(|e| {
            CloudAuthError::InvalidCredential(format!(
                "gcp_service_account credential must be the raw service account JSON: {}",
                e
            ))
        })?;
        if key.key_type != "service_account" {
            return Err(CloudAuthError::InvalidCredential(format!(
                "expected type=service_account, got '{}'",
                key.key_type
            )));
        }
        if key.client_email.is_empty() || key.private_key.is_empty() {
            return Err(CloudAuthError::InvalidCredential(
                "service account JSON missing client_email or private_key".to_string(),
            ));
        }
        Ok(key)
    }

    pub fn token_endpoint(&self) -> &str {
        self.token_uri
            .as_deref()
            .unwrap_or("https://oauth2.googleapis.com/token")
    }
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    scope: String,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Lifetime in seconds. Google returns 3599 today; we treat as advisory.
    expires_in: u64,
}

#[derive(Clone)]
struct CachedToken {
    token: Arc<str>,
    /// Unix seconds at which the token expires.
    expires_at: u64,
}

/// Thread-safe access-token cache.
///
/// Cache entries are keyed by `(SHA256(credential_json), sorted_scopes)`
/// so the same SA used for two different scope sets gets two cache slots.
/// Per-key minting is serialized via a `tokio::sync::Mutex` to avoid the
/// thundering-herd minting on cold start.
#[derive(Clone, Default)]
pub struct GcpTokenCache {
    inner: Arc<GcpTokenCacheInner>,
}

#[derive(Default)]
struct GcpTokenCacheInner {
    tokens: DashMap<String, CachedToken>,
    /// Per-cache-key mint locks. Separate from `tokens` because the
    /// `DashMap` shard lock is too coarse to hold across the awaiting
    /// reqwest call.
    locks: DashMap<String, Arc<Mutex<()>>>,
}

impl GcpTokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a non-expired access token for `(credential_json, scopes)`,
    /// minting and caching one if needed.
    ///
    /// `scopes` is normalized (sorted + deduped) before being incorporated
    /// into the cache key so equivalent scope sets hit the same slot
    /// regardless of order.
    pub async fn access_token(
        &self,
        client: &reqwest::Client,
        credential_json: &str,
        scopes: &[&str],
    ) -> CloudAuthResult<Arc<str>> {
        let mut scope_vec: Vec<&str> = scopes.to_vec();
        scope_vec.sort();
        scope_vec.dedup();
        let scopes_joined = scope_vec.join(" ");
        let cache_key = cache_key(credential_json, &scopes_joined);

        // Fast path: cached and still valid.
        if let Some(entry) = self.inner.tokens.get(&cache_key)
            && entry.expires_at > now_unix() + SAFETY_SECS
        {
            return Ok(entry.token.clone());
        }

        // Slow path: serialize minting on this cache key.
        let mint_lock = self
            .inner
            .locks
            .entry(cache_key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = mint_lock.lock().await;

        // Re-check after acquiring the lock; another task may have minted
        // while we were waiting.
        if let Some(entry) = self.inner.tokens.get(&cache_key)
            && entry.expires_at > now_unix() + SAFETY_SECS
        {
            return Ok(entry.token.clone());
        }

        let key = GcpServiceAccountKey::from_json(credential_json)?;
        let cached = mint_access_token(client, &key, &scopes_joined).await?;
        let token = cached.token.clone();
        self.inner.tokens.insert(cache_key, cached);
        Ok(token)
    }

    /// Drops any cached token for the given credential + scopes. Useful
    /// when an upstream call returns 401 and we want the next request to
    /// re-mint without waiting for natural expiry.
    pub fn invalidate(&self, credential_json: &str, scopes: &[&str]) {
        let mut scope_vec: Vec<&str> = scopes.to_vec();
        scope_vec.sort();
        scope_vec.dedup();
        let key = cache_key(credential_json, &scope_vec.join(" "));
        self.inner.tokens.remove(&key);
    }
}

const SAFETY_SECS: u64 = 300; // refresh when within 5 min of expiry

fn cache_key(credential_json: &str, scopes: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(credential_json.as_bytes());
    hasher.update(b"|");
    hasher.update(scopes.as_bytes());
    hex::encode(hasher.finalize())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn mint_access_token(
    client: &reqwest::Client,
    key: &GcpServiceAccountKey,
    scope: &str,
) -> CloudAuthResult<CachedToken> {
    let token_uri = key.token_endpoint().to_string();
    let iat = now_unix();
    let exp = iat + 3600;
    let claims = JwtClaims {
        iss: &key.client_email,
        sub: &key.client_email,
        aud: &token_uri,
        scope: scope.to_string(),
        iat,
        exp,
    };

    let encoding_key = EncodingKey::from_rsa_pem(key.private_key.as_bytes()).map_err(|e| {
        CloudAuthError::InvalidCredential(format!(
            "private_key is not valid PEM-encoded RSA: {}",
            e
        ))
    })?;
    let header = Header::new(jsonwebtoken::Algorithm::RS256);
    let assertion = jsonwebtoken::encode(&header, &claims, &encoding_key)
        .map_err(|e| CloudAuthError::Signing(format!("JWT sign failed: {}", e)))?;

    let form = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", assertion.as_str()),
    ];

    let response = client
        .post(&token_uri)
        .form(&form)
        .send()
        .await
        .map_err(|e| CloudAuthError::Network(format!("POST {} failed: {}", token_uri, e)))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(CloudAuthError::UpstreamError {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: TokenResponse = response.json().await?;
    let expires_at = now_unix()
        .saturating_add(parsed.expires_in)
        .max(now_unix() + Duration::from_secs(60).as_secs());

    Ok(CachedToken {
        token: Arc::from(parsed.access_token.into_boxed_str()),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SA: &str = r#"{
        "type": "service_account",
        "project_id": "test-project",
        "private_key_id": "abc123",
        "private_key": "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEAxx==\n-----END RSA PRIVATE KEY-----\n",
        "client_email": "sa@test-project.iam.gserviceaccount.com",
        "client_id": "112233"
    }"#;

    #[test]
    fn parses_service_account_json() {
        let key = GcpServiceAccountKey::from_json(SAMPLE_SA).expect("parse");
        assert_eq!(key.client_email, "sa@test-project.iam.gserviceaccount.com");
        assert_eq!(key.project_id, "test-project");
        assert_eq!(key.token_endpoint(), "https://oauth2.googleapis.com/token");
    }

    #[test]
    fn rejects_non_service_account_type() {
        let raw = SAMPLE_SA.replace("service_account", "user_account");
        let err = GcpServiceAccountKey::from_json(&raw).unwrap_err();
        assert!(matches!(err, CloudAuthError::InvalidCredential(_)));
    }

    #[test]
    fn rejects_missing_private_key() {
        let raw = r#"{"type":"service_account","project_id":"p","private_key_id":"k","private_key":"","client_email":"sa@p.iam.gserviceaccount.com"}"#;
        let err = GcpServiceAccountKey::from_json(raw).unwrap_err();
        assert!(matches!(err, CloudAuthError::InvalidCredential(_)));
    }

    #[test]
    fn cache_key_is_deterministic_and_scope_order_independent() {
        let k1 = cache_key("creds", "a b c");
        let k2 = cache_key("creds", "a b c");
        assert_eq!(k1, k2);
        let k3 = cache_key("creds", "c b a");
        assert_ne!(k1, k3, "cache_key is raw — caller must sort scopes first");
    }

    #[test]
    fn custom_token_uri_is_honored() {
        let raw = r#"{
            "type": "service_account",
            "project_id": "p",
            "private_key_id": "k",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nX\n-----END RSA PRIVATE KEY-----\n",
            "client_email": "sa@p.iam.gserviceaccount.com",
            "token_uri": "https://custom.example.com/token"
        }"#;
        let key = GcpServiceAccountKey::from_json(raw).expect("parse");
        assert_eq!(key.token_endpoint(), "https://custom.example.com/token");
    }
}
