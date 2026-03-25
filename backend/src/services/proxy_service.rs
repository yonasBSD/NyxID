use std::sync::Arc;

use mongodb::bson::doc;
use reqwest::Client;
use zeroize::Zeroizing;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::provider_config::{COLLECTION_NAME as PROVIDER_CONFIGS, ProviderConfig};
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_provider_token::{
    COLLECTION_NAME as USER_PROVIDER_TOKENS, UserProviderToken,
};
use crate::models::user_service_connection::{
    COLLECTION_NAME as USER_SERVICE_CONNECTIONS, UserServiceConnection,
};
use crate::services::delegation_service::DelegatedCredential;
use crate::services::node_ws_manager::NodeWsManager;
use crate::services::{user_api_key_service, user_service_service};

/// Result of resolving a proxy target.
pub struct ProxyTarget {
    pub base_url: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub credential: String,
    pub service: DownstreamService,
}

/// Headers that are safe to forward to downstream services.
/// Uses an allowlist approach to prevent leaking sensitive headers.
const ALLOWED_FORWARD_HEADERS: &[&str] = &[
    "content-type",
    "accept",
    "accept-language",
    "accept-encoding",
    // content-length intentionally excluded: reqwest recalculates it from the
    // actual body, and forwarding the original value causes mismatches when
    // middleware or translators modify the request body.
    "user-agent",
    "x-request-id",
    "x-correlation-id",
];

/// Resolve a downstream service by its slug.
/// Returns the service document or NotFound.
pub async fn resolve_service_by_slug(
    db: &mongodb::Database,
    slug: &str,
) -> AppResult<DownstreamService> {
    db.collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find_one(doc! { "slug": slug, "is_active": true })
        .await?
        .ok_or_else(|| AppError::NotFound("Service not found".to_string()))
}

/// Resolve the downstream service and credential for a proxy request.
///
/// Enforces that the user has an active connection. For "connection" services,
/// uses the per-user credential. For "internal" services, uses the master credential.
/// Provider services are not proxyable.
pub async fn resolve_proxy_target(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    service_id: &str,
) -> AppResult<ProxyTarget> {
    // Load the downstream service
    let service = db
        .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find_one(doc! { "_id": service_id })
        .await?
        .ok_or_else(|| AppError::NotFound("Downstream service not found".to_string()))?;

    if !service.is_active {
        return Err(AppError::BadRequest("Service is inactive".to_string()));
    }

    if service.service_type != "http" {
        return Err(AppError::BadRequest(
            "SSH services are not available through the HTTP proxy".to_string(),
        ));
    }

    // Provider services cannot be proxied to
    if service.service_category == "provider" {
        return Err(AppError::BadRequest(
            "Provider services are not proxyable".to_string(),
        ));
    }

    // Check for user connection (required for credential services, optional for auto-connect)
    let user_conn = db
        .collection::<UserServiceConnection>(USER_SERVICE_CONNECTIONS)
        .find_one(doc! {
            "user_id": user_id,
            "service_id": service_id,
        })
        .await?;

    // If user has explicitly disconnected (is_active: false), block access
    if let Some(ref conn) = user_conn
        && !conn.is_active
    {
        return Err(AppError::Forbidden(
            "You have disconnected from this service".to_string(),
        ));
    }

    // For services requiring user credentials, a connection record is mandatory
    if service.requires_user_credential && user_conn.is_none() {
        return Err(AppError::Forbidden(
            "You must connect to this service before making requests".to_string(),
        ));
    }

    // No-auth services: skip credential handling entirely
    if service.auth_method == "none" {
        return Ok(ProxyTarget {
            base_url: service.base_url.clone(),
            auth_method: service.auth_method.clone(),
            auth_key_name: service.auth_key_name.clone(),
            credential: String::new(),
            service,
        });
    }

    // Determine which credential to use
    let credential_encrypted = if service.requires_user_credential {
        // Connection services: must have per-user credential
        user_conn
            .and_then(|c| c.credential_encrypted)
            .ok_or_else(|| {
                AppError::BadRequest(
                    "Connection is missing credential. Please reconnect with your API key."
                        .to_string(),
                )
            })?
    } else {
        // Internal services: use master credential
        service.credential_encrypted.clone()
    };

    // SEC-M3: Wrap raw decrypted bytes in Zeroizing so they are zeroed on drop
    let decrypted_bytes = Zeroizing::new(encryption_keys.decrypt(&credential_encrypted).await?);
    let credential = String::from_utf8((*decrypted_bytes).clone()).map_err(|e| {
        tracing::error!("Credential UTF-8 decode failed: {e}");
        AppError::Internal("Failed to decode credential".to_string())
    })?;

    let base_url = resolve_gateway_url_override(db, user_id, &service)
        .await?
        .unwrap_or_else(|| service.base_url.clone());

    Ok(ProxyTarget {
        base_url,
        auth_method: service.auth_method.clone(),
        auth_key_name: service.auth_key_name.clone(),
        credential,
        service,
    })
}

/// Resolve proxy target with lenient credential handling for node-routed requests.
///
/// Unlike `resolve_proxy_target()`, this does NOT require a connection record or
/// credential for "connection" services. Returns `(ProxyTarget, has_credential)`
/// where `has_credential` indicates whether a server-side credential was resolved
/// (i.e. standard proxy fallback is viable).
pub async fn resolve_proxy_target_lenient(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    service_id: &str,
) -> AppResult<(ProxyTarget, bool)> {
    let service = db
        .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find_one(doc! { "_id": service_id })
        .await?
        .ok_or_else(|| AppError::NotFound("Downstream service not found".to_string()))?;

    if !service.is_active {
        return Err(AppError::BadRequest("Service is inactive".to_string()));
    }

    if service.service_type != "http" {
        return Err(AppError::BadRequest(
            "SSH services are not available through the HTTP proxy".to_string(),
        ));
    }

    if service.service_category == "provider" {
        return Err(AppError::BadRequest(
            "Provider services are not proxyable".to_string(),
        ));
    }

    // No-auth services: no credential needed
    if service.auth_method == "none" {
        return Ok((
            ProxyTarget {
                base_url: service.base_url.clone(),
                auth_method: service.auth_method.clone(),
                auth_key_name: service.auth_key_name.clone(),
                credential: String::new(),
                service,
            },
            true,
        ));
    }

    // Try to resolve a credential, but don't fail if missing
    let user_conn = db
        .collection::<UserServiceConnection>(USER_SERVICE_CONNECTIONS)
        .find_one(doc! {
            "user_id": user_id,
            "service_id": service_id,
        })
        .await?;

    // Respect explicit disconnection even in lenient mode
    if let Some(ref conn) = user_conn
        && !conn.is_active
    {
        return Err(AppError::Forbidden(
            "You have disconnected from this service".to_string(),
        ));
    }

    let credential_encrypted = if service.requires_user_credential {
        user_conn.and_then(|c| c.credential_encrypted)
    } else {
        Some(service.credential_encrypted.clone())
    };

    let (credential, has_credential) = match credential_encrypted {
        Some(enc) => {
            let decrypted_bytes = Zeroizing::new(encryption_keys.decrypt(&enc).await?);
            let cred = String::from_utf8((*decrypted_bytes).clone()).map_err(|e| {
                tracing::error!("Credential UTF-8 decode failed: {e}");
                AppError::Internal("Failed to decode credential".to_string())
            })?;
            (cred, true)
        }
        None => (String::new(), false),
    };

    let base_url = resolve_gateway_url_override(db, user_id, &service)
        .await?
        .unwrap_or_else(|| service.base_url.clone());

    Ok((
        ProxyTarget {
            base_url,
            auth_method: service.auth_method.clone(),
            auth_key_name: service.auth_key_name.clone(),
            credential,
            service,
        },
        has_credential,
    ))
}

/// Result of resolving a proxy target from the UserService model.
pub struct UserServiceResolution {
    pub target: ProxyTarget,
    pub node_id: Option<String>,
    pub user_service_id: String,
}

/// Resolve proxy target from the new UserService model.
///
/// Returns `Ok(Some(UserServiceResolution))` if a UserService exists
/// for this user+slug/catalog_service_id.
/// Returns `Ok(None)` to signal the caller should fall back to old resolution.
pub async fn resolve_proxy_target_from_user_service(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    _node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    slug: Option<&str>,
    catalog_service_id: Option<&str>,
) -> AppResult<Option<UserServiceResolution>> {
    // Find the UserService
    let user_service = if let Some(slug) = slug {
        user_service_service::find_by_slug(db, user_id, slug).await?
    } else if let Some(csid) = catalog_service_id {
        user_service_service::find_by_catalog_service_id(db, user_id, csid).await?
    } else {
        return Ok(None);
    };

    let user_service = match user_service {
        Some(us) => us,
        None => return Ok(None),
    };

    // Load the endpoint
    let endpoint = db
        .collection::<UserEndpoint>(USER_ENDPOINTS)
        .find_one(doc! { "_id": &user_service.endpoint_id })
        .await?
        .ok_or_else(|| {
            tracing::error!(
                endpoint_id = %user_service.endpoint_id,
                "UserService references missing endpoint"
            );
            AppError::Internal("Data integrity error: endpoint not found".to_string())
        })?;

    // Load the api key
    let api_key = db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": &user_service.api_key_id })
        .await?
        .ok_or_else(|| {
            tracing::error!(
                api_key_id = %user_service.api_key_id,
                "UserService references missing API key"
            );
            AppError::Internal("Data integrity error: API key not found".to_string())
        })?;

    if api_key.status != "active" {
        return Err(AppError::BadRequest(format!(
            "API key is {}",
            api_key.status
        )));
    }

    // Handle no-auth services
    if user_service.auth_method == "none" {
        let now = chrono::Utc::now();
        let minimal_service = build_minimal_downstream_service(&user_service, &endpoint, now);

        return Ok(Some(UserServiceResolution {
            target: ProxyTarget {
                base_url: endpoint.url.clone(),
                auth_method: user_service.auth_method.clone(),
                auth_key_name: user_service.auth_key_name.clone(),
                credential: String::new(),
                service: minimal_service,
            },
            node_id: user_service.node_id.clone(),
            user_service_id: user_service.id.clone(),
        }));
    }

    // Node-routed services: skip credential decryption entirely.
    // The node agent injects credentials locally -- NyxID doesn't need them.
    // This covers: node_managed, ssh_certificate, and any service with node_id set.
    if user_service.node_id.is_some() {
        let now = chrono::Utc::now();
        let minimal_service = build_minimal_downstream_service(&user_service, &endpoint, now);

        return Ok(Some(UserServiceResolution {
            target: ProxyTarget {
                base_url: endpoint.url.clone(),
                auth_method: user_service.auth_method.clone(),
                auth_key_name: user_service.auth_key_name.clone(),
                credential: String::new(),
                service: minimal_service,
            },
            node_id: user_service.node_id.clone(),
            user_service_id: user_service.id.clone(),
        }));
    }

    // Direct routing: decrypt credential from NyxID storage
    // (Node-routed services are already handled above and never reach here)
    let credential = if api_key.credential_type == "oauth2" {
        let needs_refresh = api_key
            .expires_at
            .is_some_and(|exp| exp <= chrono::Utc::now() + chrono::Duration::minutes(5));

        if needs_refresh {
            tracing::debug!(api_key_id = %api_key.id, "OAuth token expired or near-expiry, attempting refresh");
        }

        let encrypted = api_key.access_token_encrypted.as_ref().ok_or_else(|| {
            AppError::BadRequest("OAuth token has no credential stored".to_string())
        })?;
        let decrypted_bytes = Zeroizing::new(encryption_keys.decrypt(encrypted).await?);
        String::from_utf8((*decrypted_bytes).clone()).map_err(|e| {
            tracing::error!("Credential UTF-8 decode failed: {e}");
            AppError::Internal("Failed to decode credential".to_string())
        })?
    } else {
        let encrypted = api_key.credential_encrypted.as_ref().ok_or_else(|| {
            AppError::BadRequest(
                "No credential stored. Add a credential or route through a node.".to_string(),
            )
        })?;
        let decrypted_bytes = Zeroizing::new(encryption_keys.decrypt(encrypted).await?);
        String::from_utf8((*decrypted_bytes).clone()).map_err(|e| {
            tracing::error!("Credential UTF-8 decode failed: {e}");
            AppError::Internal("Failed to decode credential".to_string())
        })?
    };

    // Fire-and-forget: update last_used_at
    let db_clone = db.clone();
    let key_id = api_key.id.clone();
    tokio::spawn(async move {
        user_api_key_service::touch_last_used(&db_clone, &key_id).await;
    });

    let now = chrono::Utc::now();
    let minimal_service = build_minimal_downstream_service(&user_service, &endpoint, now);

    Ok(Some(UserServiceResolution {
        target: ProxyTarget {
            base_url: endpoint.url.clone(),
            auth_method: user_service.auth_method.clone(),
            auth_key_name: user_service.auth_key_name.clone(),
            credential,
            service: minimal_service,
        },
        node_id: user_service.node_id.clone(),
        user_service_id: user_service.id.clone(),
    }))
}

/// Build a minimal DownstreamService struct for backward compatibility with
/// existing proxy pipeline code that expects a `ProxyTarget.service`.
fn build_minimal_downstream_service(
    user_service: &crate::models::user_service::UserService,
    endpoint: &UserEndpoint,
    now: chrono::DateTime<chrono::Utc>,
) -> DownstreamService {
    DownstreamService {
        id: user_service
            .catalog_service_id
            .clone()
            .unwrap_or_else(|| user_service.id.clone()),
        name: endpoint.label.clone(),
        slug: user_service.slug.clone(),
        description: None,
        base_url: endpoint.url.clone(),
        service_type: "http".to_string(),
        visibility: "public".to_string(),
        auth_method: user_service.auth_method.clone(),
        auth_key_name: user_service.auth_key_name.clone(),
        credential_encrypted: vec![],
        auth_type: None,
        openapi_spec_url: None,
        asyncapi_spec_url: None,
        streaming_supported: true,
        ssh_config: None,
        oauth_client_id: None,
        service_category: "connection".to_string(),
        requires_user_credential: true,
        is_active: true,
        created_by: "system".to_string(),
        identity_propagation_mode: "none".to_string(),
        identity_include_user_id: false,
        identity_include_email: false,
        identity_include_name: false,
        identity_jwt_audience: None,
        inject_delegation_token: false,
        delegation_token_scope: "llm:proxy".to_string(),
        provider_config_id: None,
        created_at: now,
        updated_at: now,
    }
}

/// For services linked to a provider with `requires_gateway_url`, look up the
/// user's provider token and return their per-user gateway URL.
///
/// Returns `Ok(None)` for providers that don't require a gateway URL.
/// Returns `Err` if the provider requires a gateway URL but the user hasn't
/// connected one -- this prevents fallback to the placeholder base_url.
async fn resolve_gateway_url_override(
    db: &mongodb::Database,
    user_id: &str,
    service: &DownstreamService,
) -> AppResult<Option<String>> {
    let provider_config_id = match &service.provider_config_id {
        Some(id) => id,
        None => return Ok(None),
    };

    let provider = db
        .collection::<ProviderConfig>(PROVIDER_CONFIGS)
        .find_one(doc! { "_id": provider_config_id })
        .await?;

    let provider = match provider {
        Some(p) if p.requires_gateway_url => p,
        _ => return Ok(None),
    };

    let user_token = db
        .collection::<UserProviderToken>(USER_PROVIDER_TOKENS)
        .find_one(doc! {
            "user_id": user_id,
            "provider_config_id": &provider.id,
            "status": "active",
        })
        .await?;

    match user_token.and_then(|t| t.gateway_url) {
        Some(url) if !url.is_empty() => Ok(Some(url)),
        _ => Err(AppError::BadRequest(format!(
            "Connect your {} instance first (provide your gateway URL in Providers)",
            provider.name
        ))),
    }
}

/// Forward a request to the downstream service with credential injection,
/// identity propagation headers, and delegated provider credentials.
///
/// Uses an allowlist for headers to prevent leaking sensitive data.
/// Preserves the original HTTP method for all auth methods including query auth.
#[allow(clippy::too_many_arguments)]
pub async fn forward_request(
    client: &Client,
    target: &ProxyTarget,
    method: reqwest::Method,
    path: &str,
    query: Option<&str>,
    headers: reqwest::header::HeaderMap,
    body: Option<bytes::Bytes>,
    identity_headers: Vec<(String, String)>,
    delegated_credentials: Vec<DelegatedCredential>,
) -> AppResult<reqwest::Response> {
    // SEC-H3: Reject paths containing traversal sequences
    if path.contains("..") || path.contains("//") {
        return Err(AppError::BadRequest("Invalid proxy path".to_string()));
    }

    // TODO(SEC-H1): Re-validate the resolved IP at proxy time to prevent DNS rebinding.
    // Currently base_url is only validated at service creation/update time. An attacker
    // could change DNS to point to a private IP after validation. Consider using a custom
    // DNS resolver or reqwest's `resolve` feature to check the resolved IP before connecting.

    let url = if let Some(q) = query {
        format!(
            "{}/{}?{}",
            target.base_url.trim_end_matches('/'),
            path.trim_start_matches('/'),
            q
        )
    } else {
        format!(
            "{}/{}",
            target.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    };

    let mut request = client.request(method.clone(), &url);

    // Copy only allowed headers (allowlist approach)
    for (name, value) in headers.iter() {
        let name_lower = name.as_str().to_lowercase();
        if ALLOWED_FORWARD_HEADERS.contains(&name_lower.as_str()) {
            request = request.header(name, value);
        }
    }

    // Inject identity propagation headers
    for (name, value) in &identity_headers {
        request = request.header(name, value);
    }

    // Inject credentials based on auth method
    match target.auth_method.as_str() {
        "none" => {
            // No credential injection
        }
        "header" => {
            request = request.header(&target.auth_key_name, &target.credential);
        }
        "bearer" => {
            request = request.bearer_auth(&target.credential);
        }
        "query" => {
            // Use the request builder's query method to properly URL-encode parameters.
            // This preserves the original HTTP method, headers, and body.
            request = request.query(&[(&target.auth_key_name, &target.credential)]);
        }
        "basic" => {
            // credential format: "username:password"
            let parts: Vec<&str> = target.credential.splitn(2, ':').collect();
            if parts.len() == 2 {
                request = request.basic_auth(parts[0], Some(parts[1]));
            } else {
                return Err(AppError::Internal(
                    "Basic auth credential must be in 'username:password' format".to_string(),
                ));
            }
        }
        _ => {
            return Err(AppError::Internal(format!(
                "Unknown auth method: {}",
                target.auth_method
            )));
        }
    }

    // Inject delegated provider credentials
    for cred in &delegated_credentials {
        match cred.injection_method.as_str() {
            "bearer" => {
                request =
                    request.header(&cred.injection_key, format!("Bearer {}", cred.credential));
            }
            "header" => {
                request = request.header(&cred.injection_key, &cred.credential);
            }
            "query" => {
                request = request.query(&[(&cred.injection_key, &cred.credential)]);
            }
            _ => {}
        }
    }

    if let Some(ref body_bytes) = body {
        // Log request body for LLM proxy calls to diagnose truncation issues
        if url.contains("/responses") {
            let body_str = String::from_utf8_lossy(body_bytes);
            let preview = if body_str.len() > 2048 {
                // Find a safe char boundary at or before 2048 bytes
                let mut end = 2048;
                while end > 0 && !body_str.is_char_boundary(end) {
                    end -= 1;
                }
                format!(
                    "{}...(truncated, total {} bytes)",
                    &body_str[..end],
                    body_str.len()
                )
            } else {
                body_str.to_string()
            };
            tracing::info!(
                url = %url,
                body_len = body_bytes.len(),
                body = %preview,
                "Proxy LLM request body"
            );
        }
    }

    if let Some(body_bytes) = body {
        request = request.body(body_bytes);
    }

    let response = request.send().await.map_err(|e| {
        tracing::error!("Proxy request to {} failed: {e}", target.base_url);
        AppError::Internal("Proxy request failed".to_string())
    })?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use chrono::Utc;
    use tokio::{net::TcpListener, sync::mpsc};

    #[derive(Debug)]
    struct CapturedRequest {
        content_type: Option<String>,
        body: Vec<u8>,
    }

    async fn capture_request(
        State(sender): State<mpsc::UnboundedSender<CapturedRequest>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        let _ = sender.send(CapturedRequest {
            content_type: headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
            body: body.to_vec(),
        });

        StatusCode::OK
    }

    fn make_proxy_target(base_url: String) -> ProxyTarget {
        let now = Utc::now();
        ProxyTarget {
            base_url: base_url.clone(),
            auth_method: "none".to_string(),
            auth_key_name: "Authorization".to_string(),
            credential: String::new(),
            service: DownstreamService {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Upload Service".to_string(),
                slug: "upload-service".to_string(),
                description: Some("Receives binary uploads".to_string()),
                base_url,
                auth_method: "none".to_string(),
                auth_key_name: "Authorization".to_string(),
                credential_encrypted: vec![],
                auth_type: None,
                openapi_spec_url: None,
                asyncapi_spec_url: None,
                streaming_supported: false,
                ssh_config: None,
                service_type: "http".to_string(),
                visibility: "public".to_string(),
                oauth_client_id: None,
                service_category: "internal".to_string(),
                requires_user_credential: false,
                is_active: true,
                created_by: "test".to_string(),
                identity_propagation_mode: "none".to_string(),
                identity_include_user_id: false,
                identity_include_email: false,
                identity_include_name: false,
                identity_jwt_audience: None,
                inject_delegation_token: false,
                delegation_token_scope: "llm:proxy".to_string(),
                provider_config_id: None,
                created_at: now,
                updated_at: now,
            },
        }
    }

    #[tokio::test]
    async fn forward_request_preserves_binary_body_and_content_type() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let app = Router::new()
            .route("/upload", post(capture_request))
            .with_state(sender);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test app");
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/zip".parse().unwrap(),
        );
        let response = forward_request(
            &Client::new(),
            &make_proxy_target(format!("http://{addr}")),
            reqwest::Method::POST,
            "upload",
            None,
            headers,
            Some(bytes::Bytes::from_static(b"PK\x03\x04")),
            vec![],
            vec![],
        )
        .await
        .expect("proxy request should succeed");

        assert_eq!(response.status(), StatusCode::OK);

        let captured = receiver.recv().await.expect("captured request");
        assert_eq!(captured.content_type.as_deref(), Some("application/zip"));
        assert_eq!(captured.body, b"PK\x03\x04");

        server.abort();
    }
}
