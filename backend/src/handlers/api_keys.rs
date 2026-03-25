use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::api_key::ApiKey;
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::node::{COLLECTION_NAME as NODES, Node};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::mw::auth::AuthUser;
use crate::services::key_service;

// --- Request / Response types ---

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: Option<String>,
    /// Accepts RFC 3339 ("2026-04-01T00:00:00Z") or date-only ("2026-04-01").
    pub expires_at: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub allowed_service_ids: Vec<String>,
    #[serde(default)]
    pub allowed_node_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_all_services: bool,
    #[serde(default = "default_true")]
    pub allow_all_nodes: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub scopes: Option<String>,
    pub allowed_service_ids: Option<Vec<String>>,
    pub allowed_node_ids: Option<Vec<String>>,
    pub allow_all_services: Option<bool>,
    pub allow_all_nodes: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub key_prefix: String,
    /// The full API key. Shown only once at creation time.
    pub full_key: String,
    pub scopes: String,
    pub created_at: String,
    pub allowed_service_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub allow_all_services: bool,
    pub allow_all_nodes: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AllowedServiceInfo {
    pub id: String,
    pub slug: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AllowedNodeInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub key_prefix: String,
    pub scopes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub allowed_service_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub allow_all_services: bool,
    pub allow_all_nodes: bool,
    pub allowed_services: Vec<AllowedServiceInfo>,
    pub allowed_nodes: Vec<AllowedNodeInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyListResponse {
    pub keys: Vec<ApiKeyResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteApiKeyResponse {
    pub message: String,
}

// --- Enrichment ---

/// Batch-enrich a list of API keys by loading all referenced UserServices and
/// Nodes in two `$in` queries instead of N+1 individual lookups.
async fn enrich_api_keys_batch(
    state: &AppState,
    keys: &[ApiKey],
) -> AppResult<Vec<ApiKeyResponse>> {
    // Collect all referenced IDs across all keys
    let all_service_ids: Vec<&str> = keys
        .iter()
        .flat_map(|k| k.allowed_service_ids.iter().map(|s| s.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let all_node_ids: Vec<&str> = keys
        .iter()
        .flat_map(|k| k.allowed_node_ids.iter().map(|s| s.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Batch-load UserServices
    let service_map: HashMap<String, UserService> = if all_service_ids.is_empty() {
        HashMap::new()
    } else {
        let services: Vec<UserService> = state
            .db
            .collection::<UserService>(USER_SERVICES)
            .find(doc! { "_id": { "$in": &all_service_ids } })
            .await?
            .try_collect()
            .await?;
        services.into_iter().map(|s| (s.id.clone(), s)).collect()
    };

    // Collect catalog_service_ids for name resolution
    let catalog_ids: Vec<&str> = service_map
        .values()
        .filter_map(|s| s.catalog_service_id.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let catalog_name_map: HashMap<String, String> = if catalog_ids.is_empty() {
        HashMap::new()
    } else {
        let catalog_services: Vec<DownstreamService> = state
            .db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find(doc! { "_id": { "$in": &catalog_ids } })
            .await?
            .try_collect()
            .await?;
        catalog_services
            .into_iter()
            .map(|ds| (ds.id.clone(), ds.name))
            .collect()
    };

    // Collect endpoint_ids for label resolution
    let endpoint_ids: Vec<&str> = service_map
        .values()
        .map(|s| s.endpoint_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let endpoint_label_map: HashMap<String, String> = if endpoint_ids.is_empty() {
        HashMap::new()
    } else {
        let endpoints: Vec<UserEndpoint> = state
            .db
            .collection::<UserEndpoint>(USER_ENDPOINTS)
            .find(doc! { "_id": { "$in": &endpoint_ids } })
            .await?
            .try_collect()
            .await?;
        endpoints
            .into_iter()
            .map(|ep| (ep.id.clone(), ep.label))
            .collect()
    };

    // Batch-load Nodes
    let node_map: HashMap<String, Node> = if all_node_ids.is_empty() {
        HashMap::new()
    } else {
        let nodes: Vec<Node> = state
            .db
            .collection::<Node>(NODES)
            .find(doc! { "_id": { "$in": &all_node_ids } })
            .await?
            .try_collect()
            .await?;
        nodes.into_iter().map(|n| (n.id.clone(), n)).collect()
    };

    // Build responses
    let items = keys
        .iter()
        .map(|key| {
            let allowed_services: Vec<AllowedServiceInfo> = key
                .allowed_service_ids
                .iter()
                .filter_map(|sid| {
                    service_map.get(sid).map(|svc| {
                        let label = endpoint_label_map
                            .get(&svc.endpoint_id)
                            .cloned()
                            .unwrap_or_else(|| svc.slug.clone());
                        let catalog_service_name = svc
                            .catalog_service_id
                            .as_ref()
                            .and_then(|cid| catalog_name_map.get(cid).cloned());
                        AllowedServiceInfo {
                            id: svc.id.clone(),
                            slug: svc.slug.clone(),
                            label,
                            catalog_service_name,
                        }
                    })
                })
                .collect();

            let allowed_nodes: Vec<AllowedNodeInfo> = key
                .allowed_node_ids
                .iter()
                .filter_map(|nid| {
                    node_map.get(nid).map(|node| AllowedNodeInfo {
                        id: node.id.clone(),
                        name: node.name.clone(),
                        status: node.status.as_str().to_string(),
                    })
                })
                .collect();

            ApiKeyResponse {
                id: key.id.clone(),
                name: key.name.clone(),
                description: key.description.clone(),
                key_prefix: key.key_prefix.clone(),
                scopes: key.scopes.clone(),
                last_used_at: key.last_used_at.map(|dt| dt.to_rfc3339()),
                expires_at: key.expires_at.map(|dt| dt.to_rfc3339()),
                is_active: key.is_active,
                created_at: key.created_at.to_rfc3339(),
                allowed_service_ids: key.allowed_service_ids.clone(),
                allowed_node_ids: key.allowed_node_ids.clone(),
                allow_all_services: key.allow_all_services,
                allow_all_nodes: key.allow_all_nodes,
                allowed_services,
                allowed_nodes,
            }
        })
        .collect();

    Ok(items)
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/api/v1/api-keys",
    responses(
        (status = 200, description = "List of NyxID API keys", body = ApiKeyListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// GET /api/v1/api-keys
pub async fn list_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<ApiKeyListResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let keys = key_service::list_api_keys(&state.db, &user_id_str).await?;
    let items = enrich_api_keys_batch(&state, &keys).await?;
    Ok(Json(ApiKeyListResponse { keys: items }))
}

#[utoipa::path(
    get,
    path = "/api/v1/api-keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key details", body = ApiKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "API key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// GET /api/v1/api-keys/{key_id}
pub async fn get_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<ApiKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let key = key_service::get_api_key(&state.db, &user_id_str, &key_id).await?;
    let enriched = enrich_api_keys_batch(&state, &[key]).await?;
    Ok(Json(enriched.into_iter().next().unwrap()))
}

/// Parse an optional expiry date string. Accepts RFC 3339 datetime
/// (e.g. "2026-04-01T00:00:00Z") or date-only (e.g. "2026-04-01").
fn parse_expires_at(s: &str) -> AppResult<DateTime<Utc>> {
    // Try RFC 3339 first
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try date-only (YYYY-MM-DD) -> end of day UTC
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        && let Some(dt) = date.and_hms_opt(23, 59, 59)
    {
        return Ok(dt.and_utc());
    }
    Err(AppError::ValidationError(
        "Invalid expires_at format. Use RFC 3339 (e.g. 2026-04-01T00:00:00Z) or date-only (e.g. 2026-04-01)".to_string(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/api-keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "Created NyxID API key (full key shown once)", body = CreateApiKeyResponse),
        (status = 400, description = "Validation error", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// POST /api/v1/api-keys
pub async fn create_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateApiKeyRequest>,
) -> AppResult<Json<CreateApiKeyResponse>> {
    if body.name.is_empty() {
        return Err(AppError::ValidationError(
            "API key name is required".to_string(),
        ));
    }

    let scopes = body.scopes.as_deref().unwrap_or("read");

    let expires_at = body
        .expires_at
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(parse_expires_at)
        .transpose()?;

    let user_id_str = auth_user.user_id.to_string();
    let created = key_service::create_api_key(
        &state.db,
        &user_id_str,
        &body.name,
        scopes,
        expires_at,
        body.description.as_deref(),
        Some(&body.allowed_service_ids),
        Some(&body.allowed_node_ids),
        Some(body.allow_all_services),
        Some(body.allow_all_nodes),
    )
    .await?;

    Ok(Json(CreateApiKeyResponse {
        id: created.id,
        name: created.name,
        description: created.description,
        key_prefix: created.key_prefix,
        full_key: created.full_key,
        scopes: created.scopes,
        created_at: created.created_at.to_rfc3339(),
        allowed_service_ids: created.allowed_service_ids,
        allowed_node_ids: created.allowed_node_ids,
        allow_all_services: created.allow_all_services,
        allow_all_nodes: created.allow_all_nodes,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/api-keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    request_body = UpdateApiKeyRequest,
    responses(
        (status = 200, description = "Updated API key", body = ApiKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "API key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// PUT /api/v1/api-keys/{key_id}
pub async fn update_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
    Json(body): Json<UpdateApiKeyRequest>,
) -> AppResult<Json<ApiKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();

    let updated = key_service::update_api_key_scope(
        &state.db,
        &user_id_str,
        &key_id,
        body.name.as_deref(),
        body.description.as_deref(),
        body.scopes.as_deref(),
        body.allowed_service_ids.as_deref(),
        body.allowed_node_ids.as_deref(),
        body.allow_all_services,
        body.allow_all_nodes,
    )
    .await?;

    let enriched = enrich_api_keys_batch(&state, &[updated]).await?;
    Ok(Json(enriched.into_iter().next().unwrap()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/{key_id}",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key deleted", body = DeleteApiKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "API key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// DELETE /api/v1/api-keys/{key_id}
pub async fn delete_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<DeleteApiKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    key_service::delete_api_key(&state.db, &user_id_str, &key_id).await?;

    Ok(Json(DeleteApiKeyResponse {
        message: "API key deleted".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/api-keys/{key_id}/rotate",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "Rotated API key (new full key shown once)", body = CreateApiKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "API key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "API Keys"
)]
/// POST /api/v1/api-keys/{key_id}/rotate
pub async fn rotate_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<CreateApiKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let created = key_service::rotate_api_key(&state.db, &user_id_str, &key_id).await?;

    Ok(Json(CreateApiKeyResponse {
        id: created.id,
        name: created.name,
        description: created.description,
        key_prefix: created.key_prefix,
        full_key: created.full_key,
        scopes: created.scopes,
        created_at: created.created_at.to_rfc3339(),
        allowed_service_ids: created.allowed_service_ids,
        allowed_node_ids: created.allowed_node_ids,
        allow_all_services: created.allow_all_services,
        allow_all_nodes: created.allow_all_nodes,
    }))
}
