use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::mw::auth::AuthUser;
use crate::services::{org_service, user_endpoint_service, user_service_service};

/// Resolve which user_id owns this endpoint and whether the actor may
/// modify it. Returns the effective owner_id (may be an org user id) for
/// downstream service calls. Errors out as Forbidden / NotFound otherwise.
///
/// `OrgMembership.allowed_service_ids` is keyed by `UserService.id`, not
/// by endpoint id. We translate by looking up every UserService that
/// references this endpoint and gating on `allows_any_resource`. An
/// orphan endpoint (referenced by zero services) is treated as a
/// scope-less resource: only Direct owners or unscoped admins can touch
/// it, since a scoped admin has no concrete claim to it.
async fn resolve_endpoint_write_owner(
    state: &AppState,
    actor: &str,
    endpoint_id: &str,
) -> AppResult<String> {
    let endpoint = state
        .db
        .collection::<UserEndpoint>(USER_ENDPOINTS)
        .find_one(doc! { "_id": endpoint_id })
        .await?
        .ok_or_else(|| AppError::NotFound("Endpoint not found".to_string()))?;

    let access = org_service::resolve_owner_access(&state.db, actor, &endpoint.user_id).await?;
    if !access.can_read() {
        return Err(AppError::NotFound("Endpoint not found".to_string()));
    }
    let backing_service_ids = user_service_service::user_service_ids_for_endpoint(
        &state.db,
        &endpoint.user_id,
        &endpoint.id,
    )
    .await?;
    if !access.allows_any_resource(&backing_service_ids) {
        return Err(AppError::NotFound("Endpoint not found".to_string()));
    }
    if !access.can_write() {
        return Err(AppError::OrgRoleInsufficient(
            "you do not have permission to modify this endpoint".to_string(),
        ));
    }
    Ok(endpoint.user_id)
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateEndpointRequest {
    pub url: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EndpointResponse {
    pub id: String,
    pub label: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EndpointListResponse {
    pub endpoints: Vec<EndpointResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/endpoints",
    responses(
        (status = 200, description = "List of user endpoints", body = EndpointListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "Endpoints"
)]
/// GET /api/v1/endpoints
pub async fn list_endpoints(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<EndpointListResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let endpoints = user_endpoint_service::list_endpoints(&state.db, &user_id_str).await?;
    let items = endpoints.into_iter().map(endpoint_response).collect();
    Ok(Json(EndpointListResponse { endpoints: items }))
}

#[utoipa::path(
    put,
    path = "/api/v1/endpoints/{endpoint_id}",
    params(
        ("endpoint_id" = String, Path, description = "User endpoint ID")
    ),
    request_body = UpdateEndpointRequest,
    responses(
        (status = 200, description = "Updated endpoint", body = EndpointResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Endpoint not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Endpoints"
)]
/// PUT /api/v1/endpoints/{endpoint_id}
pub async fn update_endpoint(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(endpoint_id): Path<String>,
    Json(body): Json<UpdateEndpointRequest>,
) -> AppResult<Json<EndpointResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_endpoint_write_owner(&state, &actor, &endpoint_id).await?;

    user_endpoint_service::update_endpoint(
        &state.db,
        &owner_id,
        &endpoint_id,
        body.url.as_deref(),
        body.label.as_deref(),
    )
    .await?;

    let ep = user_endpoint_service::get_endpoint(&state.db, &owner_id, &endpoint_id).await?;
    Ok(Json(endpoint_response(ep)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/endpoints/{endpoint_id}",
    params(
        ("endpoint_id" = String, Path, description = "User endpoint ID")
    ),
    responses(
        (status = 204, description = "Endpoint deleted"),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Endpoint not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Endpoints"
)]
/// DELETE /api/v1/endpoints/{endpoint_id}
pub async fn delete_endpoint(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(endpoint_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_endpoint_write_owner(&state, &actor, &endpoint_id).await?;
    user_endpoint_service::delete_endpoint(&state.db, &owner_id, &endpoint_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn endpoint_response(ep: UserEndpoint) -> EndpointResponse {
    EndpointResponse {
        id: ep.id,
        label: ep.label,
        url: ep.url,
        catalog_service_id: ep.catalog_service_id,
        created_at: ep.created_at.to_rfc3339(),
        updated_at: ep.updated_at.to_rfc3339(),
    }
}
