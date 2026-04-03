use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::user::{COLLECTION_NAME as USERS, User};
use crate::mw::auth::AuthUser;
use crate::services::invite_code_service;

// --- Helpers ---

/// Verify that the authenticated user is an admin.
async fn require_admin(state: &AppState, auth_user: &AuthUser) -> AppResult<()> {
    use mongodb::bson::doc;

    let user_id = auth_user.user_id.to_string();

    let user_model = state
        .db
        .collection::<User>(USERS)
        .find_one(doc! { "_id": &user_id })
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if !user_model.is_admin {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }

    Ok(())
}

// --- Request / Response types ---

#[derive(Debug, Deserialize)]
pub struct CreateInviteCodeRequest {
    pub max_uses: Option<i32>,
    pub note: Option<String>,
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
}

#[derive(Debug, Serialize)]
pub struct InviteCodeListResponse {
    pub invite_codes: Vec<InviteCodeResponse>,
}

#[derive(Debug, Serialize)]
pub struct DeactivateInviteCodeResponse {
    pub message: String,
}

fn to_response(ic: crate::models::invite_code::InviteCode) -> InviteCodeResponse {
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
    }
}

// --- Handlers ---

/// POST /api/v1/admin/invite-codes
///
/// Create a new invite code (admin only).
pub async fn create_invite_code(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateInviteCodeRequest>,
) -> AppResult<Json<InviteCodeResponse>> {
    require_admin(&state, &auth_user).await?;

    let max_uses = body.max_uses.unwrap_or(10);
    let admin_id = auth_user.user_id.to_string();

    let invite = invite_code_service::create_invite_code(
        &state.db,
        &admin_id,
        max_uses,
        body.note.as_deref(),
    )
    .await?;

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
    auth_user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<DeactivateInviteCodeResponse>> {
    require_admin(&state, &auth_user).await?;

    invite_code_service::deactivate_invite_code(&state.db, &id).await?;

    Ok(Json(DeactivateInviteCodeResponse {
        message: "Invite code deactivated".to_string(),
    }))
}
