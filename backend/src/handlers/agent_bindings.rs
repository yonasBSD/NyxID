use axum::{
    Json,
    extract::{Path, State},
};
use futures::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::AppResult;
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::mw::auth::AuthUser;
use crate::services::{agent_binding_service, audit_service};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBindingRequest {
    pub user_service_id: String,
    pub user_api_key_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BindingResponse {
    pub id: String,
    pub api_key_id: String,
    pub user_service_id: String,
    pub user_api_key_id: String,
    pub service_slug: String,
    pub service_label: String,
    pub credential_label: String,
    pub created_at: String,
    pub updated_at: String,
}

async fn enrich_bindings(
    state: &AppState,
    bindings: Vec<crate::models::agent_service_binding::AgentServiceBinding>,
) -> AppResult<Vec<BindingResponse>> {
    let service_ids: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.user_service_id.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let credential_ids: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.user_api_key_id.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let services: Vec<UserService> = if service_ids.is_empty() {
        Vec::new()
    } else {
        state
            .db
            .collection::<UserService>(USER_SERVICES)
            .find(doc! { "_id": { "$in": &service_ids } })
            .await?
            .try_collect()
            .await?
    };
    let endpoints: Vec<UserEndpoint> = if services.is_empty() {
        Vec::new()
    } else {
        let endpoint_ids: Vec<&str> = services
            .iter()
            .map(|service| service.endpoint_id.as_str())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        state
            .db
            .collection::<UserEndpoint>(USER_ENDPOINTS)
            .find(doc! { "_id": { "$in": &endpoint_ids } })
            .await?
            .try_collect()
            .await?
    };
    let credentials: Vec<UserApiKey> = if credential_ids.is_empty() {
        Vec::new()
    } else {
        state
            .db
            .collection::<UserApiKey>(USER_API_KEYS)
            .find(doc! { "_id": { "$in": &credential_ids } })
            .await?
            .try_collect()
            .await?
    };

    let endpoint_labels: HashMap<String, String> = endpoints
        .into_iter()
        .map(|endpoint| (endpoint.id, endpoint.label))
        .collect();
    let service_map: HashMap<String, UserService> = services
        .into_iter()
        .map(|service| (service.id.clone(), service))
        .collect();
    let credential_map: HashMap<String, UserApiKey> = credentials
        .into_iter()
        .map(|credential| (credential.id.clone(), credential))
        .collect();

    Ok(bindings
        .into_iter()
        .map(|binding| {
            let service = service_map.get(&binding.user_service_id);
            let service_slug = service
                .map(|service| service.slug.clone())
                .unwrap_or_else(|| binding.user_service_id.clone());
            let service_label = service
                .and_then(|service| endpoint_labels.get(&service.endpoint_id).cloned())
                .unwrap_or_else(|| service_slug.clone());
            let credential_label = credential_map
                .get(&binding.user_api_key_id)
                .map(|credential| credential.label.clone())
                .unwrap_or_else(|| binding.user_api_key_id.clone());

            BindingResponse {
                id: binding.id,
                api_key_id: binding.api_key_id,
                user_service_id: binding.user_service_id,
                user_api_key_id: binding.user_api_key_id,
                service_slug,
                service_label,
                credential_label,
                created_at: binding.created_at.to_rfc3339(),
                updated_at: binding.updated_at.to_rfc3339(),
            }
        })
        .collect())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BindingListResponse {
    pub bindings: Vec<BindingResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteBindingResponse {
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/api-keys/{key_id}/bindings",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    request_body = CreateBindingRequest,
    responses(
        (status = 200, description = "Created binding", body = BindingResponse),
        (status = 400, description = "Validation error", body = crate::errors::ErrorResponse),
        (status = 404, description = "Not found", body = crate::errors::ErrorResponse),
        (status = 409, description = "Binding already exists", body = crate::errors::ErrorResponse)
    ),
    tag = "Agent Bindings"
)]
/// POST /api/v1/api-keys/{key_id}/bindings
pub async fn create_binding(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
    Json(body): Json<CreateBindingRequest>,
) -> AppResult<Json<BindingResponse>> {
    let user_id = auth_user.user_id.to_string();
    let binding = agent_binding_service::create_binding(
        &state.db,
        &user_id,
        &key_id,
        &body.user_service_id,
        &body.user_api_key_id,
    )
    .await?;

    audit_service::log_async(
        state.db.clone(),
        Some(user_id),
        "agent_binding_created".to_string(),
        Some(serde_json::json!({
            "binding_id": &binding.id,
            "api_key_id": &key_id,
            "user_service_id": &body.user_service_id,
            "user_api_key_id": &body.user_api_key_id,
        })),
        None,
        None,
        auth_user.api_key_id.clone(),
        auth_user.api_key_name.clone(),
    );

    let mut responses = enrich_bindings(&state, vec![binding]).await?;
    Ok(Json(responses.remove(0)))
}

#[utoipa::path(
    get,
    path = "/api/v1/api-keys/{key_id}/bindings",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "List of bindings", body = BindingListResponse),
        (status = 404, description = "API key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Agent Bindings"
)]
/// GET /api/v1/api-keys/{key_id}/bindings
pub async fn list_bindings(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<BindingListResponse>> {
    let user_id = auth_user.user_id.to_string();
    let bindings = agent_binding_service::list_bindings(&state.db, &user_id, &key_id).await?;
    let bindings = enrich_bindings(&state, bindings).await?;
    Ok(Json(BindingListResponse { bindings }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/{key_id}/bindings/{binding_id}",
    params(
        ("key_id" = String, Path, description = "API key ID"),
        ("binding_id" = String, Path, description = "Binding ID")
    ),
    responses(
        (status = 200, description = "Binding deleted", body = DeleteBindingResponse),
        (status = 404, description = "Binding not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Agent Bindings"
)]
/// DELETE /api/v1/api-keys/{key_id}/bindings/{binding_id}
pub async fn delete_binding(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((key_id, binding_id)): Path<(String, String)>,
) -> AppResult<Json<DeleteBindingResponse>> {
    let user_id = auth_user.user_id.to_string();
    agent_binding_service::delete_binding(&state.db, &user_id, &key_id, &binding_id).await?;

    audit_service::log_async(
        state.db.clone(),
        Some(user_id),
        "agent_binding_deleted".to_string(),
        Some(serde_json::json!({
            "binding_id": &binding_id,
            "api_key_id": &key_id,
        })),
        None,
        None,
        auth_user.api_key_id.clone(),
        auth_user.api_key_name.clone(),
    );

    Ok(Json(DeleteBindingResponse {
        message: "Binding deleted".to_string(),
    }))
}
