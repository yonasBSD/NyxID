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
use crate::models::user_endpoint::UserEndpoint;
use crate::mw::auth::AuthUser;
use crate::services::user_endpoint_service;

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
    let user_id_str = auth_user.user_id.to_string();
    user_endpoint_service::update_endpoint(
        &state.db,
        &user_id_str,
        &endpoint_id,
        body.url.as_deref(),
        body.label.as_deref(),
    )
    .await?;

    let ep = user_endpoint_service::get_endpoint(&state.db, &user_id_str, &endpoint_id).await?;
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
    let user_id_str = auth_user.user_id.to_string();
    user_endpoint_service::delete_endpoint(&state.db, &user_id_str, &endpoint_id).await?;
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
