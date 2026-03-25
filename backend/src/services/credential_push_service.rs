use std::sync::Arc;

use futures::TryStreamExt;
use mongodb::bson::doc;

use crate::crypto::aes::EncryptionKeys;
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::services::node_ws_manager::{CredentialUpdateParams, NodeWsManager};

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
    // Find UserServices that reference this api_key and have a node_id
    let services: Vec<UserService> = match db
        .collection::<UserService>(USER_SERVICES)
        .find(doc! {
            "user_id": user_id,
            "api_key_id": api_key_id,
            "node_id": { "$ne": null },
            "is_active": true,
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
    match svc.auth_method.as_str() {
        "bearer" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
            header_value: Some(format!("Bearer {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        "header" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
        "query" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "query_param".to_string(),
            header_name: None,
            header_value: None,
            param_name: Some(svc.auth_key_name.clone()),
            param_value: Some(credential.to_string()),
            target_url,
        },
        "basic" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some("Authorization".to_string()),
            header_value: Some(format!("Basic {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        _ => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
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
