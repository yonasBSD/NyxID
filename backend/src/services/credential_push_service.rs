use std::sync::Arc;

use futures::TryStreamExt;
use mongodb::bson::doc;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::services::node_service;
use crate::services::node_ws_manager::{CredentialUpdateParams, NodeWsManager};

/// API-key scope to apply during a rotation-time push fan-out.
///
/// Mirrors the node + service scope fields on `AuthUser`. A credential
/// shared across multiple `UserService`s can reach nodes the caller
/// does not own (node dim) AND services the caller does not have in
/// `allowed_service_ids` (service dim). The fan-out must enforce
/// *both* so external-key rotation cannot be used to rewrite
/// out-of-scope siblings (thirtieth-round Codex P1 — service dim
/// added, node dim inherited from the twenty-ninth-round fix).
pub struct ActorScope {
    pub allow_all_nodes: bool,
    pub allowed_node_ids: Vec<String>,
    pub allow_all_services: bool,
    pub allowed_service_ids: Vec<String>,
}

impl ActorScope {
    /// Construct an unrestricted scope — used by session / admin
    /// callers and internal services that aren't API-key gated.
    #[allow(dead_code)]
    pub fn unrestricted() -> Self {
        Self {
            allow_all_nodes: true,
            allowed_node_ids: vec![],
            allow_all_services: true,
            allowed_service_ids: vec![],
        }
    }

    fn permits_node(&self, node_id: &str) -> bool {
        self.allow_all_nodes || self.allowed_node_ids.iter().any(|id| id == node_id)
    }

    fn permits_service(&self, service_id: &str) -> bool {
        self.allow_all_services || self.allowed_service_ids.iter().any(|id| id == service_id)
    }
}

/// Fire-and-forget push that also verifies node ownership per target
/// before delivery. Used by the `/api/v1/api-keys/external/:id`
/// rotation path where the caller authenticated to edit the `UserApiKey`
/// but hasn't passed the per-node ownership check that `PUT /keys`
/// applies via `ensure_node_writable_by_actor`. Without this filter, an
/// org admin or scoped key editor could rewrite credentials on nodes
/// they don't control (twenty-first-round Codex P1).
///
/// Additionally honors `actor_node_scope`: a scoped API key with an
/// `allowed_node_ids` allow-list cannot use external-key rotation as a
/// back door to rewrite credentials on out-of-scope nodes owned by the
/// same user (thirtieth-round Codex P1). Mirrors the scope enforcement
/// `execute_tool` applies at proxy time.
///
/// Services whose node the actor cannot write to OR whose node is
/// outside the caller's API-key scope are silently skipped (logged as a
/// debug/info line). Services on nodes that pass both checks are pushed
/// through `push_credential_to_node_if_routed` as usual.
pub async fn push_credential_to_node_if_owned(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    actor_user_id: &str,
    api_key_id: &str,
    actor_scope: ActorScope,
) {
    let services: Vec<UserService> = match db
        .collection::<UserService>(USER_SERVICES)
        .find(doc! {
            "user_id": user_id,
            "api_key_id": api_key_id,
            "node_id": { "$ne": null },
            "is_active": true,
            "auth_method": { "$ne": "none" },
        })
        .await
    {
        Ok(cursor) => cursor.try_collect().await.unwrap_or_default(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to query UserServices for owned credential push");
            return;
        }
    };

    if services.is_empty() {
        return;
    }

    let api_key = match db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": api_key_id })
        .await
    {
        Ok(Some(k)) => k,
        _ => {
            tracing::warn!(api_key_id = %api_key_id, "UserApiKey not found for owned credential push");
            return;
        }
    };

    let credential = match decrypt_api_key_credential(&api_key, encryption_keys).await {
        Some(c) => c,
        None => {
            tracing::warn!(api_key_id = %api_key_id, "No credential to push");
            return;
        }
    };

    for svc in &services {
        let Some(node_id) = svc.node_id.as_deref() else {
            continue;
        };

        // API-key service scope check: when a credential is shared
        // across multiple services, a scoped key whose
        // `allowed_service_ids` authorizes only one sibling must not
        // use rotation of the shared credential to overwrite node-
        // local credentials for the other siblings — that would be a
        // back door past the service allow-list
        // (thirty-first-round Codex P1). Runs before the node-scope
        // check so we short-circuit even before touching the node
        // ownership query.
        if !actor_scope.permits_service(&svc.id) {
            tracing::info!(
                actor_user_id = %actor_user_id,
                service_id = %svc.id,
                service_slug = %svc.slug,
                "Skipping credential push: service is outside actor API-key scope"
            );
            continue;
        }

        // API-key node scope check: a key limited to a subset of nodes
        // must not use external-key rotation to rewrite credentials on
        // out-of-scope nodes, even when the underlying user owns them
        // (thirtieth-round Codex P1).
        if !actor_scope.permits_node(node_id) {
            tracing::info!(
                actor_user_id = %actor_user_id,
                node_id = %node_id,
                service_slug = %svc.slug,
                "Skipping credential push: target node is outside actor API-key scope"
            );
            continue;
        }

        // Per-node ownership check: only push to nodes the actor can
        // write to. Failure here is logged but doesn't abort the rest
        // of the rotation — some nodes may be legitimately out of
        // scope for this actor while others are rotate-worthy.
        if node_service::ensure_node_writable_by_actor(db, actor_user_id, node_id)
            .await
            .is_err()
        {
            tracing::info!(
                actor_user_id = %actor_user_id,
                node_id = %node_id,
                service_slug = %svc.slug,
                "Skipping credential push: actor does not own the target node"
            );
            continue;
        }

        let target_url = match db
            .collection::<UserEndpoint>(USER_ENDPOINTS)
            .find_one(doc! { "_id": &svc.endpoint_id })
            .await
        {
            Ok(Some(ep)) if !ep.url.is_empty() => Some(ep.url),
            _ => None,
        };
        let params = build_credential_params(svc, &credential, target_url);
        if let Err(e) = node_ws_manager.send_credential_update(node_id, &params) {
            tracing::warn!(
                node_id = %node_id,
                service_slug = %svc.slug,
                error = %e,
                "Failed to push credential to owned node (node may be offline)"
            );
        }
    }
}

/// After a credential is stored or refreshed, check if any UserService
/// referencing this UserApiKey is node-routed. If so, push the credential
/// to the connected node.
///
/// This is fire-and-forget: errors are logged but not propagated.
pub async fn push_credential_to_node_if_routed(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    api_key_id: &str,
) {
    // Find UserServices that reference this api_key and have a node_id.
    // Exclude services downgraded to `auth_method: "none"`: pushing the
    // rotated secret would silently re-enable injection on a node for a
    // service the user explicitly turned off. Nineteenth-round Codex P2.
    let services: Vec<UserService> = match db
        .collection::<UserService>(USER_SERVICES)
        .find(doc! {
            "user_id": user_id,
            "api_key_id": api_key_id,
            "node_id": { "$ne": null },
            "is_active": true,
            "auth_method": { "$ne": "none" },
        })
        .await
    {
        Ok(cursor) => match cursor.try_collect().await {
            Ok(svcs) => svcs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to query UserServices for credential push");
                return;
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "Failed to query UserServices for credential push");
            return;
        }
    };

    if services.is_empty() {
        return;
    }

    // Load the UserApiKey to get the decrypted credential
    let api_key = match db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": api_key_id })
        .await
    {
        Ok(Some(k)) => k,
        Ok(None) => {
            tracing::warn!(api_key_id = %api_key_id, "UserApiKey not found for credential push");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load UserApiKey for credential push");
            return;
        }
    };

    // Decrypt the credential
    let credential = match decrypt_api_key_credential(&api_key, encryption_keys).await {
        Some(c) => c,
        None => {
            tracing::warn!(api_key_id = %api_key_id, "No credential to push");
            return;
        }
    };

    for svc in &services {
        let node_id = match &svc.node_id {
            Some(id) => id,
            None => continue,
        };

        // Load endpoint URL for target_url field
        let target_url = match db
            .collection::<UserEndpoint>(USER_ENDPOINTS)
            .find_one(doc! { "_id": &svc.endpoint_id })
            .await
        {
            Ok(Some(ep)) if !ep.url.is_empty() => Some(ep.url),
            _ => None,
        };

        let params = build_credential_params(svc, &credential, target_url);

        if let Err(e) = node_ws_manager.send_credential_update(node_id, &params) {
            tracing::warn!(
                node_id = %node_id,
                service_slug = %svc.slug,
                error = %e,
                "Failed to push credential to node (node may be offline)"
            );
        }
    }
}

/// Effective post-update fields that the strict push uses to build the
/// `credential_update` WS frame. The caller (the `PUT /keys` handler)
/// composes these from `body + view` before persisting the
/// `UserService` mutation, so a push failure leaves no partial state
/// behind (fourth-round Codex review P1).
pub struct StrictPushTarget<'a> {
    pub target_node_id: &'a str,
    pub service_slug: &'a str,
    pub auth_method: &'a str,
    pub auth_key_name: &'a str,
    pub target_url: Option<&'a str>,
}

/// Strict variant of `push_credential_to_node_if_routed` used by the
/// `PUT /keys` flow that just stored a fresh credential server-side. The
/// caller gates subsequent `UserService` writes and
/// `activate_node_managed_api_key` on a successful return here — if this
/// function errors (node offline, WS buffer full, serialization failure),
/// the handler aborts the request so the credential stays on the server
/// for a later retry instead of being silently lost (first Codex review
/// P1), and no routing/auth mutation has been persisted yet.
///
/// Takes `target` fields directly rather than re-reading `UserService`
/// from the database — the caller knows the *effective* post-update
/// values (`node_id`, `auth_method`, endpoint URL, …) and we need to
/// push to *those*, not to whatever the DB still shows from before.
///
/// **Best-effort delivery semantics.** "Strict" here means the WS frame
/// was queued on the node's outbound channel — it does NOT wait for a
/// `credential_update_ack` from the node. If the node accepts the frame
/// but later fails to persist it (keychain / config write error), the
/// handler still returns success and commits routing mutations.
/// Follow-up: add request-id + oneshot pending waiter so callers can
/// block on `credential_update_ack` with a timeout (twentieth-round
/// Codex P1). The existing push-before-mutation ordering and the
/// server-side credential preservation in `reconcile_provider_key_for_
/// service_routing` mean a failed node-side apply leaves the server copy
/// intact for the user to retry via the same PUT, so the worst case is
/// temporary proxy failure, not credential loss.
pub async fn push_credential_to_node_strict(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    api_key_id: &str,
    target: StrictPushTarget<'_>,
) -> AppResult<()> {
    let api_key = db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": api_key_id, "user_id": user_id })
        .await?
        .ok_or_else(|| {
            AppError::NotFound("API key disappeared before credential push could run".to_string())
        })?;

    let credential = decrypt_api_key_credential(&api_key, encryption_keys)
        .await
        .ok_or_else(|| {
            AppError::Internal(
                "Failed to decrypt credential for node push; secret cannot be delivered"
                    .to_string(),
            )
        })?;

    let params = build_credential_params_from_fields(
        target.service_slug,
        target.auth_method,
        target.auth_key_name,
        &credential,
        target.target_url.map(str::to_string),
    );

    // Capability-aware delivery. New node agents advertise
    // `credential_ack_correlation` in their `status_update` — the
    // backend then uses strict ack-wait so a node-side apply failure
    // fails the PUT. Older agents don't advertise the flag; we fall
    // back to the best-effort `send_credential_update` (returns Ok
    // once the frame is queued). This lets backend + node roll out
    // independently (twenty-seventh-round Codex P2).
    //
    // Wait briefly for capability resolution before classifying the
    // node. Without this, a PUT that lands in the short window after
    // the WS auth handshake but before the node's first
    // `status_update` would see `supports_credential_ack_correlation
    // == false` and silently downgrade to fire-and-forget on an
    // otherwise-strict agent (twenty-ninth-round Codex P2). If the
    // first status_update never arrives within the timeout we fall
    // through — old agents that never send one must still make
    // progress.
    node_ws_manager
        .await_capability_resolution(target.target_node_id, std::time::Duration::from_millis(500))
        .await;

    if node_ws_manager.supports_credential_ack_correlation(target.target_node_id) {
        node_ws_manager
            .send_credential_update_and_wait(
                target.target_node_id,
                &params,
                std::time::Duration::from_secs(10),
            )
            .await?;
    } else {
        node_ws_manager.send_credential_update(target.target_node_id, &params)?;
        tracing::info!(
            node_id = %target.target_node_id,
            service_slug = %target.service_slug,
            "Pushed credential in legacy mode (node has not advertised credential_ack_correlation)"
        );
    }
    Ok(())
}

/// Push a `"no-auth"` placeholder to the node: preserves (or sets) the
/// slug's `target_url` but drops any stored secret, leaving
/// `proxy_executor` able to resolve the downstream without the "No
/// credentials configured" 502 that a raw `credential_remove` produces.
/// Used when `PUT /keys` downgrades `auth_method` to `"none"` on a
/// same-node routed service (thirty-third-round Codex P1). Best-effort
/// delivery: logs failures instead of aborting, since the DB
/// mutation has already committed by the time this is called.
pub async fn push_no_auth_to_node(
    node_ws_manager: &Arc<NodeWsManager>,
    node_id: &str,
    service_slug: &str,
    target_url: Option<&str>,
) {
    let params = CredentialUpdateParams {
        service_slug: service_slug.to_string(),
        injection_method: "none".to_string(),
        header_name: None,
        header_value: None,
        param_name: None,
        param_value: None,
        target_url: target_url.map(str::to_string),
    };

    node_ws_manager
        .await_capability_resolution(node_id, std::time::Duration::from_millis(500))
        .await;

    if node_ws_manager.supports_credential_ack_correlation(node_id) {
        if let Err(e) = node_ws_manager
            .send_credential_update_and_wait(node_id, &params, std::time::Duration::from_secs(10))
            .await
        {
            tracing::warn!(
                node_id = %node_id,
                service_slug = %service_slug,
                error = %e,
                "no-auth placeholder push did not ack cleanly — node may keep injecting the old secret. Run `nyxid node credentials remove` on the node to clean up"
            );
        }
    } else {
        // Legacy agent: no ack correlation and no no-auth handling
        // either. Best we can do is log an operator hint; the old
        // secret remains in the local config until manually removed.
        tracing::warn!(
            node_id = %node_id,
            service_slug = %service_slug,
            "auth_method downgraded to none but the node is a legacy agent without no-auth placeholder support — it will keep injecting the old secret. Run `nyxid node credentials remove {}` on that node to clean up, then upgrade the node agent",
            service_slug
        );
    }
}

/// After an OAuth callback stores a token, find any UserService records
/// for this user + provider that are node-routed, and push the credential.
///
/// This bridges the old provider system with the new UserService model:
/// looks up UserApiKey records that have the same provider_config_id.
pub async fn push_oauth_credential_to_nodes(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    provider_config_id: &str,
) {
    // Find UserApiKeys linked to this provider
    let api_keys: Vec<UserApiKey> = match db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find(doc! {
            "user_id": user_id,
            "provider_config_id": provider_config_id,
            "status": "active",
            "credential_type": { "$ne": "node_managed" },
        })
        .await
    {
        Ok(cursor) => cursor.try_collect().await.unwrap_or_default(),
        Err(_) => return,
    };

    for api_key in &api_keys {
        push_credential_to_node_if_routed(
            db,
            encryption_keys,
            node_ws_manager,
            user_id,
            &api_key.id,
        )
        .await;
    }
}

/// Build CredentialUpdateParams from a UserService and decrypted credential.
fn build_credential_params(
    svc: &UserService,
    credential: &str,
    target_url: Option<String>,
) -> CredentialUpdateParams {
    build_credential_params_from_fields(
        &svc.slug,
        &svc.auth_method,
        &svc.auth_key_name,
        credential,
        target_url,
    )
}

/// Build `CredentialUpdateParams` from raw fields. Used by the strict
/// push path when the caller has post-update values that don't yet
/// reflect what's in the DB.
fn build_credential_params_from_fields(
    service_slug: &str,
    auth_method: &str,
    auth_key_name: &str,
    credential: &str,
    target_url: Option<String>,
) -> CredentialUpdateParams {
    match auth_method {
        "bearer" => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "header".to_string(),
            // Bearer auth is always injected under `Authorization`, matching
            // the direct-proxy behavior and mirroring how `basic` is handled
            // below. Stored `auth_key_name` may still be empty on services
            // originally created with `auth_method: "none"` or carry a
            // stale value from a previous `header`-auth configuration —
            // using it verbatim would push `Bearer …` under the wrong
            // header and break node-side auth (twenty-first-round Codex
            // P2). Users who want a custom header for a bearer-like
            // token should register the service with `auth_method:
            // "header"` and embed the `Bearer ` prefix in the credential.
            header_name: Some("Authorization".to_string()),
            header_value: Some(format!("Bearer {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        "header" => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "header".to_string(),
            header_name: Some(auth_key_name.to_string()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
        "query" => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "query_param".to_string(),
            header_name: None,
            header_value: None,
            param_name: Some(auth_key_name.to_string()),
            param_value: Some(credential.to_string()),
            target_url,
        },
        "basic" => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "header".to_string(),
            header_name: Some("Authorization".to_string()),
            header_value: Some(format!("Basic {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        "path" => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "path_prefix".to_string(),
            header_name: Some(auth_key_name.to_string()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
        "none" => CredentialUpdateParams {
            // No-auth placeholder: node keeps the slug entry (so
            // `proxy_executor` can resolve `target_url`) but stops
            // injecting any secret. Used when a server-held service
            // is downgraded to `auth_method: "none"` on the same
            // node (thirty-third-round Codex P1).
            service_slug: service_slug.to_string(),
            injection_method: "none".to_string(),
            header_name: None,
            header_value: None,
            param_name: None,
            param_value: None,
            target_url,
        },
        _ => CredentialUpdateParams {
            service_slug: service_slug.to_string(),
            injection_method: "header".to_string(),
            header_name: Some(auth_key_name.to_string()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
    }
}

/// Decrypt the active credential from a UserApiKey.
async fn decrypt_api_key_credential(
    api_key: &UserApiKey,
    encryption_keys: &EncryptionKeys,
) -> Option<String> {
    let encrypted = match api_key.credential_type.as_str() {
        "oauth2" => api_key.access_token_encrypted.as_ref(),
        _ => api_key.credential_encrypted.as_ref(),
    }?;

    let decrypted_bytes = match encryption_keys.decrypt(encrypted).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to decrypt credential for push");
            return None;
        }
    };

    String::from_utf8(decrypted_bytes).ok()
}
