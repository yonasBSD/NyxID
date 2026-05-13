//! In-process response cache for the `aws_sigv4` and `gcp_service_account`
//! proxy auth methods (NyxID#716).
//!
//! AWS Cost Explorer charges $0.01 per paginated API request. A `/daily`-
//! style skill that polls cost-by-namespace from a handful of windows
//! (last 30 days, current month, previous month) at hourly intervals
//! would otherwise burn through API budget without producing fresher
//! data than the underlying ~6h billing-data latency. The cache
//! short-circuits repeat-identical proxy requests at the NyxID boundary.
//!
//! BigQuery responses are also cached even though there's no per-call
//! charge — a large billing-export query takes 5-10 seconds and the
//! same query in two consecutive `/daily` runs returns the same data.
//!
//! Lifetime:
//! - Keyed on `(auth_method, base_url, method, path+query, sha256(body))`
//!   so identical inputs from the same `UserService` collapse to one
//!   entry. Different users / services get different entries because
//!   their `base_url` is the catalog `DownstreamService.base_url`.
//! - Default TTL is 300s; tunable via `CLOUD_RESPONSE_CACHE_TTL_SECS`
//!   in `AppConfig`. Set to 0 to disable.
//! - Successful responses (2xx) only — caching 4xx/5xx would hide
//!   permission misconfigurations.
//! - In-process, no persistence. Restart drops the cache.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use reqwest::header::{HeaderName, HeaderValue};
use sha2::{Digest, Sha256};

/// Auth methods that participate in this cache. Others bypass it.
pub fn is_cacheable_auth_method(auth_method: &str) -> bool {
    matches!(auth_method, "aws_sigv4" | "gcp_service_account")
}

#[derive(Clone)]
struct CachedEntry {
    status: u16,
    headers: Vec<(String, String)>,
    body: Bytes,
    expires_at: Instant,
}

/// Thread-safe response cache shared across the proxy hot path.
#[derive(Clone)]
pub struct CloudResponseCache {
    inner: Arc<DashMap<String, CachedEntry>>,
    ttl: Duration,
}

impl CloudResponseCache {
    /// Build a cache with the given TTL. A zero TTL disables caching:
    /// `get` always returns `None` and `insert_and_replay` becomes a
    /// passthrough.
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Compute a stable cache key for a request. Body bytes are SHA256'd
    /// so the key length stays bounded; URL and method are included
    /// verbatim because they're small.
    pub fn key(
        auth_method: &str,
        base_url: &str,
        method: &str,
        path_and_query: &str,
        body: &[u8],
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body);
        let body_hash = hex::encode(hasher.finalize());
        format!("{auth_method}|{method}|{base_url}|{path_and_query}|{body_hash}")
    }

    /// Look up a non-expired entry. Returns a reqwest::Response built
    /// from the cached bytes.
    pub fn get(&self, key: &str) -> Option<reqwest::Response> {
        if self.ttl.is_zero() {
            return None;
        }
        let entry = self.inner.get(key)?;
        if entry.expires_at <= Instant::now() {
            return None;
        }
        Some(synthesize_response(&entry))
    }

    /// Buffer the response, decide whether to cache, and return a
    /// replayable `reqwest::Response` built from the same bytes the
    /// cache stored.
    ///
    /// The original response is consumed. Non-2xx responses are
    /// returned as-is and not cached. When caching is disabled (TTL=0)
    /// the body is still buffered + re-synthesized so callers see a
    /// consistent return type, but the entry is never stored.
    pub async fn insert_and_replay(
        &self,
        key: String,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                let value_str = value.to_str().ok()?.to_string();
                Some((name.as_str().to_string(), value_str))
            })
            .collect();
        let body = response.bytes().await?;

        let should_cache = !self.ttl.is_zero() && (200..300).contains(&status);
        if should_cache {
            self.inner.insert(
                key,
                CachedEntry {
                    status,
                    headers: headers.clone(),
                    body: body.clone(),
                    expires_at: Instant::now() + self.ttl,
                },
            );
        }

        Ok(synthesize_response(&CachedEntry {
            status,
            headers,
            body,
            expires_at: Instant::now(),
        }))
    }

    /// Visible size for diagnostics + tests.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

fn synthesize_response(entry: &CachedEntry) -> reqwest::Response {
    let mut builder = http::Response::builder().status(entry.status);
    for (name, value) in &entry.headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            builder = builder.header(n, v);
        }
    }
    let http_response = builder
        .body(entry.body.clone())
        .expect("status was validated on insert");
    reqwest::Response::from(http_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_body() {
        let a = CloudResponseCache::key(
            "aws_sigv4",
            "https://ce.us-east-1.amazonaws.com",
            "POST",
            "/",
            b"{}",
        );
        let b = CloudResponseCache::key(
            "aws_sigv4",
            "https://ce.us-east-1.amazonaws.com",
            "POST",
            "/",
            b"{\"q\":1}",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn key_changes_with_auth_method() {
        let a = CloudResponseCache::key("aws_sigv4", "https://x", "GET", "/", b"");
        let b = CloudResponseCache::key("gcp_service_account", "https://x", "GET", "/", b"");
        assert_ne!(a, b);
    }

    #[test]
    fn is_cacheable_recognizes_new_methods_only() {
        assert!(is_cacheable_auth_method("aws_sigv4"));
        assert!(is_cacheable_auth_method("gcp_service_account"));
        assert!(!is_cacheable_auth_method("bearer"));
        assert!(!is_cacheable_auth_method("none"));
        assert!(!is_cacheable_auth_method("token_exchange"));
    }

    #[tokio::test]
    async fn zero_ttl_disables_storage() {
        let cache = CloudResponseCache::new(0);
        let key = "test-key".to_string();

        // Build a fake reqwest::Response from raw bytes.
        let http_response = http::Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Bytes::from_static(b"{\"ok\":true}"))
            .unwrap();
        let resp = reqwest::Response::from(http_response);

        let replayed = cache.insert_and_replay(key.clone(), resp).await.unwrap();
        assert_eq!(replayed.status().as_u16(), 200);
        assert_eq!(cache.len(), 0, "zero TTL must not store entries");
        // And a subsequent get returns nothing.
        assert!(cache.get(&key).is_none());
    }

    #[tokio::test]
    async fn caches_2xx_and_returns_on_get() {
        let cache = CloudResponseCache::new(300);
        let key = CloudResponseCache::key("aws_sigv4", "https://x", "POST", "/", b"{}");

        let http_response = http::Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Bytes::from_static(b"{\"cost\":1.23}"))
            .unwrap();
        let resp = reqwest::Response::from(http_response);

        cache.insert_and_replay(key.clone(), resp).await.unwrap();
        assert_eq!(cache.len(), 1);

        let hit = cache.get(&key).expect("entry should be cached");
        assert_eq!(hit.status().as_u16(), 200);
        let body = hit.bytes().await.unwrap();
        assert_eq!(&body[..], b"{\"cost\":1.23}");
    }

    #[tokio::test]
    async fn does_not_cache_non_2xx() {
        let cache = CloudResponseCache::new(300);
        let key = CloudResponseCache::key("aws_sigv4", "https://x", "POST", "/", b"{}");

        let http_response = http::Response::builder()
            .status(403)
            .body(Bytes::from_static(b"AccessDenied"))
            .unwrap();
        let resp = reqwest::Response::from(http_response);

        let replayed = cache.insert_and_replay(key.clone(), resp).await.unwrap();
        assert_eq!(replayed.status().as_u16(), 403);
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&key).is_none());
    }
}
