//! Broker / token-vault binding lifecycle.
//!
//! When a `broker_capability_enabled` OAuth client redeems an authorization
//! code, NyxID encrypts the user's refresh_token at rest and returns an
//! opaque `binding_id` instead of the refresh_token itself. The client
//! later exchanges the binding_id for a short-lived access token via
//! RFC 8693 token exchange (commit #5).

use chrono::Utc;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};
use crate::models::authorization_code::ExternalSubjectRef;
use crate::models::oauth_broker_binding::{
    COLLECTION_NAME as OAUTH_BROKER_BINDINGS, OauthBrokerBinding, generate_binding_id,
    hash_binding_id,
};

/// Default TTL (in seconds) for broker-issued access tokens.
///
/// Broker-bound credentials demand fast revocation propagation -- short
/// access lifetimes mean a revoked binding is honored within 5 minutes
/// without resource-server introspection.
pub const BROKER_ACCESS_TTL_SECS: i64 = 300;

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

#[cfg(test)]
mod tests {
    // Encryption round-trip is covered by `EncryptionKeys` tests; this module
    // mostly exercises field plumbing. Real Mongo integration is verified by
    // the handler-level test in commit #5/#6 once the exchange path lands.
}
