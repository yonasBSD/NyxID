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
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::mw::auth::AuthUser;
use crate::services::{org_service, user_api_key_service, user_service_service};

/// Look up the external API key without an ownership filter and check
/// whether the actor may modify it (directly or as an org admin).
/// Returns the effective owner_id (which may be an org user_id) for
/// downstream service calls.
///
/// `OrgMembership.allowed_service_ids` lives in the `UserService.id`
/// space, so we translate by looking up every UserService that
/// references this credential and gating on `allows_any_resource`. An
/// orphan credential (referenced by zero services) is only writable by
/// Direct owners or unscoped admins.
async fn resolve_api_key_write_owner(
    state: &AppState,
    actor: &str,
    key_id: &str,
) -> AppResult<String> {
    let key = state
        .db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": key_id })
        .await?
        .ok_or_else(|| AppError::NotFound("API key not found".to_string()))?;

    let access = org_service::resolve_owner_access(&state.db, actor, &key.user_id).await?;
    if !access.can_read() {
        return Err(AppError::NotFound("API key not found".to_string()));
    }
    let backing_service_ids =
        user_service_service::user_service_ids_for_api_key(&state.db, &key.user_id, &key.id)
            .await?;
    if !access.allows_any_resource(&backing_service_ids) {
        return Err(AppError::NotFound("API key not found".to_string()));
    }
    if !access.can_write() {
        return Err(AppError::OrgRoleInsufficient(
            "you do not have permission to modify this API key".to_string(),
        ));
    }
    Ok(key.user_id)
}

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
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_api_key_write_owner(&state, &actor, &key_id).await?;

    user_api_key_service::update_api_key(
        &state.db,
        &state.encryption_keys,
        &owner_id,
        &key_id,
        body.label.as_deref(),
        body.credential.as_deref(),
    )
    .await?;

    let key = user_api_key_service::get_api_key(&state.db, &owner_id, &key_id).await?;
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
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_api_key_write_owner(&state, &actor, &key_id).await?;
    user_api_key_service::delete_api_key(&state.db, &owner_id, &key_id).await?;
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
