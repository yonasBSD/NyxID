use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::AppResult;
use crate::mw::auth::AuthUser;
use crate::services::{
    credential_push_service, unified_key_service, user_api_key_service, user_endpoint_service,
    user_service_service,
};

#[derive(Deserialize, ToSchema)]
pub struct CreateKeyRequest {
    /// Catalog service slug (e.g., "llm-openai").
    pub service_slug: Option<String>,
    /// The credential value (API key, bearer token, etc.)
    /// Optional: not needed when routing via node (node manages credentials)
    pub credential: Option<String>,
    /// User-facing label
    pub label: String,
    /// Endpoint URL override (required for self-hosted providers and custom endpoints)
    pub endpoint_url: Option<String>,
    /// Custom slug (required when service_slug is None)
    pub slug: Option<String>,
    /// Custom auth method (default: "bearer")
    pub auth_method: Option<String>,
    /// Custom auth key name (default: "Authorization")
    pub auth_key_name: Option<String>,
    /// Route through this node agent (optional)
    pub node_id: Option<String>,
    /// SSH host (required for custom SSH services)
    pub ssh_host: Option<String>,
    /// SSH port (default: 22)
    pub ssh_port: Option<u16>,
    /// Enable SSH certificate auth (default: true)
    pub ssh_certificate_auth: Option<bool>,
    /// Comma-separated allowed principals
    pub ssh_principals: Option<String>,
    /// Certificate TTL in minutes (default: 30)
    pub ssh_certificate_ttl_minutes: Option<u32>,
}

impl std::fmt::Debug for CreateKeyRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateKeyRequest")
            .field("service_slug", &self.service_slug)
            .field("credential", &"[REDACTED]")
            .field("label", &self.label)
            .field("endpoint_url", &self.endpoint_url)
            .field("slug", &self.slug)
            .field("auth_method", &self.auth_method)
            .field("auth_key_name", &self.auth_key_name)
            .field("node_id", &self.node_id)
            .field("ssh_host", &self.ssh_host)
            .field("ssh_port", &self.ssh_port)
            .field("ssh_certificate_auth", &self.ssh_certificate_auth)
            .field("ssh_principals", &self.ssh_principals)
            .field(
                "ssh_certificate_ttl_minutes",
                &self.ssh_certificate_ttl_minutes,
            )
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct KeyResponse {
    pub id: String,
    pub label: String,
    pub slug: String,
    pub endpoint_url: String,
    pub endpoint_id: String,
    pub api_key_id: String,
    pub credential_type: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub service_type: String,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    // SSH fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_ca_public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_allowed_principals: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_certificate_ttl_minutes: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct KeyListResponse {
    pub keys: Vec<KeyResponse>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateKeyRequest {
    /// New display label
    pub label: Option<String>,
    /// New endpoint URL
    pub endpoint_url: Option<String>,
    /// Auth method (bearer, header, query, basic, none)
    pub auth_method: Option<String>,
    /// Auth key name (e.g., Authorization)
    pub auth_key_name: Option<String>,
    /// Node ID for routing ("" to clear, Some(id) to set)
    pub node_id: Option<String>,
    /// Activate or deactivate
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteKeyResponse {
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/keys",
    request_body = CreateKeyRequest,
    responses(
        (status = 200, description = "Key created with auto-provisioned endpoint, credential, and service", body = KeyResponse),
        (status = 400, description = "Validation error", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Catalog entry not found", body = crate::errors::ErrorResponse)
    ),
    tag = "AI Services"
)]
/// POST /api/v1/keys
pub async fn create_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateKeyRequest>,
) -> AppResult<Json<KeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let credential = body.credential.as_deref().unwrap_or("");

    // Build SSH params if SSH-specific fields are present
    let ssh_params = body.ssh_host.as_deref().map(|host| {
        let principals_str = body.ssh_principals.as_deref().unwrap_or("");
        let principals: Vec<String> = principals_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        unified_key_service::SshCreateParams {
            host,
            port: body.ssh_port.unwrap_or(22),
            certificate_auth: body.ssh_certificate_auth.unwrap_or(true),
            principals,
            certificate_ttl_minutes: body.ssh_certificate_ttl_minutes.unwrap_or(30),
        }
    });

    let result = unified_key_service::create_key(
        &state.db,
        &state.encryption_keys,
        &user_id_str,
        body.service_slug.as_deref(),
        body.endpoint_url.as_deref(),
        credential,
        &body.label,
        body.slug.as_deref(),
        body.auth_method.as_deref(),
        body.auth_key_name.as_deref(),
        body.node_id.as_deref(),
        ssh_params,
    )
    .await?;

    // Fire-and-forget: push credential to node if routed AND we have a credential to push
    let has_pushable_credential = result.api_key.credential_encrypted.is_some()
        || result.api_key.access_token_encrypted.is_some();
    if result.service.node_id.is_some() && has_pushable_credential {
        let db = state.db.clone();
        let enc = state.encryption_keys.clone();
        let ws = state.node_ws_manager.clone();
        let uid = user_id_str.clone();
        let key_id = result.api_key.id.clone();
        tokio::spawn(async move {
            credential_push_service::push_credential_to_node_if_routed(
                &db, &enc, &ws, &uid, &key_id,
            )
            .await;
        });
    }

    Ok(Json(key_response_from_result(&result)))
}

#[utoipa::path(
    get,
    path = "/api/v1/keys",
    responses(
        (status = 200, description = "List of user's AI service keys", body = KeyListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "AI Services"
)]
/// GET /api/v1/keys
pub async fn list_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<KeyListResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let views = unified_key_service::list_keys(&state.db, &user_id_str).await?;
    let keys = views.into_iter().map(key_response_from_view).collect();
    Ok(Json(KeyListResponse { keys }))
}

#[utoipa::path(
    get,
    path = "/api/v1/keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "User service ID")
    ),
    responses(
        (status = 200, description = "Key details", body = KeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "AI Services"
)]
/// GET /api/v1/keys/{key_id}
pub async fn get_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<KeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let view = unified_key_service::get_key(&state.db, &user_id_str, &key_id).await?;
    Ok(Json(key_response_from_view(view)))
}

#[utoipa::path(
    put,
    path = "/api/v1/keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "User service ID")
    ),
    request_body = UpdateKeyRequest,
    responses(
        (status = 200, description = "Key updated", body = KeyResponse),
        (status = 400, description = "Validation error", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "AI Services"
)]
/// PUT /api/v1/keys/{key_id}
pub async fn update_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
    Json(body): Json<UpdateKeyRequest>,
) -> AppResult<Json<KeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();

    // Load current state to find sub-resource IDs
    let view = unified_key_service::get_key(&state.db, &user_id_str, &key_id).await?;

    // Update label on UserApiKey if provided
    if let Some(ref label) = body.label {
        user_api_key_service::update_api_key(
            &state.db,
            &state.encryption_keys,
            &user_id_str,
            &view.api_key_id,
            Some(label.as_str()),
            None,
        )
        .await?;
    }

    // Update endpoint URL if provided
    if let Some(ref url) = body.endpoint_url {
        user_endpoint_service::update_endpoint(
            &state.db,
            &user_id_str,
            &view.endpoint_id,
            Some(url.as_str()),
            None,
        )
        .await?;
    }

    // Update UserService fields if any are provided
    if body.auth_method.is_some()
        || body.auth_key_name.is_some()
        || body.node_id.is_some()
        || body.is_active.is_some()
    {
        user_service_service::update_user_service(
            &state.db,
            &user_id_str,
            &key_id,
            body.auth_method.as_deref(),
            body.auth_key_name.as_deref(),
            body.node_id.as_deref(),
            None,
            body.is_active,
        )
        .await?;

        if body.node_id.is_some() || body.auth_method.is_some() {
            unified_key_service::reconcile_provider_key_for_service_routing(
                &state.db,
                &user_id_str,
                &key_id,
            )
            .await?;
        }
    }

    // Return refreshed view
    let updated = unified_key_service::get_key(&state.db, &user_id_str, &key_id).await?;
    Ok(Json(key_response_from_view(updated)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "User service ID")
    ),
    responses(
        (status = 200, description = "Key revoked", body = DeleteKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "AI Services"
)]
/// DELETE /api/v1/keys/{key_id}
pub async fn delete_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<DeleteKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    unified_key_service::revoke_key(&state.db, &user_id_str, &key_id).await?;
    Ok(Json(DeleteKeyResponse {
        message: "Key revoked successfully".to_string(),
    }))
}

fn key_response_from_result(result: &unified_key_service::CreateKeyResult) -> KeyResponse {
    KeyResponse {
        id: result.service.id.clone(),
        label: result.api_key.label.clone(),
        slug: result.service.slug.clone(),
        endpoint_url: result.endpoint.url.clone(),
        endpoint_id: result.endpoint.id.clone(),
        api_key_id: result.api_key.id.clone(),
        credential_type: result.api_key.credential_type.clone(),
        auth_method: result.service.auth_method.clone(),
        auth_key_name: result.service.auth_key_name.clone(),
        status: result.api_key.status.clone(),
        catalog_service_id: result.service.catalog_service_id.clone(),
        catalog_service_name: None,
        node_id: result.service.node_id.clone(),
        node_priority: result.service.node_priority,
        service_type: result.service.service_type.clone(),
        is_active: result.service.is_active,
        expires_at: result.api_key.expires_at.map(|dt| dt.to_rfc3339()),
        last_used_at: None,
        error_message: None,
        created_at: result.service.created_at.to_rfc3339(),
        ssh_host: result.ssh_host.clone(),
        ssh_port: result.ssh_port,
        ssh_ca_public_key: result.ssh_ca_public_key.clone(),
        ssh_allowed_principals: result.ssh_allowed_principals.clone(),
        ssh_certificate_ttl_minutes: result.ssh_certificate_ttl_minutes,
    }
}

fn key_response_from_view(view: unified_key_service::KeyView) -> KeyResponse {
    KeyResponse {
        id: view.id,
        label: view.label,
        slug: view.slug,
        endpoint_url: view.endpoint_url,
        endpoint_id: view.endpoint_id,
        api_key_id: view.api_key_id,
        credential_type: view.credential_type,
        auth_method: view.auth_method,
        auth_key_name: view.auth_key_name,
        status: view.status,
        catalog_service_id: view.catalog_service_id,
        catalog_service_name: view.catalog_service_name,
        node_id: view.node_id,
        node_priority: view.node_priority,
        service_type: view.service_type,
        is_active: view.is_active,
        expires_at: view.expires_at,
        last_used_at: view.last_used_at,
        error_message: view.error_message,
        created_at: view.created_at,
        ssh_host: view.ssh_host,
        ssh_port: view.ssh_port,
        ssh_ca_public_key: view.ssh_ca_public_key,
        ssh_allowed_principals: view.ssh_allowed_principals,
        ssh_certificate_ttl_minutes: view.ssh_certificate_ttl_minutes,
    }
}
