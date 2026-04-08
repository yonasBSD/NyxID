//! Shared Lark / Feishu tenant token exchange with single-flight caching.
//!
//! The Lark `tenant_access_token` is obtained by POSTing `{app_id, app_secret}`
//! to `/open-apis/auth/v3/tenant_access_token/internal`. It has a ~2h TTL and
//! can be reused across all API calls for the same Lark app. Every caller that
//! needs one (the proxy's `lark_token_exchange` auth method, the channel bot
//! adapter) shares the same in-memory cache keyed by `{base_url}::{app_id}`.
//!
//! Concurrency model: a per-key `TokenSlot` holds the cached value behind an
//! `RwLock` (fast path: concurrent reads, no contention) and serializes
//! token fetches behind a separate `Mutex` (slow path: exactly one exchange
//! in flight per key, waiters double-check the cache after acquiring the
//! fetch lock so they see a freshly-populated value without re-fetching).
//!
//! The cache lives for the lifetime of the backend process. There is no
//! background sweeper and no persistence: restarting the backend pays one
//! exchange latency per distinct app on the next request.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

use crate::errors::{AppError, AppResult};

/// A single cached tenant access token with its computed expiry time.
#[derive(Clone, Debug)]
pub struct CachedTenantToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Per-key slot combining the stored value and the fetch serializer.
#[derive(Default)]
struct TokenSlot {
    cached: RwLock<Option<CachedTenantToken>>,
    fetch_lock: Mutex<()>,
}

/// Process-wide cache of tenant tokens keyed by `{base_url}::{app_id}`.
///
/// Clone is cheap -- all state lives behind `Arc`/`DashMap`.
#[derive(Default, Clone)]
pub struct TenantTokenCache {
    entries: Arc<DashMap<String, Arc<TokenSlot>>>,
}

impl TenantTokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a cached token or fetch a fresh one via `fetch_fn`.
    ///
    /// The `fetch_fn` closure is invoked at most once per concurrent burst
    /// per cache key. Other waiters block on the per-key fetch lock, then
    /// observe the fresh value through the cache without calling `fetch_fn`
    /// themselves. Cache hits bypass the fetch lock entirely.
    pub async fn get_or_fetch<F, Fut>(&self, key: &str, fetch_fn: F) -> AppResult<String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<CachedTenantToken>>,
    {
        // DashMap::entry is atomic: races between concurrent first-touches on
        // a brand-new key produce exactly one TokenSlot.
        let slot = self
            .entries
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(TokenSlot::default()))
            .clone();

        // Fast path: the vast majority of calls land here. Concurrent readers
        // on a valid cache entry don't contend with each other.
        if let Some(token) = Self::read_fresh(&slot).await {
            return Ok(token);
        }

        // Slow path: serialize fetches per key.
        let _fetch_guard = slot.fetch_lock.lock().await;

        // Double-check after acquiring the fetch lock. If another task
        // already populated the cache while we were waiting, we skip the
        // network round-trip entirely.
        if let Some(token) = Self::read_fresh(&slot).await {
            return Ok(token);
        }

        // Perform the fetch. The fetch_lock is still held, blocking any
        // other fetcher for THIS key. Other keys are unaffected.
        let fresh = fetch_fn().await?;
        let token_value = fresh.token.clone();
        *slot.cached.write().await = Some(fresh);
        Ok(token_value)
    }

    /// Read the cached token if it exists and is still considered fresh
    /// (more than 10 minutes remaining before expiry).
    async fn read_fresh(slot: &TokenSlot) -> Option<String> {
        let guard = slot.cached.read().await;
        let cached = guard.as_ref()?;
        if cached.expires_at > Utc::now() + Duration::minutes(10) {
            Some(cached.token.clone())
        } else {
            None
        }
    }

    /// Test-only: insert a cache entry directly. Used to exercise the
    /// expired/near-expiry branches without going through `fetch_fn`.
    #[cfg(test)]
    async fn insert_for_test(&self, key: &str, entry: CachedTenantToken) {
        let slot = self
            .entries
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(TokenSlot::default()))
            .clone();
        *slot.cached.write().await = Some(entry);
    }
}

/// Parse a `lark_token_exchange` credential into `(app_id, app_secret)`.
///
/// The credential is stored as JSON `{"app_id": "...", "app_secret": "..."}`
/// in `UserApiKey.credential_encrypted`. Both fields must be present and
/// non-empty; anything else is a user configuration error.
pub fn parse_tenant_credential(credential: &str) -> AppResult<(String, String)> {
    #[derive(Deserialize)]
    struct TenantCredential {
        app_id: String,
        app_secret: String,
    }

    let parsed: TenantCredential = serde_json::from_str(credential).map_err(|_| {
        AppError::BadRequest(
            "lark_token_exchange credential must be JSON '{\"app_id\":\"...\",\"app_secret\":\"...\"}'"
                .to_string(),
        )
    })?;

    if parsed.app_id.trim().is_empty() || parsed.app_secret.trim().is_empty() {
        return Err(AppError::BadRequest(
            "lark_token_exchange credential requires non-empty app_id and app_secret".to_string(),
        ));
    }

    Ok((parsed.app_id, parsed.app_secret))
}

/// Build the Lark / Feishu token exchange URL from a service base URL.
pub fn tenant_token_url(base_url: &str) -> String {
    format!(
        "{}/open-apis/auth/v3/tenant_access_token/internal",
        base_url.trim_end_matches('/')
    )
}

/// Build the cache key for a given base URL and app id.
pub fn cache_key(base_url: &str, app_id: &str) -> String {
    format!("{}::{}", base_url.trim_end_matches('/'), app_id)
}

/// Lark's token exchange response.
#[derive(Deserialize)]
struct TenantTokenResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    tenant_access_token: Option<String>,
    /// Remaining TTL in seconds. Lark returns this between 0 and ~7200.
    #[serde(default)]
    expire: Option<i64>,
}

/// Call Lark's tenant token exchange endpoint directly, without caching.
///
/// Callers usually want `get_cached_tenant_token` instead -- this is the
/// underlying fetch helper used by the cache and by tests.
pub async fn fetch_tenant_token(
    http: &reqwest::Client,
    base_url: &str,
    app_id: &str,
    app_secret: &str,
) -> AppResult<CachedTenantToken> {
    let url = tenant_token_url(base_url);
    let body = serde_json::json!({
        "app_id": app_id,
        "app_secret": app_secret,
    });

    let response: TenantTokenResponse = http
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            AppError::ChannelPlatformError(format!("Lark tenant token request failed: {e}"))
        })?
        .json()
        .await
        .map_err(|e| {
            AppError::ChannelPlatformError(format!("Lark tenant token response parse failed: {e}"))
        })?;

    if response.code != 0 {
        return Err(AppError::ChannelPlatformError(format!(
            "Lark tenant token exchange failed (code={}, msg={})",
            response.code,
            response.msg.as_deref().unwrap_or("")
        )));
    }

    let token = response.tenant_access_token.ok_or_else(|| {
        AppError::ChannelPlatformError(
            "Lark tenant token response missing tenant_access_token".to_string(),
        )
    })?;

    // Lark returns remaining TTL in seconds. Bank on a 10-minute safety
    // margin at read time (in `read_fresh`); here we just record the raw
    // expiry so the read side can make its own cutoff decision. If the
    // response omitted `expire`, fall back to 2h which is Lark's documented
    // maximum TTL.
    let ttl_secs = response.expire.unwrap_or(7200).max(60);
    let expires_at = Utc::now() + Duration::seconds(ttl_secs);

    Ok(CachedTenantToken { token, expires_at })
}

/// Get a tenant access token, using the shared cache when possible.
///
/// This is the entry point for the proxy's `lark_token_exchange` auth method
/// and for the channel bot adapter's outbound message flow.
pub async fn get_cached_tenant_token(
    cache: &TenantTokenCache,
    http: &reqwest::Client,
    base_url: &str,
    app_id: &str,
    app_secret: &str,
) -> AppResult<String> {
    let key = cache_key(base_url, app_id);
    cache
        .get_or_fetch(&key, || async {
            fetch_tenant_token(http, base_url, app_id, app_secret).await
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ─── parse_tenant_credential ──────────────────────────────────────

    #[test]
    fn parse_tenant_credential_happy_path() {
        let credential = r#"{"app_id":"cli_xxx","app_secret":"yyy"}"#;
        let (app_id, app_secret) = parse_tenant_credential(credential).unwrap();
        assert_eq!(app_id, "cli_xxx");
        assert_eq!(app_secret, "yyy");
    }

    #[test]
    fn parse_tenant_credential_rejects_non_json() {
        let err = parse_tenant_credential("not json").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("json"));
    }

    #[test]
    fn parse_tenant_credential_rejects_missing_fields() {
        assert!(parse_tenant_credential(r#"{"app_id":"x"}"#).is_err());
        assert!(parse_tenant_credential(r#"{"app_secret":"y"}"#).is_err());
    }

    #[test]
    fn parse_tenant_credential_rejects_empty_values() {
        assert!(parse_tenant_credential(r#"{"app_id":"","app_secret":"y"}"#).is_err());
        assert!(parse_tenant_credential(r#"{"app_id":"x","app_secret":"  "}"#).is_err());
    }

    #[test]
    fn tenant_token_url_normalises_trailing_slash() {
        assert_eq!(
            tenant_token_url("https://open.larksuite.com/"),
            "https://open.larksuite.com/open-apis/auth/v3/tenant_access_token/internal"
        );
        assert_eq!(
            tenant_token_url("https://open.larksuite.com"),
            "https://open.larksuite.com/open-apis/auth/v3/tenant_access_token/internal"
        );
    }

    #[test]
    fn cache_key_separates_lark_from_feishu() {
        assert_ne!(
            cache_key("https://open.larksuite.com", "cli_xxx"),
            cache_key("https://open.feishu.cn", "cli_xxx")
        );
    }

    // ─── cache + single-flight ────────────────────────────────────────

    fn fresh_token(name: &str, ttl_secs: i64) -> CachedTenantToken {
        CachedTenantToken {
            token: name.to_string(),
            expires_at: Utc::now() + Duration::seconds(ttl_secs),
        }
    }

    #[tokio::test]
    async fn cache_hit_skips_fetch() {
        let cache = TenantTokenCache::new();
        cache
            .insert_for_test("key", fresh_token("cached-token", 3600))
            .await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let token = cache
            .get_or_fetch("key", || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(fresh_token("fresh-token", 3600))
                }
            })
            .await
            .unwrap();

        assert_eq!(token, "cached-token");
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn near_expiry_triggers_refresh() {
        let cache = TenantTokenCache::new();
        // Less than 10 minutes remaining -> treated as expired for refresh.
        cache.insert_for_test("key", fresh_token("stale", 60)).await;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let token = cache
            .get_or_fetch("key", || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(fresh_token("fresh", 3600))
                }
            })
            .await
            .unwrap();

        assert_eq!(token, "fresh");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cold_cache_single_flight_with_100_concurrent_callers() {
        let cache = TenantTokenCache::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..100 {
            let cache = cache.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("shared-key", || {
                        let counter = counter.clone();
                        async move {
                            // Simulate a slow network call so concurrent
                            // callers have time to stack up on the fetch
                            // lock. If single-flight is broken, each caller
                            // races into its own fetch and the counter
                            // crosses 1.
                            counter.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Ok(fresh_token("only-fetch", 3600))
                        }
                    })
                    .await
            }));
        }

        for handle in handles {
            assert_eq!(handle.await.unwrap().unwrap(), "only-fetch");
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "fetch_fn must be called exactly once"
        );
    }

    #[tokio::test]
    async fn distinct_keys_fetch_independently() {
        let cache = TenantTokenCache::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for i in 0..100 {
            let cache = cache.clone();
            let counter = counter.clone();
            // Alternate between two keys -- each key should produce exactly
            // one fetch, so counter should land on 2.
            let key = if i % 2 == 0 { "key-a" } else { "key-b" };
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_fetch(key, || {
                        let counter = counter.clone();
                        async move {
                            counter.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                            Ok(fresh_token("token", 3600))
                        }
                    })
                    .await
            }));
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fetch_error_propagates_without_poisoning_cache() {
        let cache = TenantTokenCache::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // First call errors out.
        let counter_clone = counter.clone();
        let first = cache
            .get_or_fetch("key", || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::ChannelPlatformError("boom".to_string()))
                }
            })
            .await;
        assert!(first.is_err());

        // Second call succeeds -- the slot should not be stuck in an error
        // state; the next fetch must be invoked.
        let counter_clone = counter.clone();
        let second = cache
            .get_or_fetch("key", || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(fresh_token("recovered", 3600))
                }
            })
            .await
            .unwrap();
        assert_eq!(second, "recovered");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
