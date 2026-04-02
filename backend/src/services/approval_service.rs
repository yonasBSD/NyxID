use chrono::{Duration, Utc};
use futures::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{self, doc};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};
use crate::models::approval_grant::{ApprovalGrant, COLLECTION_NAME as GRANTS};
use crate::models::approval_request::{ApprovalRequest, COLLECTION_NAME as REQUESTS};
use crate::models::notification_channel::{COLLECTION_NAME as CHANNELS, NotificationChannel};
use crate::models::service_approval_config::{
    ApprovalMode, COLLECTION_NAME as SERVICE_APPROVAL_CONFIGS, ServiceApprovalConfig,
};
use crate::services::notification_service;
use crate::services::push_service::{ApnsAuth, FcmAuth};

/// Resolve the approval mode for a (user, service) pair.
pub async fn resolve_approval_mode(
    db: &Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<ApprovalMode> {
    let per_service = db
        .collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS)
        .find_one(doc! { "user_id": user_id, "service_id": service_id })
        .await?;

    Ok(per_service.map(|c| c.approval_mode).unwrap_or_default())
}

/// Check whether a user has the global approval system enabled.
pub async fn user_requires_approval(db: &Database, user_id: &str) -> AppResult<bool> {
    Ok(user_global_approval_setting(db, user_id)
        .await?
        .unwrap_or(false))
}

/// Check whether approval is required for a specific service.
///
/// Resolution order:
/// 1. If a `ServiceApprovalConfig` exists for (user, service), use its value.
/// 2. Otherwise, fall back to the global `notification_channels.approval_required`.
pub async fn requires_approval_for_service(
    db: &Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<bool> {
    // Check per-service override first
    let per_service = db
        .collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS)
        .find_one(doc! { "user_id": user_id, "service_id": service_id })
        .await?;

    let global = user_requires_approval(db, user_id).await?;
    Ok(resolve_approval_requirement(
        per_service.map(|c| c.approval_required),
        Some(global),
    ))
}

async fn user_global_approval_setting(db: &Database, user_id: &str) -> AppResult<Option<bool>> {
    let channel = db
        .collection::<NotificationChannel>(CHANNELS)
        .find_one(doc! { "user_id": user_id })
        .await?;
    Ok(channel.map(|c| c.approval_required))
}

fn resolve_approval_requirement(per_service: Option<bool>, global: Option<bool>) -> bool {
    per_service.or(global).unwrap_or(false)
}

/// Check whether the request has a valid (non-expired, non-revoked) approval grant.
/// Returns Ok(true) if access is granted, Ok(false) if approval is needed.
pub async fn check_approval(
    db: &Database,
    user_id: &str,
    service_id: &str,
    requester_type: &str,
    requester_id: &str,
) -> AppResult<bool> {
    let now = bson::DateTime::from_chrono(Utc::now());

    let grant = db
        .collection::<ApprovalGrant>(GRANTS)
        .find_one(doc! {
            "user_id": user_id,
            "service_id": service_id,
            "requester_type": requester_type,
            "requester_id": requester_id,
            "revoked": false,
            "expires_at": { "$gt": now },
        })
        .await?;

    Ok(grant.is_some())
}

/// Create an approval request.
///
/// Grant mode keeps the legacy dedupe behavior for a pending
/// `(user, service, requester)` tuple.
/// Per-request mode always creates a distinct pending request so concurrent
/// calls cannot piggyback on a single approval.
#[allow(clippy::too_many_arguments)]
pub async fn create_approval_request(
    db: &Database,
    config: &AppConfig,
    http_client: &reqwest::Client,
    fcm_auth: Option<&FcmAuth>,
    apns_auth: Option<&ApnsAuth>,
    user_id: &str,
    service_id: &str,
    service_name: &str,
    service_slug: &str,
    requester_type: &str,
    requester_id: &str,
    requester_label: Option<&str>,
    operation_summary: &str,
    action_description: Option<&str>,
    approval_mode: ApprovalMode,
    timeout_secs: u32,
) -> AppResult<ApprovalRequest> {
    let collection = db.collection::<ApprovalRequest>(REQUESTS);
    let idempotency_key = compute_pending_request_idempotency_key(
        &approval_mode,
        user_id,
        service_id,
        requester_type,
        requester_id,
    );
    let mut inserted_request: Option<ApprovalRequest> = None;
    for _attempt in 0..2 {
        // Check for existing pending request with the same idempotency key.
        // This handles normal idempotent retries and the winner in concurrent inserts.
        if let Some(existing) = collection
            .find_one(doc! {
                "idempotency_key": &idempotency_key,
                "status": "pending",
            })
            .await?
        {
            return Ok(existing);
        }

        let now = Utc::now();
        let expires_at = now + Duration::seconds(i64::from(timeout_secs));

        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            service_id: service_id.to_string(),
            service_name: service_name.to_string(),
            service_slug: service_slug.to_string(),
            requester_type: requester_type.to_string(),
            requester_id: requester_id.to_string(),
            requester_label: requester_label.map(String::from),
            operation_summary: operation_summary.to_string(),
            action_description: action_description.map(String::from),
            tool_name: None,
            tool_call_id: None,
            tool_arguments: None,
            is_destructive: None,
            approval_mode: approval_mode.clone(),
            status: "pending".to_string(),
            idempotency_key: idempotency_key.clone(),
            notification_channel: None,
            telegram_message_id: None,
            telegram_chat_id: None,
            expires_at,
            decided_at: None,
            decision_channel: None,
            decision_idempotency_key: None,
            created_at: now,
        };

        match collection.insert_one(&request).await {
            Ok(_) => {
                inserted_request = Some(request);
                break;
            }
            Err(e) if is_duplicate_key_error(&e) => {
                // Concurrent insert race: another request inserted/processed first.
                // Retry once to read the pending row or create a new row if no longer pending.
                continue;
            }
            Err(e) => return Err(AppError::DatabaseError(e)),
        }
    }

    let request = inserted_request
        .ok_or_else(|| AppError::Conflict("Approval request conflict, please retry".to_string()))?;

    // Send notification
    match notification_service::send_approval_notification(
        db,
        config,
        http_client,
        fcm_auth,
        apns_auth,
        user_id,
        &request,
    )
    .await
    {
        Ok(result) => {
            // Update the request with notification details
            let channel_name = result.channels.join(",");
            let update = doc! {
                "$set": {
                    "notification_channel": &channel_name,
                    "telegram_chat_id": result.telegram_chat_id,
                    "telegram_message_id": result.telegram_message_id,
                }
            };
            collection
                .update_one(doc! { "_id": &request.id }, update)
                .await?;

            // Return the updated request
            let updated = collection
                .find_one(doc! { "_id": &request.id })
                .await?
                .unwrap_or(request);

            Ok(updated)
        }
        Err(e) => {
            tracing::warn!("Failed to send approval notification: {e}");
            // Still return the request even if notification failed --
            // user can approve via web UI
            Ok(request)
        }
    }
}

/// Create a tool approval request (from an external caller such as Aevatar).
///
/// Uses sentinel `service_id: "tool_approval"` and maps the tool name into
/// proxy-oriented fields so the existing notification and decision pipeline
/// works unchanged.
#[allow(clippy::too_many_arguments)]
pub async fn create_tool_approval_request(
    db: &Database,
    config: &AppConfig,
    http_client: &reqwest::Client,
    fcm_auth: Option<&FcmAuth>,
    apns_auth: Option<&ApnsAuth>,
    user_id: &str,
    tool_name: &str,
    tool_call_id: Option<&str>,
    tool_arguments: Option<&str>,
    is_destructive: bool,
    requester_type: &str,
    requester_id: &str,
    requester_label: Option<&str>,
) -> AppResult<ApprovalRequest> {
    let channel = db
        .collection::<NotificationChannel>(CHANNELS)
        .find_one(doc! { "user_id": user_id })
        .await?;

    let timeout_secs = channel
        .as_ref()
        .map(|c| c.approval_timeout_secs)
        .unwrap_or(30);

    let collection = db.collection::<ApprovalRequest>(REQUESTS);

    // Tool approvals are always unique (per_request semantics).
    let idempotency_key = uuid::Uuid::new_v4().to_string();

    let now = Utc::now();
    let expires_at = now + Duration::seconds(i64::from(timeout_secs));

    let operation_summary = format!("tool:{tool_name}");
    let action_description = tool_arguments.map(|args| format!("{tool_name}({args})"));

    let request = ApprovalRequest {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        service_id: "tool_approval".to_string(),
        service_name: tool_name.to_string(),
        service_slug: "tool".to_string(),
        requester_type: requester_type.to_string(),
        requester_id: requester_id.to_string(),
        requester_label: requester_label.map(String::from),
        operation_summary,
        action_description,
        tool_name: Some(tool_name.to_string()),
        tool_call_id: tool_call_id.map(String::from),
        tool_arguments: tool_arguments.map(String::from),
        is_destructive: Some(is_destructive),
        approval_mode: ApprovalMode::PerRequest,
        status: "pending".to_string(),
        idempotency_key,
        notification_channel: None,
        telegram_message_id: None,
        telegram_chat_id: None,
        expires_at,
        decided_at: None,
        decision_channel: None,
        decision_idempotency_key: None,
        created_at: now,
    };

    collection
        .insert_one(&request)
        .await
        .map_err(AppError::DatabaseError)?;

    // Send notification through existing pipeline
    match notification_service::send_approval_notification(
        db,
        config,
        http_client,
        fcm_auth,
        apns_auth,
        user_id,
        &request,
    )
    .await
    {
        Ok(result) => {
            let channel_name = result.channels.join(",");
            let update = doc! {
                "$set": {
                    "notification_channel": &channel_name,
                    "telegram_chat_id": result.telegram_chat_id,
                    "telegram_message_id": result.telegram_message_id,
                }
            };
            collection
                .update_one(doc! { "_id": &request.id }, update)
                .await?;

            let updated = collection
                .find_one(doc! { "_id": &request.id })
                .await?
                .unwrap_or(request);

            Ok(updated)
        }
        Err(e) => {
            tracing::warn!("Failed to send tool approval notification: {e}");
            Ok(request)
        }
    }
}

/// Process a user's approval decision (from Telegram callback or web UI).
/// Atomically updates status from "pending" to "approved"/"rejected".
/// On approval: creates an ApprovalGrant with the user's configured expiry.
#[allow(clippy::too_many_arguments)]
pub async fn process_decision(
    db: &Database,
    config: &AppConfig,
    http_client: &reqwest::Client,
    fcm_auth: Option<Arc<FcmAuth>>,
    apns_auth: Option<Arc<ApnsAuth>>,
    request_id: &str,
    approved: bool,
    duration_sec: Option<i64>,
    idempotency_key: Option<&str>,
    decision_channel: &str,
) -> AppResult<ApprovalRequest> {
    let now = Utc::now();
    let new_status = if approved { "approved" } else { "rejected" };
    let collection = db.collection::<ApprovalRequest>(REQUESTS);
    let mut update_set = doc! {
        "status": new_status,
        "decided_at": bson::DateTime::from_chrono(now),
        "decision_channel": decision_channel,
    };
    if let Some(key) = idempotency_key {
        update_set.insert("decision_idempotency_key", key);
    }

    // Atomic update: only process if status is still "pending"
    let updated = collection
        .find_one_and_update(
            doc! {
                "_id": request_id,
                "status": "pending",
            },
            doc! {
                "$set": update_set
            },
        )
        .with_options(
            FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build(),
        )
        .await?;

    let updated = match updated {
        Some(updated) => updated,
        None => {
            let existing = get_request(db, request_id).await?;
            // Provide a specific error for expired requests instead of the
            // generic "decision_state_conflict" from is_idempotent_replay.
            if existing.status == "expired" {
                return Err(AppError::Forbidden("Approval request expired".to_string()));
            }
            if is_idempotent_replay(
                &existing.status,
                existing.decision_idempotency_key.as_deref(),
                idempotency_key,
                approved,
            )? {
                return Ok(existing);
            }
            return Err(AppError::Conflict("already_decided".to_string()));
        }
    };

    // On approval: create a grant ONLY when the request was originally created
    // in grant mode AND the service is still in grant mode. This prevents:
    // - Stale grant-mode requests from minting grants after a switch to per_request (#146)
    // - Per-request requests from being upgraded to grants if the service switches to grant
    // The current-mode lookup is gated behind `approved` so rejections don't
    // gain a new failure path.
    //
    // Note: a TOCTOU race exists if a concurrent request switches the service to
    // per_request between this check and the insert_one below. If that happens,
    // the grant is inert: the proxy handler skips grants in per_request mode, and
    // list_grants() filters them out at read time. The grant will expire naturally.
    if approved
        && updated.approval_mode == ApprovalMode::Grant
        && resolve_approval_mode(db, &updated.user_id, &updated.service_id).await?
            == ApprovalMode::Grant
    {
        let channel = notification_service::get_or_create_channel(db, &updated.user_id).await?;
        let grant_expiry = resolve_grant_expiry(now, duration_sec, channel.grant_expiry_days);

        let grant = ApprovalGrant {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: updated.user_id.clone(),
            service_id: updated.service_id.clone(),
            service_name: updated.service_name.clone(),
            requester_type: updated.requester_type.clone(),
            requester_id: updated.requester_id.clone(),
            requester_label: updated.requester_label.clone(),
            approval_request_id: updated.id.clone(),
            granted_at: now,
            expires_at: grant_expiry,
            revoked: false,
        };

        db.collection::<ApprovalGrant>(GRANTS)
            .insert_one(&grant)
            .await?;
    }

    // Notify channels about the decision (best-effort, non-blocking).
    let request_clone = updated.clone();
    let config_clone = config.clone();
    let http_clone = http_client.clone();
    let db_clone = db.clone();
    let fcm_auth_clone = fcm_auth.clone();
    let apns_auth_clone = apns_auth.clone();
    tokio::spawn(async move {
        let _ = notification_service::notify_decision(
            &config_clone,
            &http_clone,
            fcm_auth_clone.as_deref(),
            apns_auth_clone.as_deref(),
            &db_clone,
            &request_clone,
            approved,
        )
        .await;
    });

    Ok(updated)
}

fn is_idempotent_replay(
    existing_status: &str,
    existing_decision_idempotency_key: Option<&str>,
    incoming_idempotency_key: Option<&str>,
    approved: bool,
) -> AppResult<bool> {
    let already_decided = existing_status == "approved" || existing_status == "rejected";

    if !already_decided {
        return Err(AppError::Conflict("decision_state_conflict".to_string()));
    }

    if let Some(key) = incoming_idempotency_key
        && existing_decision_idempotency_key == Some(key)
    {
        let existing_approved = existing_status == "approved";
        if existing_approved == approved {
            return Ok(true);
        }
        return Err(AppError::Conflict(
            "idempotency_key_reused_with_different_decision".to_string(),
        ));
    }

    Ok(false)
}

fn resolve_grant_expiry(
    now: chrono::DateTime<Utc>,
    duration_sec: Option<i64>,
    default_days: u32,
) -> chrono::DateTime<Utc> {
    if let Some(duration_sec) = duration_sec {
        return now + Duration::seconds(duration_sec);
    }
    now + Duration::days(i64::from(default_days))
}

/// Expire pending requests that have passed their expiry time.
/// Called by the background task.
pub async fn expire_pending_requests(
    db: &Database,
    config: &AppConfig,
    http_client: &reqwest::Client,
    fcm_auth: Option<&FcmAuth>,
    apns_auth: Option<&ApnsAuth>,
) -> AppResult<u64> {
    let sweep_time = bson::DateTime::from_chrono(Utc::now());
    let sweep_marker = format!("expiry:{}", uuid::Uuid::new_v4());

    let expired: Vec<ApprovalRequest> = db
        .collection::<ApprovalRequest>(REQUESTS)
        .find(doc! {
            "status": "pending",
            "expires_at": { "$lte": sweep_time },
        })
        .with_options(
            FindOptions::builder()
                .sort(doc! { "expires_at": 1 })
                .limit(100)
                .build(),
        )
        .await?
        .try_collect()
        .await?;

    if expired.is_empty() {
        return Ok(0);
    }

    // Batch update status to "expired", but ONLY if still pending.
    // This prevents a race where a user approves/rejects a request between
    // the find() above and this update_many(). Without the status guard,
    // the sweep could overwrite an "approved" status back to "expired".
    let ids: Vec<&str> = expired.iter().map(|r| r.id.as_str()).collect();
    let expire_result = db
        .collection::<ApprovalRequest>(REQUESTS)
        .update_many(
            doc! { "_id": { "$in": &ids }, "status": "pending" },
            doc! { "$set": {
                "status": "expired",
                "decided_at": sweep_time,
                "decision_idempotency_key": &sweep_marker,
            } },
        )
        .await?;

    let count = expire_result.modified_count;
    if count == 0 {
        return Ok(0);
    }

    // Re-query to get only the requests that were actually expired by the
    // update_many above. A request that was approved/rejected between the
    // initial find() and the update_many() will NOT have status "expired"
    // in the DB (the status guard prevented the overwrite), so it will be
    // excluded here. Matching on this sweep's unique marker also prevents
    // overlapping sweep tasks from re-sending each other's expiry side
    // effects for the same request IDs, even across multiple server
    // instances.
    let actually_expired = requests_updated_by_expiry_sweep(db, expired, &sweep_marker).await?;

    // Edit Telegram messages for expired requests (best-effort)
    for req in &actually_expired {
        if req
            .notification_channel
            .as_deref()
            .is_some_and(|ch| ch.contains("telegram"))
            && let (Some(chat_id), Some(message_id)) =
                (req.telegram_chat_id, req.telegram_message_id)
            && let Some(bot_token) = config.telegram_bot_token.as_deref()
        {
            let http = http_client.clone();
            let token = bot_token.to_string();
            let svc_name = req.service_name.clone();
            tokio::spawn(async move {
                let _ = crate::services::telegram_service::edit_message_after_decision(
                    &http,
                    &token,
                    chat_id,
                    message_id,
                    false,
                    &format!("{svc_name} (expired)"),
                )
                .await;
            });
        }
    }

    // Send silent push to update mobile app UI for expired requests (best-effort)
    for req in &actually_expired {
        let mut data = std::collections::HashMap::new();
        data.insert("type".to_string(), "approval_expired".to_string());
        data.insert("request_id".to_string(), req.id.clone());
        let _ = notification_service::send_silent_push_to_user(
            db,
            config,
            http_client,
            fcm_auth,
            apns_auth,
            &req.user_id,
            &data,
        )
        .await;
    }

    Ok(count)
}

async fn requests_updated_by_expiry_sweep(
    db: &Database,
    candidates: Vec<ApprovalRequest>,
    sweep_marker: &str,
) -> AppResult<Vec<ApprovalRequest>> {
    if candidates.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<&str> = candidates
        .iter()
        .map(|request| request.id.as_str())
        .collect();
    let expired_ids: HashSet<String> = db
        .collection::<bson::Document>(REQUESTS)
        .find(doc! {
            "_id": { "$in": &ids },
            "status": "expired",
            "decision_idempotency_key": sweep_marker,
        })
        .with_options(FindOptions::builder().projection(doc! { "_id": 1 }).build())
        .await?
        .try_collect::<Vec<bson::Document>>()
        .await?
        .into_iter()
        .filter_map(|doc| doc.get_str("_id").ok().map(str::to_owned))
        .collect();

    Ok(retain_requests_with_ids(candidates, &expired_ids))
}

fn retain_requests_with_ids(
    requests: Vec<ApprovalRequest>,
    allowed_ids: &HashSet<String>,
) -> Vec<ApprovalRequest> {
    requests
        .into_iter()
        .filter(|request| allowed_ids.contains(&request.id))
        .collect()
}

/// Block until an approval decision is made or the timeout expires.
/// Returns Ok(()) if approved, Err if rejected/expired/timeout.
pub async fn wait_for_decision(
    db: &Database,
    request_id: &str,
    timeout_secs: u32,
) -> AppResult<()> {
    let poll_interval = std::time::Duration::from_millis(1000);
    let deadline = Utc::now() + Duration::seconds(i64::from(timeout_secs));

    loop {
        tokio::time::sleep(poll_interval).await;

        let request = get_request(db, request_id).await?;

        match request.status.as_str() {
            "approved" => return Ok(()),
            "rejected" => {
                return Err(AppError::Forbidden(
                    "Approval request was rejected".to_string(),
                ));
            }
            "expired" => {
                return Err(AppError::Forbidden("Approval request expired".to_string()));
            }
            "pending" => {
                if Utc::now() >= deadline {
                    return Err(AppError::Forbidden(
                        "Approval request timed out".to_string(),
                    ));
                }
            }
            other => {
                return Err(AppError::Internal(format!(
                    "Unknown approval status: {other}"
                )));
            }
        }
    }
}

/// Convert user-facing approval decision failures into a richer error while
/// preserving unrelated backend errors so their original status/redaction
/// behavior remains intact.
pub fn map_wait_for_decision_error(
    error: AppError,
    request_id: &str,
    frontend_url: &str,
) -> AppError {
    match error {
        AppError::Forbidden(reason) => AppError::ApprovalFailed {
            request_id: request_id.to_string(),
            approve_url: format!("{}/approvals/history", frontend_url.trim_end_matches('/')),
            reason,
        },
        other => other,
    }
}

/// List approval requests for a user (for history page).
pub async fn list_requests(
    db: &Database,
    user_id: &str,
    status_filter: Option<&str>,
    page: u64,
    per_page: u64,
) -> AppResult<(Vec<ApprovalRequest>, u64)> {
    let mut filter = doc! { "user_id": user_id };
    if let Some(status) = status_filter {
        filter.insert("status", status);
    }

    let total = db
        .collection::<ApprovalRequest>(REQUESTS)
        .count_documents(filter.clone())
        .await?;

    let offset = (page.saturating_sub(1)) * per_page;
    let requests: Vec<ApprovalRequest> = db
        .collection::<ApprovalRequest>(REQUESTS)
        .find(filter)
        .with_options(
            FindOptions::builder()
                .sort(doc! { "created_at": -1 })
                .skip(offset)
                .limit(i64::try_from(per_page).unwrap_or(100))
                .build(),
        )
        .await?
        .try_collect()
        .await?;

    Ok((requests, total))
}

/// Get a single approval request by ID (for status polling).
pub async fn get_request(db: &Database, request_id: &str) -> AppResult<ApprovalRequest> {
    db.collection::<ApprovalRequest>(REQUESTS)
        .find_one(doc! { "_id": request_id })
        .await?
        .ok_or_else(|| AppError::NotFound("Approval request not found".to_string()))
}

/// List active approval grants for a user.
///
/// Only returns grants for services currently in `Grant` mode. Grants for
/// services in `PerRequest` mode (or with no config, which defaults to
/// `PerRequest`) are excluded even if they haven't been revoked yet. This
/// read-time filter acts as a safety net for any write-time race conditions
/// or partial failures during mode switches (see #146).
pub async fn list_grants(
    db: &Database,
    user_id: &str,
    page: u64,
    per_page: u64,
) -> AppResult<(Vec<ApprovalGrant>, u64)> {
    let now = bson::DateTime::from_chrono(Utc::now());

    // Collect service IDs that are explicitly in grant mode for this user.
    // Services with no config default to per_request, so their grants are excluded.
    let grant_mode_service_ids: Vec<String> = db
        .collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS)
        .find(doc! { "user_id": user_id, "approval_mode": "grant" })
        .await?
        .try_collect::<Vec<ServiceApprovalConfig>>()
        .await?
        .into_iter()
        .map(|c| c.service_id)
        .collect();

    let filter = doc! {
        "user_id": user_id,
        "revoked": false,
        "expires_at": { "$gt": now },
        "service_id": { "$in": &grant_mode_service_ids },
    };

    let total = db
        .collection::<ApprovalGrant>(GRANTS)
        .count_documents(filter.clone())
        .await?;

    let offset = (page.saturating_sub(1)) * per_page;
    let grants: Vec<ApprovalGrant> = db
        .collection::<ApprovalGrant>(GRANTS)
        .find(filter)
        .with_options(
            FindOptions::builder()
                .sort(doc! { "granted_at": -1 })
                .skip(offset)
                .limit(i64::try_from(per_page).unwrap_or(100))
                .build(),
        )
        .await?
        .try_collect()
        .await?;

    Ok((grants, total))
}

/// Revoke a specific approval grant.
pub async fn revoke_grant(db: &Database, user_id: &str, grant_id: &str) -> AppResult<()> {
    let result = db
        .collection::<ApprovalGrant>(GRANTS)
        .update_one(
            doc! {
                "_id": grant_id,
                "user_id": user_id,
            },
            doc! { "$set": { "revoked": true } },
        )
        .await?;

    if result.matched_count == 0 {
        return Err(AppError::NotFound("Grant not found".to_string()));
    }

    Ok(())
}

/// Revoke all grants for a user.
#[allow(dead_code)]
pub async fn revoke_all_grants(db: &Database, user_id: &str) -> AppResult<u64> {
    let result = db
        .collection::<ApprovalGrant>(GRANTS)
        .update_many(
            doc! { "user_id": user_id, "revoked": false },
            doc! { "$set": { "revoked": true } },
        )
        .await?;

    Ok(result.modified_count)
}

/// List per-service approval configs for a user.
pub async fn list_service_approval_configs(
    db: &Database,
    user_id: &str,
) -> AppResult<Vec<ServiceApprovalConfig>> {
    let configs: Vec<ServiceApprovalConfig> = db
        .collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS)
        .find(doc! { "user_id": user_id })
        .await?
        .try_collect()
        .await?;

    Ok(configs)
}

/// Set a per-service approval config (atomic upsert).
/// If a config already exists for (user, service), it is updated.
/// Otherwise, a new config is created. Uses `findOneAndUpdate` with
/// `upsert: true` to avoid race conditions from concurrent requests.
pub async fn set_service_approval_config(
    db: &Database,
    user_id: &str,
    service_id: &str,
    service_name: &str,
    approval_required: Option<bool>,
    approval_mode: Option<&ApprovalMode>,
) -> AppResult<ServiceApprovalConfig> {
    let now = bson::DateTime::from_chrono(Utc::now());
    let collection = db.collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS);
    let filter = doc! { "user_id": user_id, "service_id": service_id };
    let existing = collection.find_one(filter.clone()).await?;
    let (approval_required, approval_mode) =
        resolve_service_config_update(existing.as_ref(), approval_required, approval_mode)?;

    for _attempt in 0..2 {
        let config = collection
            .find_one_and_update(
                filter.clone(),
                doc! {
                    "$set": {
                        "approval_required": approval_required,
                        "approval_mode": approval_mode.as_str(),
                        "service_name": service_name,
                        "updated_at": now,
                    },
                    "$setOnInsert": {
                        "_id": uuid::Uuid::new_v4().to_string(),
                        "user_id": user_id,
                        "service_id": service_id,
                        "created_at": now,
                    }
                },
            )
            .with_options(
                FindOneAndUpdateOptions::builder()
                    .upsert(true)
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await;

        match config {
            Ok(Some(cfg)) => {
                // Revoke stale grants when the persisted mode is per_request.
                // Idempotent: re-revoking already-revoked grants is a no-op,
                // so retries after a partial failure still clean up (see #146).
                if cfg.approval_mode == ApprovalMode::PerRequest {
                    revoke_grants_for_service(db, user_id, service_id).await?;
                }
                return Ok(cfg);
            }
            Ok(None) => {
                return Err(AppError::Internal(
                    "Upsert returned no document".to_string(),
                ));
            }
            Err(e) if is_duplicate_key_error(&e) => {
                // Concurrent upserts can race on the unique (user_id, service_id) index.
                // Read-after-write resolves to the winning document.
                if let Some(existing) = collection.find_one(filter.clone()).await? {
                    if existing.approval_mode == ApprovalMode::PerRequest {
                        revoke_grants_for_service(db, user_id, service_id).await?;
                    }
                    return Ok(existing);
                }
                continue;
            }
            Err(e) => return Err(AppError::DatabaseError(e)),
        }
    }

    Err(AppError::Conflict(
        "Per-service approval config update conflicted, please retry".to_string(),
    ))
}

/// Delete a per-service approval config (revert to global default).
/// Because the global default mode is `PerRequest`, deleting any override
/// effectively switches the service to `per_request`. Active grants are
/// revoked unconditionally so they don't linger (see #146). The revoke is
/// idempotent, so retries after partial failure still clean up.
pub async fn delete_service_approval_config(
    db: &Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<()> {
    let result = db
        .collection::<ServiceApprovalConfig>(SERVICE_APPROVAL_CONFIGS)
        .delete_one(doc! { "user_id": user_id, "service_id": service_id })
        .await?;

    // Always revoke grants regardless of deleted_count. If a previous attempt
    // deleted the config but failed on revoke, this retry still cleans up.
    // The revoke is a no-op when no active grants exist.
    revoke_grants_for_service(db, user_id, service_id).await?;

    if result.deleted_count == 0 {
        return Err(AppError::NotFound(
            "Per-service approval config not found".to_string(),
        ));
    }

    Ok(())
}

fn compute_idempotency_key(
    user_id: &str,
    service_id: &str,
    requester_type: &str,
    requester_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    hasher.update(b":");
    hasher.update(service_id.as_bytes());
    hasher.update(b":");
    hasher.update(requester_type.as_bytes());
    hasher.update(b":");
    hasher.update(requester_id.as_bytes());
    hex::encode(hasher.finalize())
}

fn compute_pending_request_idempotency_key(
    approval_mode: &ApprovalMode,
    user_id: &str,
    service_id: &str,
    requester_type: &str,
    requester_id: &str,
) -> String {
    match approval_mode {
        ApprovalMode::Grant => {
            compute_idempotency_key(user_id, service_id, requester_type, requester_id)
        }
        ApprovalMode::PerRequest => uuid::Uuid::new_v4().to_string(),
    }
}

fn resolve_service_config_update(
    existing: Option<&ServiceApprovalConfig>,
    approval_required: Option<bool>,
    approval_mode: Option<&ApprovalMode>,
) -> AppResult<(bool, ApprovalMode)> {
    let resolved_required = approval_required
        .or_else(|| existing.map(|config| config.approval_required))
        .ok_or_else(|| {
            AppError::ValidationError(
                "approval_required is required when creating a new per-service approval config"
                    .to_string(),
            )
        })?;

    let resolved_mode = approval_mode
        .cloned()
        .or_else(|| existing.map(|config| config.approval_mode.clone()))
        .unwrap_or_default();

    Ok((resolved_required, resolved_mode))
}

/// Revoke all active (non-revoked) grants for a (user, service) pair.
/// Called after switching a service to `per_request` mode so stale grants
/// no longer appear in the UI (see #146).
async fn revoke_grants_for_service(
    db: &Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<()> {
    db.collection::<ApprovalGrant>(GRANTS)
        .update_many(
            doc! {
                "user_id": user_id,
                "service_id": service_id,
                "revoked": false,
            },
            doc! { "$set": { "revoked": true } },
        )
        .await?;
    Ok(())
}

fn is_duplicate_key_error(e: &mongodb::error::Error) -> bool {
    if let mongodb::error::ErrorKind::Write(mongodb::error::WriteFailure::WriteError(we)) =
        e.kind.as_ref()
    {
        return we.code == 11000;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(id: &str, status: &str) -> ApprovalRequest {
        let now = Utc::now();
        ApprovalRequest {
            id: id.to_string(),
            user_id: "user-1".to_string(),
            service_id: "service-1".to_string(),
            service_name: "OpenAI API".to_string(),
            service_slug: "openai".to_string(),
            requester_type: "service_account".to_string(),
            requester_id: "requester-1".to_string(),
            requester_label: Some("CI Pipeline".to_string()),
            operation_summary: "proxy:POST /v1/chat/completions".to_string(),
            status: status.to_string(),
            idempotency_key: format!("idem-{id}"),
            notification_channel: Some("telegram".to_string()),
            telegram_message_id: Some(12345),
            telegram_chat_id: Some(67890),
            expires_at: now,
            decided_at: None,
            decision_channel: None,
            decision_idempotency_key: None,
            action_description: None,
            tool_name: None,
            tool_call_id: None,
            tool_arguments: None,
            is_destructive: None,
            approval_mode: ApprovalMode::PerRequest,
            created_at: now,
        }
    }

    #[test]
    fn resolve_approval_requirement_prefers_per_service_true_over_global_false() {
        assert!(resolve_approval_requirement(Some(true), Some(false)));
    }

    #[test]
    fn resolve_approval_requirement_prefers_per_service_false_over_global_true() {
        assert!(!resolve_approval_requirement(Some(false), Some(true)));
    }

    #[test]
    fn resolve_approval_requirement_falls_back_to_global_when_no_per_service() {
        assert!(resolve_approval_requirement(None, Some(true)));
        assert!(!resolve_approval_requirement(None, Some(false)));
    }

    #[test]
    fn resolve_approval_requirement_defaults_to_false_when_no_settings() {
        assert!(!resolve_approval_requirement(None, None));
    }

    #[test]
    fn grant_mode_idempotency_key_deterministic() {
        let key1 = compute_pending_request_idempotency_key(
            &ApprovalMode::Grant,
            "user1",
            "svc1",
            "sa",
            "req1",
        );
        let key2 = compute_pending_request_idempotency_key(
            &ApprovalMode::Grant,
            "user1",
            "svc1",
            "sa",
            "req1",
        );
        assert_eq!(key1, key2);
    }

    #[test]
    fn grant_mode_idempotency_key_differs_for_different_inputs() {
        let key1 = compute_pending_request_idempotency_key(
            &ApprovalMode::Grant,
            "user1",
            "svc1",
            "sa",
            "req1",
        );
        let key2 = compute_pending_request_idempotency_key(
            &ApprovalMode::Grant,
            "user2",
            "svc1",
            "sa",
            "req1",
        );
        assert_ne!(key1, key2);
    }

    #[test]
    fn grant_mode_idempotency_key_is_hex_sha256() {
        let key = compute_pending_request_idempotency_key(&ApprovalMode::Grant, "u", "s", "t", "r");
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn per_request_idempotency_key_is_unique_uuid() {
        let key1 = compute_pending_request_idempotency_key(
            &ApprovalMode::PerRequest,
            "user1",
            "svc1",
            "sa",
            "req1",
        );
        let key2 = compute_pending_request_idempotency_key(
            &ApprovalMode::PerRequest,
            "user1",
            "svc1",
            "sa",
            "req1",
        );

        assert_ne!(key1, key2);
        assert!(uuid::Uuid::parse_str(&key1).is_ok());
        assert!(uuid::Uuid::parse_str(&key2).is_ok());
    }

    #[test]
    fn idempotent_replay_allows_same_key_same_decision() {
        let result = is_idempotent_replay("approved", Some("k1"), Some("k1"), true).expect("ok");
        assert!(result);
    }

    #[test]
    fn idempotent_replay_rejects_same_key_different_decision() {
        let result = is_idempotent_replay("approved", Some("k1"), Some("k1"), false);
        assert!(matches!(
            result,
            Err(AppError::Conflict(msg)) if msg == "idempotency_key_reused_with_different_decision"
        ));
    }

    #[test]
    fn idempotent_replay_rejects_without_matching_key() {
        let result = is_idempotent_replay("approved", Some("k1"), Some("k2"), true).expect("ok");
        assert!(!result);
    }

    #[test]
    fn resolve_grant_expiry_prefers_duration_seconds() {
        let now = Utc::now();
        let expiry = resolve_grant_expiry(now, Some(3600), 30);
        assert_eq!(expiry, now + Duration::seconds(3600));
    }

    #[test]
    fn resolve_grant_expiry_falls_back_to_default_days() {
        let now = Utc::now();
        let expiry = resolve_grant_expiry(now, None, 30);
        assert_eq!(expiry, now + Duration::days(30));
    }

    #[test]
    fn idempotent_replay_returns_conflict_for_expired_status() {
        // When a request has been marked "expired" (by the background sweep),
        // is_idempotent_replay should return Err(Conflict) because "expired"
        // is not a user decision ("approved"/"rejected").
        let result = is_idempotent_replay("expired", None, Some("k1"), true);
        assert!(matches!(result, Err(AppError::Conflict(msg)) if msg == "decision_state_conflict"));
    }

    #[test]
    fn idempotent_replay_returns_conflict_for_pending_status() {
        // "pending" is not a decided state either.
        let result = is_idempotent_replay("pending", None, Some("k1"), true);
        assert!(matches!(result, Err(AppError::Conflict(msg)) if msg == "decision_state_conflict"));
    }

    #[test]
    fn retain_requests_with_ids_excludes_requests_not_updated_by_sweep() {
        let candidates = vec![
            make_request("req-expired", "pending"),
            make_request("req-approved", "pending"),
        ];
        let actually_expired_ids = HashSet::from_iter([String::from("req-expired")]);

        let retained = retain_requests_with_ids(candidates, &actually_expired_ids);

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, "req-expired");
    }

    /// Regression test for #96: simulates the race window between find() and
    /// update_many() in expire_pending_requests(). When a user approves a
    /// request after the sweep's initial find() but before the guarded
    /// update_many(), the sweep must NOT send side effects (Telegram edits,
    /// push notifications) for that request.
    ///
    /// This test exercises the filtering pipeline that runs after
    /// update_many(): only requests whose IDs appear in the "actually
    /// expired" set (those that matched `status: "pending"` at update time
    /// AND carry this sweep's unique marker) should trigger side effects.
    #[test]
    fn race_window_approved_request_excluded_from_expiry_side_effects() {
        // Scenario: sweep finds 3 pending requests that have passed their
        // expiry time. Between find() and update_many():
        //  - req-2 was approved by the user (status changed to "approved")
        //  - req-3 was rejected by the user (status changed to "rejected")
        // Only req-1 remained "pending" and was actually expired.
        let candidates = vec![
            make_request("req-1", "pending"),
            make_request("req-2", "pending"), // concurrently approved
            make_request("req-3", "pending"), // concurrently rejected
        ];

        // After update_many with `status: "pending"` guard, only req-1 was
        // modified. The re-query (requests_updated_by_expiry_sweep) returns
        // only IDs that have status "expired" + this sweep's marker.
        let actually_expired_ids = HashSet::from_iter([String::from("req-1")]);

        let side_effect_targets = retain_requests_with_ids(candidates, &actually_expired_ids);

        // req-2 and req-3 must NOT appear — they were decided by the user
        // during the race window and must not receive expiry side effects.
        assert_eq!(side_effect_targets.len(), 1);
        assert_eq!(side_effect_targets[0].id, "req-1");
    }

    /// Regression test for #96: when ALL requests in the sweep's find()
    /// were concurrently decided by users, the side-effect list must be
    /// completely empty.
    #[test]
    fn race_window_all_requests_decided_yields_no_side_effects() {
        let candidates = vec![
            make_request("req-a", "pending"),
            make_request("req-b", "pending"),
        ];

        // None of the candidates were actually expired (all were decided
        // between find() and update_many()).
        let actually_expired_ids: HashSet<String> = HashSet::new();

        let side_effect_targets = retain_requests_with_ids(candidates, &actually_expired_ids);

        assert!(
            side_effect_targets.is_empty(),
            "No side effects should be emitted when all requests were decided during the race window"
        );
    }

    #[test]
    fn resolve_service_config_update_preserves_existing_mode_when_omitted() {
        let existing = ServiceApprovalConfig {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: "user1".to_string(),
            service_id: "svc1".to_string(),
            service_name: "OpenAI".to_string(),
            approval_required: true,
            approval_mode: ApprovalMode::Grant,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let (approval_required, approval_mode) =
            resolve_service_config_update(Some(&existing), Some(false), None).expect("ok");

        assert!(!approval_required);
        assert_eq!(approval_mode, ApprovalMode::Grant);
    }

    #[test]
    fn resolve_service_config_update_requires_approval_required_for_new_config() {
        let result = resolve_service_config_update(None, None, Some(&ApprovalMode::Grant));
        assert!(matches!(result, Err(AppError::ValidationError(_))));
    }

    #[test]
    fn map_wait_for_decision_error_wraps_user_decision_failures() {
        let error = map_wait_for_decision_error(
            AppError::Forbidden("Approval request timed out".to_string()),
            "req-1",
            "https://app.nyxid.dev/",
        );

        assert!(matches!(
            error,
            AppError::ApprovalFailed {
                request_id,
                approve_url,
                reason,
            } if request_id == "req-1"
                && approve_url == "https://app.nyxid.dev/approvals/history"
                && reason == "Approval request timed out"
        ));
    }

    #[test]
    fn map_wait_for_decision_error_preserves_internal_failures() {
        let error = map_wait_for_decision_error(
            AppError::DatabaseError(mongodb::error::Error::custom("db unavailable")),
            "req-1",
            "https://app.nyxid.dev",
        );

        assert!(matches!(error, AppError::DatabaseError(_)));
    }
}
