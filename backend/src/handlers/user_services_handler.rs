use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::AppResult;
use crate::models::user_service::UserService;
use crate::mw::auth::AuthUser;
use crate::services::{node_service, unified_key_service, user_service_service};

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserServiceRequest {
    pub auth_method: Option<String>,
    pub auth_key_name: Option<String>,
    /// "" to clear, Some(id) to set, None to leave unchanged
    pub node_id: Option<String>,
    pub node_priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserServiceResponse {
    pub id: String,
    pub slug: String,
    pub endpoint_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<String>,
    pub auth_method: String,
    pub auth_key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserServiceListResponse {
    pub services: Vec<UserServiceResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/user-services",
    responses(
        (status = 200, description = "List of user's proxy routing configs", body = UserServiceListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "User Services"
)]
/// GET /api/v1/user-services
pub async fn list_user_services(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<UserServiceListResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let services = user_service_service::list_user_services(&state.db, &user_id_str).await?;
    let items = services.into_iter().map(user_service_response).collect();
    Ok(Json(UserServiceListResponse { services: items }))
}

#[utoipa::path(
    put,
    path = "/api/v1/user-services/{service_id}",
    params(
        ("service_id" = String, Path, description = "User service ID")
    ),
    request_body = UpdateUserServiceRequest,
    responses(
        (status = 200, description = "Updated user service", body = UserServiceResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Service not found", body = crate::errors::ErrorResponse)
    ),
    tag = "User Services"
)]
/// PUT /api/v1/user-services/{service_id}
pub async fn update_user_service(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(service_id): Path<String>,
    Json(body): Json<UpdateUserServiceRequest>,
) -> AppResult<Json<UserServiceResponse>> {
    let user_id_str = auth_user.user_id.to_string();

    // Load current state before update (needed for node binding sync).
    let current =
        user_service_service::get_user_service(&state.db, &user_id_str, &service_id).await?;

    user_service_service::update_user_service(
        &state.db,
        &user_id_str,
        &service_id,
        body.auth_method.as_deref(),
        body.auth_key_name.as_deref(),
        body.node_id.as_deref(),
        body.node_priority,
        body.is_active,
    )
    .await?;

    if body.node_id.is_some() || body.auth_method.is_some() {
        unified_key_service::reconcile_provider_key_for_service_routing(
            &state.db,
            &user_id_str,
            &service_id,
        )
        .await?;
    }

    // Auto-sync NodeServiceBinding when node_id changes.
    if body.node_id.is_some() {
        node_service::sync_node_binding_for_user_service(
            &state.db,
            &user_id_str,
            current.catalog_service_id.as_deref(),
            body.node_id.as_deref(),
            current.node_id.as_deref(),
        )
        .await?;
    }

    let svc = user_service_service::get_user_service(&state.db, &user_id_str, &service_id).await?;
    Ok(Json(user_service_response(svc)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/user-services/{service_id}",
    params(
        ("service_id" = String, Path, description = "User service ID")
    ),
    responses(
        (status = 204, description = "User service deactivated"),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Service not found", body = crate::errors::ErrorResponse)
    ),
    tag = "User Services"
)]
/// DELETE /api/v1/user-services/{service_id}
pub async fn delete_user_service(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(service_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id_str = auth_user.user_id.to_string();

    // Load current state to clean up node binding.
    let current =
        user_service_service::get_user_service(&state.db, &user_id_str, &service_id).await?;

    user_service_service::deactivate_user_service(&state.db, &user_id_str, &service_id).await?;

    // Deactivate the node binding if this service was node-routed.
    node_service::sync_node_binding_for_user_service(
        &state.db,
        &user_id_str,
        current.catalog_service_id.as_deref(),
        None, // new node_id = none (cleared)
        current.node_id.as_deref(),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn user_service_response(svc: UserService) -> UserServiceResponse {
    UserServiceResponse {
        id: svc.id,
        slug: svc.slug,
        endpoint_id: svc.endpoint_id,
        api_key_id: svc.api_key_id,
        auth_method: svc.auth_method,
        auth_key_name: svc.auth_key_name,
        catalog_service_id: svc.catalog_service_id,
        node_id: svc.node_id,
        node_priority: svc.node_priority,
        is_active: svc.is_active,
        created_at: svc.created_at.to_rfc3339(),
        updated_at: svc.updated_at.to_rfc3339(),
    }
}
