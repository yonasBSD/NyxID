//! Broker / token-vault binding lifecycle.
//!
//! When a `broker_capability_enabled` OAuth client redeems an authorization
//! code, NyxID encrypts the user's refresh_token at rest and returns an
//! opaque `binding_id` instead of the refresh_token itself. The client
//! later exchanges the binding_id for a short-lived access token via
//! RFC 8693 token exchange (commit #5).

use chrono::{Duration, Utc};
use mongodb::bson::{self, Binary, doc, spec::BinarySubtype};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::crypto::aes::EncryptionKeys;
use crate::crypto::jwt::{self, JwtKeys};
use crate::errors::{AppError, AppResult};
use crate::models::authorization_code::ExternalSubjectRef;
use crate::models::oauth_broker_binding::{
    COLLECTION_NAME as OAUTH_BROKER_BINDINGS, OauthBrokerBinding, generate_binding_id,
    hash_binding_id,
};
use crate::models::refresh_token::{COLLECTION_NAME as REFRESH_TOKENS, RefreshToken};

/// Subject-token-type URN identifying a broker binding handle in RFC 8693
/// token exchange requests. NyxID-vendored under the `urn:nyxid:` namespace.
pub const BROKER_SUBJECT_TOKEN_TYPE: &str = "urn:nyxid:token-type:binding_id";

/// Issued-token-type URN for the access tokens the broker hands back --
/// the standard RFC 8693 access_token URN.
pub const ISSUED_TOKEN_TYPE_ACCESS_TOKEN: &str = "urn:ietf:params:oauth:token-type:access_token";

/// Default TTL (in seconds) for broker-issued access tokens.
///
/// Broker-bound credentials demand fast revocation propagation -- short
/// access lifetimes mean a revoked binding is honored within 5 minutes
/// without resource-server introspection.
pub const BROKER_ACCESS_TTL_SECS: i64 = 300;

pub struct BindingExchangeResult {
    pub access_token: String,
    pub expires_in: i64,
    pub granted_scope: String,
    pub issued_token_type: String,
}

/// Issue a new broker binding for the freshly-minted refresh_token.
///
/// Returns the raw `binding_id` (which is returned ONCE to the client)
/// and the binding's `_id` (SHA-256 of the raw binding_id, the durable
/// reference for cascade-revoke and audit).
// The issuance call needs the binding owner, encrypted credential source,
// scope snapshot, and optional external subject all at once.
#[allow(clippy::too_many_arguments)]
pub async fn create_binding(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    client_id: &str,
    user_id: &str,
    refresh_token: &str,
    refresh_token_jti: &str,
    scopes: &[String],
    external_subject: Option<&ExternalSubjectRef>,
) -> AppResult<(String, String)> {
    let raw_binding_id = generate_binding_id();
    let binding_hash = hash_binding_id(&raw_binding_id);

    let refresh_token_encrypted = encryption_keys
        .encrypt(refresh_token.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("broker binding encrypt failed: {e}")))?;

    let now = Utc::now();
    let binding = OauthBrokerBinding {
        id: binding_hash.clone(),
        client_id: client_id.to_string(),
        user_id: user_id.to_string(),
        refresh_token_jti: refresh_token_jti.to_string(),
        refresh_token_encrypted: Some(refresh_token_encrypted),
        scopes: scopes.to_vec(),
        external_subject: external_subject.cloned(),
        rotation_version: 0,
        revoked: false,
        last_used_at: None,
        revoked_at: None,
        revoke_reason: None,
        created_at: now,
    };

    db.collection::<OauthBrokerBinding>(OAUTH_BROKER_BINDINGS)
        .insert_one(&binding)
        .await?;

    Ok((raw_binding_id, binding_hash))
}

/// Exchange a binding_id for a fresh short-lived access_token.
///
/// Validates client ownership of the binding, detects refresh-token reuse
/// (cascade-revokes all bindings for the user/client pair on detection),
/// rotates the underlying refresh_token, re-encrypts it under the same
/// binding row, and returns a 5-minute access_token. The refresh_token
/// never leaves the server.
///
/// Concurrent callers race on `rotation_version`. The losing caller gets
/// `invalid_grant`; clients are expected to retry. Full chain-follow
/// retry is a v2 hardening.
// Broker exchange spans binding lookup, refresh-token rotation, JWT issuance,
// encryption, and optimistic binding update in one operation.
#[allow(clippy::too_many_arguments)]
pub async fn exchange_via_binding(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    jwt_keys: &JwtKeys,
    config: &AppConfig,
    client_id: &str,
    raw_binding_id: &str,
    requested_scope: Option<&str>,
) -> AppResult<BindingExchangeResult> {
    let binding_hash = hash_binding_id(raw_binding_id);
    let binding = db
        .collection::<OauthBrokerBinding>(OAUTH_BROKER_BINDINGS)
        .find_one(doc! { "_id": &binding_hash })
        .await?
        .ok_or_else(invalid_grant)?;

    if binding.client_id != client_id || binding.revoked {
        return Err(invalid_grant());
    }

    let encrypted_refresh = binding
        .refresh_token_encrypted
        .as_ref()
        .ok_or_else(invalid_grant)?;
    let refresh_token_bytes = encryption_keys
        .decrypt(encrypted_refresh)
        .await
        .map_err(|_| invalid_grant())?;
    let refresh_token_str = String::from_utf8(refresh_token_bytes).map_err(|_| invalid_grant())?;

    let refresh_token_doc = db
        .collection::<RefreshToken>(REFRESH_TOKENS)
        .find_one(doc! { "jti": &binding.refresh_token_jti })
        .await?
        .ok_or_else(invalid_grant)?;
    if refresh_token_doc.client_id != binding.client_id
        || refresh_token_doc.user_id != binding.user_id
    {
        return Err(invalid_grant());
    }

    if refresh_token_doc.revoked {
        revoke_bindings_for_user_client(db, &binding.client_id, &binding.user_id, "reuse_detected")
            .await?;
        tracing::warn!(
            client_id = %binding.client_id,
            binding_hash = %binding_hash_prefix(&binding_hash),
            "broker binding refresh-token reuse detected; cascading revoke"
        );
        return Err(invalid_grant());
    }

    let refresh_claims =
        jwt::verify_token(jwt_keys, config, &refresh_token_str).map_err(|_| invalid_grant())?;
    if refresh_claims.token_type != "refresh"
        || refresh_claims.jti != binding.refresh_token_jti
        || refresh_claims.sub != binding.user_id
    {
        return Err(invalid_grant());
    }

    let granted_scope = resolve_binding_scope(requested_scope, &binding.scopes)?;
    let user_uuid = Uuid::parse_str(&binding.user_id)
        .map_err(|e| AppError::Internal(format!("Invalid user_id in broker binding: {e}")))?;
    let rbac_data =
        crate::services::rbac_helpers::build_rbac_claim_data(db, &binding.user_id, &granted_scope)
            .await?;
    let access_token = jwt::generate_access_token(
        jwt_keys,
        config,
        &user_uuid,
        &granted_scope,
        Some(&rbac_data),
        Some(BROKER_ACCESS_TTL_SECS),
    )?;

    let (new_refresh_jwt, new_jti) = jwt::generate_refresh_token(jwt_keys, config, &user_uuid)?;
    let new_refresh_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let refresh_expires = now + Duration::seconds(config.jwt_refresh_ttl_secs);
    let new_refresh = RefreshToken {
        id: new_refresh_id.clone(),
        jti: new_jti.clone(),
        client_id: binding.client_id.clone(),
        user_id: binding.user_id.clone(),
        session_id: None,
        expires_at: refresh_expires,
        revoked: false,
        replaced_by: None,
        revoked_at: None,
        created_at: now,
    };

    db.collection::<RefreshToken>(REFRESH_TOKENS)
        .insert_one(&new_refresh)
        .await?;

    let revoke_result = db
        .collection::<RefreshToken>(REFRESH_TOKENS)
        .update_one(
            doc! { "_id": &refresh_token_doc.id, "revoked": false },
            doc! { "$set": {
                "revoked": true,
                "replaced_by": &new_refresh_id,
                "revoked_at": bson::DateTime::from_chrono(now),
            }},
        )
        .await?;
    if revoke_result.matched_count == 0 {
        return Err(invalid_grant());
    }

    let new_blob = encryption_keys
        .encrypt(new_refresh_jwt.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("broker binding encrypt failed: {e}")))?;

    let updated = db
        .collection::<OauthBrokerBinding>(OAUTH_BROKER_BINDINGS)
        .find_one_and_update(
            doc! {
                "_id": &binding_hash,
                "rotation_version": binding.rotation_version,
            },
            doc! { "$set": {
                "refresh_token_encrypted": Binary {
                    subtype: BinarySubtype::Generic,
                    bytes: new_blob,
                },
                "refresh_token_jti": &new_jti,
                "rotation_version": i64::from(binding.rotation_version) + 1,
                "last_used_at": bson::DateTime::from_chrono(now),
            }},
        )
        .await?;
    if updated.is_none() {
        // v2: chain-follow retry can recover the already-rotated replacement
        // instead of making the client retry the binding exchange.
        return Err(invalid_grant());
    }

    Ok(BindingExchangeResult {
        access_token,
        expires_in: BROKER_ACCESS_TTL_SECS,
        granted_scope,
        issued_token_type: ISSUED_TOKEN_TYPE_ACCESS_TOKEN.to_string(),
    })
}

/// Cascade-revoke all bindings owned by `(client_id, user_id)`. Used by
/// reuse detection (commit #5) and by the explicit revoke endpoint (#6).
///
/// Returns the number of bindings modified.
pub async fn revoke_bindings_for_user_client(
    db: &mongodb::Database,
    client_id: &str,
    user_id: &str,
    reason: &str,
) -> AppResult<u64> {
    let now = Utc::now();
    let result = db
        .collection::<OauthBrokerBinding>(OAUTH_BROKER_BINDINGS)
        .update_many(
            doc! {
                "client_id": client_id,
                "user_id": user_id,
                "revoked": false,
            },
            doc! { "$set": {
                "revoked": true,
                "revoked_at": bson::DateTime::from_chrono(now),
                "revoke_reason": reason,
            }},
        )
        .await?;

    Ok(result.modified_count)
}

pub fn binding_hash_prefix(binding_hash: &str) -> String {
    binding_hash.chars().take(16).collect()
}

fn resolve_binding_scope(
    requested_scope: Option<&str>,
    stored_scopes: &[String],
) -> AppResult<String> {
    let stored: std::collections::HashSet<&str> =
        stored_scopes.iter().map(String::as_str).collect();

    if let Some(requested) = requested_scope {
        let requested_scopes: Vec<&str> = requested.split_whitespace().collect();
        for scope in &requested_scopes {
            if !stored.contains(scope) {
                return Err(AppError::InvalidScope(format!(
                    "Scope '{}' is not allowed for this binding",
                    scope
                )));
            }
        }
        return Ok(requested_scopes.join(" "));
    }

    Ok(stored_scopes.join(" "))
}

fn invalid_grant() -> AppError {
    AppError::ExternalTokenInvalid("invalid_grant".to_string())
}

#[cfg(test)]
mod tests {
    // Encryption round-trip is covered by `EncryptionKeys` tests; this module
    // mostly exercises field plumbing. Real Mongo integration is verified by
    // the handler-level test in commit #5/#6 once the exchange path lands.
}
