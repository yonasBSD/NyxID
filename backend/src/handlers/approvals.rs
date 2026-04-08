use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::service_approval_config::ApprovalMode;
use crate::mw::auth::AuthUser;
use crate::services::{approval_service, audit_service};

// --- Response types ---

#[derive(Debug, Serialize)]
pub struct ApprovalRequestItem {
    pub id: String,
    pub service_name: String,
    pub service_slug: String,
    pub requester_type: String,
    pub requester_label: Option<String>,
    pub operation_summary: String,
    pub action_description: Option<String>,
    /// Tool approval fields (null for proxy-initiated approvals)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_destructive: Option<bool>,
    pub approval_mode: ApprovalMode,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub decision_channel: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApprovalRequestsResponse {
    pub requests: Vec<ApprovalRequestItem>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

#[derive(Debug, Serialize)]
pub struct ApprovalGrantItem {
    pub id: String,
    pub service_id: String,
    pub service_name: String,
    pub requester_type: String,
    pub requester_id: String,
    pub requester_label: Option<String>,
    pub granted_at: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct ApprovalGrantsResponse {
    pub grants: Vec<ApprovalGrantItem>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

#[derive(Debug, Serialize)]
pub struct ApprovalStatusResponse {
    pub status: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct DecideResponse {
    pub id: String,
    pub status: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

// --- Tool approval types ---

#[derive(Debug, Deserialize)]
pub struct CreateToolApprovalRequest {
    pub tool_name: String,
    pub tool_call_id: Option<String>,
    pub arguments: Option<String>,
    pub is_destructive: Option<bool>,
    #[allow(dead_code)]
    pub approval_mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateApprovalResponse {
    pub id: String,
    pub status: String,
    pub expires_at: String,
}

fn to_approval_request_item(
    request: crate::models::approval_request::ApprovalRequest,
) -> ApprovalRequestItem {
    ApprovalRequestItem {
        id: request.id,
        service_name: request.service_name,
        service_slug: request.service_slug,
        requester_type: request.requester_type,
        requester_label: request.requester_label,
        operation_summary: request.operation_summary,
        action_description: request.action_description,
        tool_name: request.tool_name,
        tool_call_id: request.tool_call_id,
        tool_arguments: request.tool_arguments,
        is_destructive: request.is_destructive,
        approval_mode: request.approval_mode,
        status: request.status,
        created_at: request.created_at.to_rfc3339(),
        decided_at: request.decided_at.map(|d| d.to_rfc3339()),
        decision_channel: request.decision_channel,
    }
}

/// Legacy strict ownership check kept for the existing unit tests. The
/// runtime path now uses `ensure_caller_can_decide` to support org-policy
/// approvals.
#[cfg(test)]
fn ensure_request_owned_by_user(request_user_id: &str, auth_user_id: &str) -> AppResult<()> {
    if request_user_id != auth_user_id {
        return Err(AppError::Forbidden(
            "You are not authorized to view this approval request".to_string(),
        ));
    }

    Ok(())
}

/// Authorize a caller against an approval request that may belong to an
/// org. The caller is allowed if either of the following holds:
///
/// 1. They are the literal `request.user_id` owner (personal request).
/// 2. They are *currently* an admin of the org that owns the request
///    AND that admin's `allowed_service_ids` scope (if any) covers the
///    `UserService` backing the request.
///
/// `request.notify_user_ids` is intentionally NOT consulted here. It
/// is only a routing hint captured at request creation time, so for
/// org-policy requests it would otherwise let an admin who has since
/// been removed or demoted decide outstanding requests. The live
/// `resolve_owner_access` check is the single source of truth.
///
/// `request.service_id` is a catalog `DownstreamService.id`, but
/// `OrgMembership.allowed_service_ids` lives in the `UserService.id`
/// space. We translate by looking up the `UserService` row that the
/// request was filed against (`user_id = request.user_id`,
/// `catalog_service_id = request.service_id`).
async fn ensure_caller_can_decide(
    db: &mongodb::Database,
    request: &crate::models::approval_request::ApprovalRequest,
    auth_user_id: &str,
) -> AppResult<()> {
    if request.user_id == auth_user_id {
        return Ok(());
    }

    // Check whether the request owner is an org and the caller is one of
    // its admins. `resolve_owner_access` returns Forbidden for non-org
    // owners or non-member callers, which collapses both "ex-admin" and
    // "stranger" into the same denial path.
    let access =
        crate::services::org_service::resolve_owner_access(db, auth_user_id, &request.user_id)
            .await?;
    if !access.can_write() {
        return Err(AppError::Forbidden(
            "You are not authorized to act on this approval request".to_string(),
        ));
    }

    // Translate the catalog service id stored on the request into the
    // UserService.id(s) used by the org membership scope, then gate on
    // `allows_any_resource`. Empty result means the request was filed
    // against a UserService that no longer exists -- safer to require
    // an unscoped admin to decide it.
    let user_service_ids = crate::services::user_service_service::user_service_ids_for_catalog(
        db,
        &request.user_id,
        &request.service_id,
    )
    .await?;
    if !access.allows_any_resource(&user_service_ids) {
        return Err(AppError::Forbidden(
            "Your org admin role is scoped to other services and cannot decide on this approval request".to_string(),
        ));
    }

    Ok(())
}

// --- Query/Request types ---

#[derive(Debug, Deserialize)]
pub struct ApprovalRequestsQuery {
    pub status: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    /// When set, list approval requests scoped to the given org instead of
    /// the caller's personal scope. The caller must be an admin of that
    /// org. Without this filter the endpoint returns the actor's personal
    /// approval history (existing behavior).
    pub org_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GrantsQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    /// When set, list grants owned by the given org instead of the
    /// caller's personal scope. The caller must be an admin of that
    /// org. Org-policy approvals create grants under the org's user_id,
    /// so this is the only way for org admins to see them.
    pub org_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeGrantQuery {
    /// When set, revoke a grant owned by the given org. The caller
    /// must be an admin of that org.
    pub org_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DecideRequest {
    pub approved: bool,
    pub duration_sec: Option<i64>,
}

// --- Handlers ---

/// GET /api/v1/approvals/requests
pub async fn list_requests(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ApprovalRequestsQuery>,
) -> AppResult<Json<ApprovalRequestsResponse>> {
    let actor = auth_user.user_id.to_string();

    if let Some(ref status) = query.status
        && !["pending", "approved", "rejected", "expired"].contains(&status.as_str())
    {
        return Err(crate::errors::AppError::ValidationError(
            "status must be one of: pending, approved, rejected, expired".to_string(),
        ));
    }

    // org_id query param scopes the listing to org-policy approvals.
    // The caller must be an admin of the target org.
    let listing_user_id = if let Some(target_org_id) = query.org_id.as_deref() {
        let access =
            crate::services::org_service::resolve_owner_access(&state.db, &actor, target_org_id)
                .await?;
        if !access.can_write() {
            return Err(AppError::OrgRoleInsufficient(
                "admin access to the target org is required to list its approval history"
                    .to_string(),
            ));
        }
        target_org_id.to_string()
    } else {
        actor
    };

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    let (requests, total) = approval_service::list_requests(
        &state.db,
        &listing_user_id,
        query.status.as_deref(),
        page,
        per_page,
    )
    .await?;

    let items: Vec<ApprovalRequestItem> =
        requests.into_iter().map(to_approval_request_item).collect();

    Ok(Json(ApprovalRequestsResponse {
        requests: items,
        total,
        page,
        per_page,
    }))
}

/// GET /api/v1/approvals/requests/{request_id}
///
/// Returns approval request detail for the current user. Org admins can
/// view requests created under their org's policy in addition to their
/// own personal requests.
pub async fn get_request_by_id(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(request_id): Path<String>,
) -> AppResult<Json<ApprovalRequestItem>> {
    let user_id = auth_user.user_id.to_string();
    let request = approval_service::get_request(&state.db, &request_id).await?;

    ensure_caller_can_decide(&state.db, &request, &user_id).await?;

    Ok(Json(to_approval_request_item(request)))
}

/// GET /api/v1/approvals/requests/{request_id}/status
///
/// Polling endpoint for callers that received approval_required.
/// Accessible by delegated tokens and service accounts.
///
/// SECURITY: caller must authenticate and match the original requester binding
/// (resource owner + requester_type + requester_id).
pub async fn get_request_status(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(request_id): Path<String>,
) -> AppResult<Json<ApprovalStatusResponse>> {
    let request = approval_service::get_request(&state.db, &request_id).await?;
    let owner_user_id = auth_user.effective_approval_owner_user_id();
    let requester_type = auth_user.approval_requester_type().ok_or_else(|| {
        AppError::Forbidden("Session-authenticated callers cannot poll approval status".to_string())
    })?;
    let requester_id = auth_user.approval_requester_id();

    // Requester binding must always match -- this is the defense against
    // an unrelated API key polling someone else's request.
    if request.requester_type != requester_type || request.requester_id != requester_id {
        return Err(AppError::Forbidden(
            "You are not authorized to view this approval request".to_string(),
        ));
    }

    // Owner check: the strict legacy path required `request.user_id == owner_user_id`
    // (the actor's effective approval owner). Org-policy requests live
    // under the org's user_id, so the strict check would block legitimate
    // polling by the agent that triggered the request. Allow either:
    // - the legacy match, OR
    // - the actor has any access to the owning org (member or admin),
    //   which means the request was created on their behalf via cascade.
    if request.user_id != owner_user_id {
        let access = crate::services::org_service::resolve_owner_access(
            &state.db,
            &owner_user_id,
            &request.user_id,
        )
        .await?;
        if !access.can_read() {
            return Err(AppError::Forbidden(
                "You are not authorized to view this approval request".to_string(),
            ));
        }
    }

    Ok(Json(ApprovalStatusResponse {
        status: request.status,
        expires_at: request.expires_at.to_rfc3339(),
    }))
}

/// POST /api/v1/approvals/requests
///
/// Create a tool approval request (for external callers such as Aevatar).
/// Triggers the same notification pipeline as proxy-initiated approvals.
pub async fn create_request(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateToolApprovalRequest>,
) -> AppResult<(axum::http::StatusCode, Json<CreateApprovalResponse>)> {
    // Validate tool_name
    let tool_name = body.tool_name.trim();
    if tool_name.is_empty() || tool_name.len() > 256 {
        return Err(AppError::ValidationError(
            "tool_name must be 1-256 characters".to_string(),
        ));
    }

    // Validate arguments length
    if let Some(ref args) = body.arguments
        && args.len() > 65536
    {
        return Err(AppError::ValidationError(
            "arguments must be at most 65536 characters".to_string(),
        ));
    }

    let user_id = auth_user.user_id.to_string();

    // Determine requester identity from auth context.
    // Session callers use "user" as requester_type (they are requesting
    // approval from themselves, valid for agents running in their session).
    let (requester_type, requester_id) = match auth_user.approval_requester_type() {
        Some(rt) => (rt.to_string(), auth_user.approval_requester_id()),
        None => ("user".to_string(), user_id.clone()),
    };

    let requester_label = auth_user.api_key_name.as_deref();

    let request = approval_service::create_tool_approval_request(
        &state.db,
        &state.config,
        &state.http_client,
        state.fcm_auth.as_deref(),
        state.apns_auth.as_deref(),
        &user_id,
        tool_name,
        body.tool_call_id.as_deref(),
        body.arguments.as_deref(),
        body.is_destructive.unwrap_or(false),
        &requester_type,
        &requester_id,
        requester_label,
    )
    .await?;

    audit_service::log_async(
        state.db.clone(),
        Some(user_id),
        "tool_approval_created".to_string(),
        Some(serde_json::json!({
            "request_id": &request.id,
            "tool_name": tool_name,
            "is_destructive": body.is_destructive.unwrap_or(false),
        })),
        None,
        None,
        auth_user.api_key_id.as_deref().map(String::from),
        auth_user.api_key_name.clone(),
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateApprovalResponse {
            id: request.id,
            status: request.status,
            expires_at: request.expires_at.to_rfc3339(),
        }),
    ))
}

/// POST /api/v1/approvals/requests/{request_id}/decide
///
/// Approve or reject an approval request via the web UI.
pub async fn decide_request(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DecideRequest>,
) -> AppResult<Json<DecideResponse>> {
    let user_id = auth_user.user_id.to_string();
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    // Verify the caller can act on this request -- direct owner, named
    // recipient, or current admin of the owning org.
    let request = approval_service::get_request(&state.db, &request_id).await?;
    ensure_caller_can_decide(&state.db, &request, &user_id).await?;

    if let Some(duration_sec) = body.duration_sec
        && duration_sec <= 0
    {
        return Err(crate::errors::AppError::ValidationError(
            "duration_sec must be positive".to_string(),
        ));
    }

    let updated = approval_service::process_decision(
        &state.db,
        &state.config,
        &state.http_client,
        state.fcm_auth.clone(),
        state.apns_auth.clone(),
        &request_id,
        body.approved,
        body.duration_sec,
        idempotency_key,
        "web",
    )
    .await?;

    audit_service::log_async(
        state.db.clone(),
        Some(user_id),
        "approval_decision".to_string(),
        Some(serde_json::json!({
            "request_id": request_id,
            "service_id": updated.service_id,
            "approved": body.approved,
            "channel": "web",
        })),
        None,
        None,
        None,
        None,
    );

    Ok(Json(DecideResponse {
        id: updated.id,
        status: updated.status,
        decided_at: updated.decided_at.map(|d| d.to_rfc3339()),
    }))
}

/// GET /api/v1/approvals/grants
pub async fn list_grants(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<GrantsQuery>,
) -> AppResult<Json<ApprovalGrantsResponse>> {
    let actor = auth_user.user_id.to_string();
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    // org_id query param scopes the listing to grants owned by an org
    // (created when an org-policy approval is granted in grant mode).
    // Caller must be an admin of the target org.
    let listing_user_id = if let Some(target_org_id) = query.org_id.as_deref() {
        let access =
            crate::services::org_service::resolve_owner_access(&state.db, &actor, target_org_id)
                .await?;
        if !access.can_write() {
            return Err(AppError::OrgRoleInsufficient(
                "admin access to the target org is required to list its approval grants"
                    .to_string(),
            ));
        }
        target_org_id.to_string()
    } else {
        actor
    };

    let (grants, total) =
        approval_service::list_grants(&state.db, &listing_user_id, page, per_page).await?;

    let items: Vec<ApprovalGrantItem> = grants
        .into_iter()
        .map(|g| ApprovalGrantItem {
            id: g.id,
            service_id: g.service_id,
            service_name: g.service_name,
            requester_type: g.requester_type,
            requester_id: g.requester_id,
            requester_label: g.requester_label,
            granted_at: g.granted_at.to_rfc3339(),
            expires_at: g.expires_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ApprovalGrantsResponse {
        grants: items,
        total,
        page,
        per_page,
    }))
}

/// DELETE /api/v1/approvals/grants/{grant_id}
pub async fn revoke_grant(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(grant_id): Path<String>,
    Query(query): Query<RevokeGrantQuery>,
) -> AppResult<Json<MessageResponse>> {
    let actor = auth_user.user_id.to_string();

    // Same pattern as list_grants: when org_id is supplied, the grant
    // is expected to be owned by the org and the caller must be an
    // admin of that org. Without this branch, org-policy grants would
    // be unrevokable through the API.
    let owner_user_id = if let Some(target_org_id) = query.org_id.as_deref() {
        let access =
            crate::services::org_service::resolve_owner_access(&state.db, &actor, target_org_id)
                .await?;
        if !access.can_write() {
            return Err(AppError::OrgRoleInsufficient(
                "admin access to the target org is required to revoke its approval grants"
                    .to_string(),
            ));
        }
        target_org_id.to_string()
    } else {
        actor.clone()
    };

    approval_service::revoke_grant(&state.db, &owner_user_id, &grant_id).await?;

    audit_service::log_async(
        state.db.clone(),
        Some(actor),
        "approval_grant_revoked".to_string(),
        Some(serde_json::json!({
            "grant_id": grant_id,
            "owner_user_id": owner_user_id,
        })),
        None,
        None,
        None,
        None,
    );

    Ok(Json(MessageResponse {
        message: "Grant revoked".to_string(),
    }))
}

// --- Per-service approval config types ---

#[derive(Debug, Serialize)]
pub struct ServiceApprovalConfigItem {
    pub service_id: String,
    pub service_name: String,
    pub approval_required: bool,
    pub approval_mode: ApprovalMode,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ServiceApprovalConfigsResponse {
    pub configs: Vec<ServiceApprovalConfigItem>,
}

#[derive(Debug, Deserialize)]
pub struct SetServiceApprovalConfigRequest {
    pub approval_required: Option<bool>,
    pub approval_mode: Option<ApprovalMode>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServiceApprovalConfigQuery {
    /// When set, the operation targets the given org's policy instead of
    /// the caller's personal scope. The caller must be an admin of that
    /// org. Used by the per-service approval CRUD endpoints so an org
    /// admin can set/list/delete policies on org-shared services.
    pub org_id: Option<String>,
}

/// Resolve the effective `user_id` for a service approval config
/// operation. Without `?org_id`, the actor manages their own personal
/// configs. With `?org_id=X`, the actor must be an admin of X and the
/// operation targets X's configs.
async fn resolve_service_config_owner(
    state: &AppState,
    actor: &str,
    org_id: Option<&str>,
) -> AppResult<String> {
    if let Some(org) = org_id {
        let access =
            crate::services::org_service::resolve_owner_access(&state.db, actor, org).await?;
        if !access.can_write() {
            return Err(AppError::OrgRoleInsufficient(
                "admin access to the target org is required to set per-service approval policy"
                    .to_string(),
            ));
        }
        Ok(org.to_string())
    } else {
        Ok(actor.to_string())
    }
}

// --- Per-service approval config handlers ---

/// GET /api/v1/approvals/service-configs
///
/// List per-service approval overrides for the current user, or for the
/// org passed via `?org_id=X` (caller must be admin of that org).
pub async fn list_service_configs(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<ServiceApprovalConfigQuery>,
) -> AppResult<Json<ServiceApprovalConfigsResponse>> {
    let actor = auth_user.user_id.to_string();
    let user_id = resolve_service_config_owner(&state, &actor, query.org_id.as_deref()).await?;

    let configs = approval_service::list_service_approval_configs(&state.db, &user_id).await?;

    let items: Vec<ServiceApprovalConfigItem> = configs
        .into_iter()
        .map(|c| ServiceApprovalConfigItem {
            service_id: c.service_id,
            service_name: c.service_name,
            approval_required: c.approval_required,
            approval_mode: c.approval_mode,
            created_at: c.created_at.to_rfc3339(),
            updated_at: c.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ServiceApprovalConfigsResponse { configs: items }))
}

/// PUT /api/v1/approvals/service-configs/{service_id}
///
/// Set a per-service approval override. Creates or updates. Pass
/// `?org_id=X` to set the policy on org X's behalf (caller must be admin).
pub async fn set_service_config(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceApprovalConfigQuery>,
    Json(body): Json<SetServiceApprovalConfigRequest>,
) -> AppResult<Json<ServiceApprovalConfigItem>> {
    let actor = auth_user.user_id.to_string();
    let user_id = resolve_service_config_owner(&state, &actor, query.org_id.as_deref()).await?;

    if body.approval_required.is_none() && body.approval_mode.is_none() {
        return Err(AppError::ValidationError(
            "At least one of approval_required or approval_mode must be provided".to_string(),
        ));
    }

    // Verify the service exists
    let service = state
        .db
        .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find_one(mongodb::bson::doc! { "_id": &service_id, "is_active": true })
        .await?
        .ok_or_else(|| AppError::NotFound("Service not found".to_string()))?;

    let config = approval_service::set_service_approval_config(
        &state.db,
        &user_id,
        &service_id,
        &service.name,
        body.approval_required,
        body.approval_mode.as_ref(),
    )
    .await?;

    audit_service::log_async(
        state.db.clone(),
        Some(actor),
        "service_approval_config_set".to_string(),
        Some(serde_json::json!({
            "service_id": service_id,
            "service_name": service.name,
            "policy_owner_user_id": user_id,
            "approval_required": config.approval_required,
            "approval_mode": config.approval_mode.as_str(),
        })),
        None,
        None,
        None,
        None,
    );

    Ok(Json(ServiceApprovalConfigItem {
        service_id: config.service_id,
        service_name: config.service_name,
        approval_required: config.approval_required,
        approval_mode: config.approval_mode,
        created_at: config.created_at.to_rfc3339(),
        updated_at: config.updated_at.to_rfc3339(),
    }))
}

/// DELETE /api/v1/approvals/service-configs/{service_id}
///
/// Remove a per-service approval override (revert to global default).
/// Pass `?org_id=X` to remove the policy on org X's behalf (admin only).
pub async fn delete_service_config(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceApprovalConfigQuery>,
) -> AppResult<Json<MessageResponse>> {
    let actor = auth_user.user_id.to_string();
    let user_id = resolve_service_config_owner(&state, &actor, query.org_id.as_deref()).await?;

    approval_service::delete_service_approval_config(&state.db, &user_id, &service_id).await?;

    audit_service::log_async(
        state.db.clone(),
        Some(actor),
        "service_approval_config_deleted".to_string(),
        Some(serde_json::json!({
            "service_id": service_id,
            "policy_owner_user_id": user_id,
        })),
        None,
        None,
        None,
        None,
    );

    Ok(Json(MessageResponse {
        message: "Per-service approval config removed".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::approval_request::ApprovalRequest;
    use chrono::Utc;

    fn sample_request(user_id: &str) -> ApprovalRequest {
        ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            service_id: uuid::Uuid::new_v4().to_string(),
            service_name: "OpenAI".to_string(),
            service_slug: "openai".to_string(),
            requester_type: "service_account".to_string(),
            requester_id: "sa_123".to_string(),
            requester_label: Some("CI bot".to_string()),
            operation_summary: "proxy:POST /v1/chat/completions".to_string(),
            action_description: Some("POST /v1/chat/completions (model: gpt-4)".to_string()),
            tool_name: None,
            tool_call_id: None,
            tool_arguments: None,
            is_destructive: None,
            approval_mode: ApprovalMode::PerRequest,
            status: "pending".to_string(),
            idempotency_key: "idem_123".to_string(),
            notification_channel: Some("fcm".to_string()),
            telegram_message_id: None,
            telegram_chat_id: None,
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            decided_at: None,
            decision_channel: None,
            decision_idempotency_key: None,
            notify_user_ids: vec![],
            created_at: Utc::now(),
        }
    }

    #[test]
    fn ensure_request_owned_by_user_allows_owner() {
        let result = ensure_request_owned_by_user("user_1", "user_1");
        assert!(result.is_ok());
    }

    #[test]
    fn ensure_request_owned_by_user_rejects_non_owner() {
        let result = ensure_request_owned_by_user("user_1", "user_2");
        assert!(matches!(result, Err(AppError::Forbidden(_))));
    }

    #[test]
    fn to_approval_request_item_maps_core_fields() {
        let request = sample_request("user_1");
        let expected_id = request.id.clone();
        let expected_service = request.service_name.clone();
        let expected_status = request.status.clone();

        let item = to_approval_request_item(request);

        assert_eq!(item.id, expected_id);
        assert_eq!(item.service_name, expected_service);
        assert_eq!(item.status, expected_status);
        assert!(item.created_at.contains('T'));
        // Proxy approvals have no tool fields
        assert!(item.tool_name.is_none());
        assert!(item.is_destructive.is_none());
    }

    #[test]
    fn to_approval_request_item_includes_tool_fields() {
        let mut request = sample_request("user_1");
        request.tool_name = Some("invoke_service".to_string());
        request.tool_call_id = Some("call_abc".to_string());
        request.tool_arguments = Some(r#"{"service_id":"svc_1"}"#.to_string());
        request.is_destructive = Some(true);

        let item = to_approval_request_item(request);

        assert_eq!(item.tool_name.as_deref(), Some("invoke_service"));
        assert_eq!(item.tool_call_id.as_deref(), Some("call_abc"));
        assert!(item.tool_arguments.is_some());
        assert_eq!(item.is_destructive, Some(true));
    }

    #[test]
    fn create_tool_approval_request_deserializes() {
        let json = r#"{
            "tool_name": "invoke_service",
            "tool_call_id": "call_123",
            "arguments": "{\"key\":\"val\"}",
            "is_destructive": true,
            "approval_mode": "alwaysrequire"
        }"#;
        let parsed: CreateToolApprovalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool_name, "invoke_service");
        assert_eq!(parsed.tool_call_id.as_deref(), Some("call_123"));
        assert_eq!(parsed.is_destructive, Some(true));
    }

    #[test]
    fn create_tool_approval_request_minimal() {
        let json = r#"{"tool_name": "list_services"}"#;
        let parsed: CreateToolApprovalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.tool_name, "list_services");
        assert!(parsed.tool_call_id.is_none());
        assert!(parsed.arguments.is_none());
        assert!(parsed.is_destructive.is_none());
    }
}
