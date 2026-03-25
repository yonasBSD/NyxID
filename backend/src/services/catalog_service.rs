use std::collections::HashMap;

use futures::TryStreamExt;
use mongodb::bson::doc;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::provider_config::{COLLECTION_NAME as PROVIDER_CONFIGS, ProviderConfig};
use crate::models::service_provider_requirement::{
    COLLECTION_NAME as SERVICE_PROVIDER_REQUIREMENTS, ServiceProviderRequirement,
};

/// A catalog entry combining DownstreamService + ProviderConfig info.
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub base_url: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub provider_config_id: Option<String>,
    pub provider_type: Option<String>,
    pub requires_gateway_url: bool,
    pub api_key_instructions: Option<String>,
    pub api_key_url: Option<String>,
    pub icon_url: Option<String>,
    pub documentation_url: Option<String>,
    pub credential_mode: Option<String>,
    // SSH fields
    pub service_type: String,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_ca_public_key: Option<String>,
    pub ssh_allowed_principals: Option<Vec<String>>,
    pub ssh_certificate_ttl_minutes: Option<u32>,
    // OAuth config fields (for node-native OAuth)
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub device_code_url: Option<String>,
    pub device_verification_url: Option<String>,
    pub device_token_url: Option<String>,
    pub default_scopes: Option<Vec<String>>,
    pub supports_pkce: bool,
    pub device_code_format: Option<String>,
    pub token_endpoint_auth_method: Option<String>,
    pub extra_auth_params: Option<HashMap<String, String>>,
    pub oauth_client_id: Option<String>,
    pub client_id_param_name: Option<String>,
}

fn build_catalog_entry(
    svc: DownstreamService,
    provider: Option<&ProviderConfig>,
    spr: Option<&ServiceProviderRequirement>,
    oauth_client_id: Option<String>,
) -> CatalogEntry {
    CatalogEntry {
        service_type: svc.service_type.clone(),
        ssh_host: svc.ssh_config.as_ref().map(|c| c.host.clone()),
        ssh_port: svc.ssh_config.as_ref().map(|c| c.port),
        ssh_ca_public_key: svc
            .ssh_config
            .as_ref()
            .and_then(|c| c.ca_public_key.clone()),
        ssh_allowed_principals: svc
            .ssh_config
            .as_ref()
            .map(|c| c.allowed_principals.clone()),
        ssh_certificate_ttl_minutes: svc.ssh_config.as_ref().map(|c| c.certificate_ttl_minutes),
        slug: svc.slug,
        name: svc.name,
        description: svc.description,
        base_url: svc.base_url,
        // For internal services (auth_method="none"), resolve actual injection
        // from ServiceProviderRequirement (e.g., bearer/Authorization, header/x-api-key, query/key)
        auth_method: if svc.auth_method == "none" {
            spr.map(|r| r.injection_method.clone())
                .unwrap_or_else(|| svc.auth_method)
        } else {
            svc.auth_method
        },
        auth_key_name: if svc.auth_key_name.is_empty() {
            spr.and_then(|r| r.injection_key.clone())
                .unwrap_or_else(|| "Authorization".to_string())
        } else {
            svc.auth_key_name
        },
        provider_config_id: provider.map(|p| p.id.clone()),
        provider_type: provider.map(|p| p.provider_type.clone()),
        requires_gateway_url: provider.is_some_and(|p| p.requires_gateway_url),
        api_key_instructions: provider.and_then(|p| p.api_key_instructions.clone()),
        api_key_url: provider.and_then(|p| p.api_key_url.clone()),
        icon_url: provider.and_then(|p| p.icon_url.clone()),
        documentation_url: provider.and_then(|p| p.documentation_url.clone()),
        credential_mode: provider.map(|p| p.credential_mode.clone()),
        // OAuth config
        authorization_url: provider.and_then(|p| p.authorization_url.clone()),
        token_url: provider.and_then(|p| p.token_url.clone()),
        device_code_url: provider.and_then(|p| p.device_code_url.clone()),
        device_verification_url: provider.and_then(|p| p.device_verification_url.clone()),
        device_token_url: provider.and_then(|p| p.device_token_url.clone()),
        default_scopes: provider.and_then(|p| p.default_scopes.clone()),
        supports_pkce: provider.is_some_and(|p| p.supports_pkce),
        device_code_format: provider.map(|p| p.device_code_format.clone()),
        token_endpoint_auth_method: provider.map(|p| p.token_endpoint_auth_method.clone()),
        extra_auth_params: provider.and_then(|p| p.extra_auth_params.clone()),
        oauth_client_id,
        client_id_param_name: provider.and_then(|p| p.client_id_param_name.clone()),
    }
}

async fn decrypt_provider_client_id(
    provider: &ProviderConfig,
    encryption_keys: &EncryptionKeys,
) -> AppResult<Option<String>> {
    let Some(encrypted) = provider.client_id_encrypted.as_ref() else {
        return Ok(None);
    };

    let decrypted = encryption_keys.decrypt(encrypted).await?;
    let client_id = String::from_utf8(decrypted)
        .map_err(|_| AppError::Internal("Failed to decode provider client_id".to_string()))?;
    if client_id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(client_id))
    }
}

/// List catalog entries available for user key creation.
/// Filters to connection-category + provider-linked services.
pub async fn list_catalog(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
) -> AppResult<Vec<CatalogEntry>> {
    let services: Vec<DownstreamService> = db
        .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find(doc! {
            "service_type": { "$in": ["http", "ssh"] },
            "is_active": true,
            "$or": [
                { "requires_user_credential": true },
                { "provider_config_id": { "$ne": null } },
                { "service_type": "ssh" },
            ],
            "service_category": { "$in": ["connection", "internal"] },
        })
        .sort(doc! { "name": 1 })
        .await?
        .try_collect()
        .await?;

    // Batch-load all referenced provider configs
    let provider_ids: Vec<&str> = services
        .iter()
        .filter_map(|s| s.provider_config_id.as_deref())
        .collect();

    let providers: Vec<ProviderConfig> = if provider_ids.is_empty() {
        vec![]
    } else {
        db.collection::<ProviderConfig>(PROVIDER_CONFIGS)
            .find(doc! { "_id": { "$in": &provider_ids } })
            .await?
            .try_collect()
            .await?
    };

    // Batch-load service provider requirements to get actual auth injection config
    let svc_ids: Vec<&str> = services.iter().map(|s| s.id.as_str()).collect();
    let sprs: Vec<ServiceProviderRequirement> = if svc_ids.is_empty() {
        vec![]
    } else {
        db.collection::<ServiceProviderRequirement>(SERVICE_PROVIDER_REQUIREMENTS)
            .find(doc! { "service_id": { "$in": &svc_ids } })
            .await?
            .try_collect()
            .await?
    };

    let mut resolved_entries = Vec::with_capacity(services.len());
    for svc in services {
        let provider = svc
            .provider_config_id
            .as_ref()
            .and_then(|pid| providers.iter().find(|p| &p.id == pid));

        let spr = sprs.iter().find(|r| r.service_id == svc.id);

        let oauth_client_id = match provider {
            Some(provider) if provider.credential_mode != "user" => {
                decrypt_provider_client_id(provider, encryption_keys).await?
            }
            _ => None,
        };

        resolved_entries.push(build_catalog_entry(svc, provider, spr, oauth_client_id));
    }

    Ok(resolved_entries)
}

/// Get single catalog entry by slug.
pub async fn get_catalog_entry(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    slug: &str,
) -> AppResult<CatalogEntry> {
    let svc = db
        .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
        .find_one(doc! { "slug": slug, "is_active": true })
        .await?
        .ok_or_else(|| AppError::NotFound("Catalog entry not found".to_string()))?;

    let provider = if let Some(ref pid) = svc.provider_config_id {
        db.collection::<ProviderConfig>(PROVIDER_CONFIGS)
            .find_one(doc! { "_id": pid })
            .await?
    } else {
        None
    };

    let spr = db
        .collection::<ServiceProviderRequirement>(SERVICE_PROVIDER_REQUIREMENTS)
        .find_one(doc! { "service_id": &svc.id })
        .await?;

    let oauth_client_id = match provider.as_ref() {
        Some(provider) if provider.credential_mode != "user" => {
            decrypt_provider_client_id(provider, encryption_keys).await?
        }
        _ => None,
    };

    Ok(build_catalog_entry(
        svc,
        provider.as_ref(),
        spr.as_ref(),
        oauth_client_id,
    ))
}
