use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::handlers::admin_helpers::require_admin;
use crate::handlers::auth::{extract_ip, extract_user_agent};
use crate::models::invite_code::{InviteCode, InviteCodeUsage};
use crate::mw::auth::AuthUser;
use crate::services::{audit_service, invite_code_service};

// --- Request / Response types ---
//
// Note: the 1..=1000 bound on `max_uses` is enforced by the `validator` crate
// attributes on `CreateInviteCodeRequest::max_uses` below. Keeping the limit
// at the request-type level means the error message is returned through the
// normal validation-error path instead of a bespoke handler check.

#[derive(Debug, Deserialize, Validate)]
pub struct CreateInviteCodeRequest {
    #[validate(range(min = 1, max = 1000, message = "max_uses must be between 1 and 1000"))]
    pub max_uses: Option<i32>,
    #[validate(length(max = 512, message = "Note must be at most 512 characters"))]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InviteCodeUsageResponse {
    pub user_id: String,
    pub used_at: String,
}

#[derive(Debug, Serialize)]
pub struct InviteCodeResponse {
    pub id: String,
    pub code: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by: String,
    pub note: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
    pub usages: Vec<InviteCodeUsageResponse>,
}

#[derive(Debug, Serialize)]
pub struct InviteCodeListResponse {
    pub invite_codes: Vec<InviteCodeResponse>,
}

#[derive(Debug, Serialize)]
pub struct DeactivateInviteCodeResponse {
    pub message: String,
}

fn usage_to_response(usage: InviteCodeUsage) -> InviteCodeUsageResponse {
    InviteCodeUsageResponse {
        user_id: usage.user_id,
        used_at: usage.used_at.to_rfc3339(),
    }
}

fn to_response(ic: InviteCode) -> InviteCodeResponse {
    InviteCodeResponse {
        id: ic.id,
        code: ic.code,
        max_uses: ic.max_uses,
        used_count: ic.used_count,
        created_by: ic.created_by,
        note: ic.note,
        is_active: ic.is_active,
        created_at: ic.created_at.to_rfc3339(),
        updated_at: ic.updated_at.to_rfc3339(),
        usages: ic.usages.into_iter().map(usage_to_response).collect(),
    }
}

// --- Handlers ---

/// POST /api/v1/admin/invite-codes
///
/// Create a new invite code (admin only).
pub async fn create_invite_code(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    auth_user: AuthUser,
    headers: HeaderMap,
    Json(body): Json<CreateInviteCodeRequest>,
) -> AppResult<Json<InviteCodeResponse>> {
    require_admin(&state, &auth_user).await?;

    body.validate()
        .map_err(|e| AppError::ValidationError(e.to_string()))?;

    let max_uses = body.max_uses.unwrap_or(10);
    let admin_id = auth_user.user_id.to_string();

    let invite = invite_code_service::create_invite_code(
        &state.db,
        &admin_id,
        max_uses,
        body.note.as_deref(),
    )
    .await?;

    audit_service::log_async(
        state.db.clone(),
        Some(admin_id),
        "admin_invite_code_create".to_string(),
        Some(serde_json::json!({
            "invite_code_id": invite.id,
            "code": invite.code,
            "max_uses": invite.max_uses,
        })),
        extract_ip(&headers, Some(peer)),
        extract_user_agent(&headers),
        None,
        None,
    );

    Ok(Json(to_response(invite)))
}

/// GET /api/v1/admin/invite-codes
///
/// List all invite codes (admin only).
pub async fn list_invite_codes(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<InviteCodeListResponse>> {
    require_admin(&state, &auth_user).await?;

    let codes = invite_code_service::list_invite_codes(&state.db).await?;

    Ok(Json(InviteCodeListResponse {
        invite_codes: codes.into_iter().map(to_response).collect(),
    }))
}

/// DELETE /api/v1/admin/invite-codes/{id}
///
/// Deactivate an invite code (admin only).
pub async fn deactivate_invite_code(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    auth_user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<DeactivateInviteCodeResponse>> {
    require_admin(&state, &auth_user).await?;

    invite_code_service::deactivate_invite_code(&state.db, &id).await?;

    audit_service::log_async(
        state.db.clone(),
        Some(auth_user.user_id.to_string()),
        "admin_invite_code_deactivate".to_string(),
        Some(serde_json::json!({ "invite_code_id": id })),
        extract_ip(&headers, Some(peer)),
        extract_user_agent(&headers),
        None,
        None,
    );

    Ok(Json(DeactivateInviteCodeResponse {
        message: "Invite code deactivated".to_string(),
    }))
}
