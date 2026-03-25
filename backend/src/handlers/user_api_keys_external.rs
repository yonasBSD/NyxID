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
use crate::models::user_api_key::UserApiKey;
use crate::mw::auth::AuthUser;
use crate::services::user_api_key_service;

#[derive(Deserialize, ToSchema)]
pub struct UpdateExternalApiKeyRequest {
    pub label: Option<String>,
    pub credential: Option<String>,
}

impl std::fmt::Debug for UpdateExternalApiKeyRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateExternalApiKeyRequest")
            .field("label", &self.label)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExternalApiKeyResponse {
    pub id: String,
    pub label: String,
    pub credential_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExternalApiKeyListResponse {
    pub api_keys: Vec<ExternalApiKeyResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/api-keys/external",
    responses(
        (status = 200, description = "List of user's external API keys", body = ExternalApiKeyListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "External API Keys"
)]
/// GET /api/v1/api-keys/external
pub async fn list_external_api_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<ExternalApiKeyListResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    let keys = user_api_key_service::list_api_keys(&state.db, &user_id_str).await?;
    let items = keys.into_iter().map(external_api_key_response).collect();
    Ok(Json(ExternalApiKeyListResponse { api_keys: items }))
}

#[utoipa::path(
    put,
    path = "/api/v1/api-keys/external/{key_id}",
    params(
        ("key_id" = String, Path, description = "External API key ID")
    ),
    request_body = UpdateExternalApiKeyRequest,
    responses(
        (status = 200, description = "Updated external API key", body = ExternalApiKeyResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "External API Keys"
)]
/// PUT /api/v1/api-keys/external/{key_id}
pub async fn update_external_api_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
    Json(body): Json<UpdateExternalApiKeyRequest>,
) -> AppResult<Json<ExternalApiKeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();
    user_api_key_service::update_api_key(
        &state.db,
        &state.encryption_keys,
        &user_id_str,
        &key_id,
        body.label.as_deref(),
        body.credential.as_deref(),
    )
    .await?;

    let key = user_api_key_service::get_api_key(&state.db, &user_id_str, &key_id).await?;
    Ok(Json(external_api_key_response(key)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/external/{key_id}",
    params(
        ("key_id" = String, Path, description = "External API key ID")
    ),
    responses(
        (status = 204, description = "External API key deleted"),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Key not found", body = crate::errors::ErrorResponse)
    ),
    tag = "External API Keys"
)]
/// DELETE /api/v1/api-keys/external/{key_id}
pub async fn delete_external_api_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let user_id_str = auth_user.user_id.to_string();
    user_api_key_service::delete_api_key(&state.db, &user_id_str, &key_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn external_api_key_response(key: UserApiKey) -> ExternalApiKeyResponse {
    ExternalApiKeyResponse {
        id: key.id,
        label: key.label,
        credential_type: key.credential_type,
        status: key.status,
        provider_config_id: key.provider_config_id,
        expires_at: key.expires_at.map(|dt| dt.to_rfc3339()),
        last_used_at: key.last_used_at.map(|dt| dt.to_rfc3339()),
        error_message: key.error_message,
        created_at: key.created_at.to_rfc3339(),
        updated_at: key.updated_at.to_rfc3339(),
    }
}
