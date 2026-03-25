use std::collections::HashMap;

use chrono::Utc;
use futures::TryStreamExt;
use mongodb::bson::doc;
use rand::Rng;
use uuid::Uuid;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::provider_config::{COLLECTION_NAME as PROVIDER_CONFIGS, ProviderConfig};
use crate::models::user_api_key::UserApiKey;
use crate::models::user_endpoint::UserEndpoint;
use crate::models::user_provider_token::{
    COLLECTION_NAME as USER_PROVIDER_TOKENS, UserProviderToken,
};
use crate::models::user_service::UserService;
use crate::services::{
    ssh_service, user_api_key_service, user_endpoint_service, user_service_service,
};

/// Generate a slug from a label: lowercase, replace non-alphanumeric with
/// hyphens, collapse runs, then append a 4-char random alphanumeric suffix.
fn generate_slug_from_label(label: &str) -> String {
    let base: String = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let base = if base.is_empty() {
        "service".to_string()
    } else if base.len() > 80 {
        base[..80].to_string()
    } else {
        base
    };

    let mut rng = rand::thread_rng();
    let suffix: String = (0..4)
        .map(|_| {
            let idx: u8 = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();

    format!("{base}-{suffix}")
}

/// Find a unique slug for a user by appending `-2`, `-3`, etc. if the base
/// slug already exists.
async fn resolve_unique_slug(
    db: &mongodb::Database,
    user_id: &str,
    base_slug: &str,
) -> AppResult<String> {
    if user_service_service::find_by_slug(db, user_id, base_slug)
        .await?
        .is_none()
    {
        return Ok(base_slug.to_string());
    }
    for n in 2..=100 {
        let candidate = format!("{base_slug}-{n}");
        if user_service_service::find_by_slug(db, user_id, &candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(AppError::Conflict(
        "Too many services with the same slug".to_string(),
    ))
}

/// SSH-specific parameters for custom SSH service creation.
pub struct SshCreateParams<'a> {
    pub host: &'a str,
    pub port: u16,
    pub certificate_auth: bool,
    pub principals: Vec<String>,
    pub certificate_ttl_minutes: u32,
}

/// Result of creating a key (all 3 records).
pub struct CreateKeyResult {
    pub endpoint: UserEndpoint,
    pub api_key: UserApiKey,
    pub service: UserService,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_ca_public_key: Option<String>,
    pub ssh_allowed_principals: Option<Vec<String>>,
    pub ssh_certificate_ttl_minutes: Option<u32>,
}

/// Combined view for GET /keys and GET /keys/:id.
pub struct KeyView {
    pub id: String,
    pub label: String,
    pub slug: String,
    pub endpoint_url: String,
    pub endpoint_id: String,
    pub api_key_id: String,
    pub credential_type: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub status: String,
    pub catalog_service_id: Option<String>,
    pub catalog_service_name: Option<String>,
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub service_type: String,
    pub is_active: bool,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    // SSH fields
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_ca_public_key: Option<String>,
    pub ssh_allowed_principals: Option<Vec<String>>,
    pub ssh_certificate_ttl_minutes: Option<u32>,
}

fn normalized_provider_credential_type(provider_type: &str) -> &'static str {
    match provider_type {
        "oauth2" | "device_code" => "oauth2",
        _ => "api_key",
    }
}

fn direct_credential_type_from_auth_method(auth_method: &str) -> Option<&'static str> {
    match auth_method {
        "none" => None,
        "bearer" => Some("bearer"),
        "basic" => Some("basic"),
        _ => Some("api_key"),
    }
}

fn direct_credential_type_for_service(
    api_key: &UserApiKey,
    service: &UserService,
    provider: Option<&ProviderConfig>,
) -> Option<&'static str> {
    if service.service_type == "ssh" || api_key.credential_type == "ssh_certificate" {
        return None;
    }

    if let Some(provider) = provider {
        return Some(normalized_provider_credential_type(&provider.provider_type));
    }

    match api_key.credential_type.as_str() {
        "oauth2" => Some("oauth2"),
        "bearer" => Some("bearer"),
        "basic" => Some("basic"),
        "node_managed" => direct_credential_type_from_auth_method(&service.auth_method),
        _ => Some("api_key"),
    }
}

async fn find_existing_provider_token(
    db: &mongodb::Database,
    user_id: &str,
    provider_config_id: &str,
) -> AppResult<Option<UserProviderToken>> {
    db.collection::<UserProviderToken>(USER_PROVIDER_TOKENS)
        .find_one(doc! {
            "user_id": user_id,
            "provider_config_id": provider_config_id,
            "status": { "$in": ["active", "expired", "refresh_failed"] },
        })
        .await
        .map_err(Into::into)
}

/// POST /api/v1/keys -- auto-provision endpoint + api_key + service from catalog or custom.
#[allow(clippy::too_many_arguments)]
pub async fn create_key(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    service_slug: Option<&str>,
    endpoint_url: Option<&str>,
    credential: &str,
    label: &str,
    slug_override: Option<&str>,
    auth_method: Option<&str>,
    auth_key_name: Option<&str>,
    node_id: Option<&str>,
    ssh_params: Option<SshCreateParams<'_>>,
) -> AppResult<CreateKeyResult> {
    if let Some(slug) = service_slug {
        // -- Catalog path --
        let svc = db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "slug": slug, "is_active": true })
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Catalog service '{slug}' not found")))?;

        let is_ssh = svc.service_type == "ssh";
        let provider = if let Some(ref pid) = svc.provider_config_id {
            db.collection::<ProviderConfig>(PROVIDER_CONFIGS)
                .find_one(doc! { "_id": pid })
                .await?
        } else {
            None
        };
        let provider_type = provider.as_ref().map(|p| p.provider_type.as_str());
        let existing_provider_token =
            if let Some(provider_config_id) = svc.provider_config_id.as_deref() {
                find_existing_provider_token(db, user_id, provider_config_id).await?
            } else {
                None
            };

        // SSH services must be node-routed
        if is_ssh && node_id.is_none() {
            return Err(AppError::BadRequest(
                "SSH services must be routed through a node agent".to_string(),
            ));
        }

        // Determine endpoint URL
        let ep_url = if let Some(url) = endpoint_url {
            url.to_string()
        } else if is_ssh {
            // SSH: derive from SshServiceConfig
            svc.ssh_config
                .as_ref()
                .map(|c| format!("ssh://{}:{}", c.host, c.port))
                .unwrap_or_default()
        } else if node_id.is_some() {
            // Node-routed: endpoint URL stored on node, not on NyxID
            String::new()
        } else if provider.as_ref().is_some_and(|p| p.requires_gateway_url) {
            return Err(AppError::BadRequest(
                "This service requires an endpoint URL".to_string(),
            ));
        } else {
            svc.base_url.clone()
        };

        // Determine credential type
        let node_managed_credential = node_id.is_some() && credential.is_empty();

        if node_id.is_some() && svc.provider_config_id.is_some() && !credential.is_empty() {
            return Err(AppError::BadRequest(
                "Node-routed provider services must be authorized on the node agent. Do not send the credential to NyxID."
                    .to_string(),
            ));
        }

        let credential_type = if is_ssh {
            "ssh_certificate".to_string()
        } else if node_managed_credential {
            "node_managed".to_string()
        } else if let Some(ref token) = existing_provider_token {
            match token.token_type.as_str() {
                "oauth2" => "oauth2".to_string(),
                _ => "api_key".to_string(),
            }
        } else if matches!(provider_type, Some("oauth2" | "device_code")) {
            "oauth2".to_string()
        } else if let Some(kind) = provider_type {
            normalized_provider_credential_type(kind).to_string()
        } else {
            svc.auth_type.as_deref().unwrap_or("api_key").to_string()
        };

        // Validate: credential required for direct routing (non-SSH, non-node-managed)
        let can_defer_direct_credential = existing_provider_token.is_some()
            || matches!(provider_type, Some("oauth2" | "device_code"));
        if credential.is_empty() && node_id.is_none() && !is_ssh && !can_defer_direct_credential {
            return Err(AppError::BadRequest(
                "Credential is required for direct routing (or select a node)".to_string(),
            ));
        }

        // Determine provider_config_id for the api key
        let provider_config_id = svc.provider_config_id.as_deref();

        // Create all three records
        let endpoint =
            user_endpoint_service::create_endpoint(db, user_id, &svc.name, &ep_url, Some(&svc.id))
                .await?;

        let api_key = if !node_managed_credential {
            if let Some(ref provider_token) = existing_provider_token {
                user_api_key_service::create_api_key_from_provider_token(
                    db,
                    user_id,
                    label,
                    provider_config_id.expect("provider token implies provider config"),
                    provider_token,
                )
                .await?
            } else {
                let pending_oauth = matches!(provider_type, Some("oauth2" | "device_code"))
                    && credential.is_empty()
                    && node_id.is_none();
                user_api_key_service::create_api_key(
                    db,
                    encryption_keys,
                    user_id,
                    user_api_key_service::CreateApiKeyParams {
                        label,
                        credential_type: &credential_type,
                        credential,
                        access_token: (credential_type == "oauth2" && !credential.is_empty())
                            .then_some(credential),
                        refresh_token: None,
                        token_scopes: None,
                        expires_at: None,
                        provider_config_id,
                        status: if pending_oauth {
                            "pending_auth"
                        } else {
                            "active"
                        },
                        source: Some("user_created"),
                        source_id: None,
                    },
                )
                .await?
            }
        } else {
            user_api_key_service::create_api_key(
                db,
                encryption_keys,
                user_id,
                user_api_key_service::CreateApiKeyParams {
                    label,
                    credential_type: &credential_type,
                    credential,
                    access_token: None,
                    refresh_token: None,
                    token_scopes: None,
                    expires_at: None,
                    provider_config_id,
                    status: "active",
                    source: Some("user_created"),
                    source_id: None,
                },
            )
            .await?
        };

        // Auto-suffix slug if one already exists for this user (e.g. llm-openai -> llm-openai-2)
        let unique_slug = resolve_unique_slug(db, user_id, &svc.slug).await?;

        let service = user_service_service::create_user_service(
            db,
            user_id,
            &unique_slug,
            &endpoint.id,
            &api_key.id,
            &svc.auth_method,
            &svc.auth_key_name,
            Some(&svc.id),
            node_id,
            0,
            &svc.service_type,
        )
        .await?;

        let (
            ssh_host,
            ssh_port,
            ssh_ca_public_key,
            ssh_allowed_principals,
            ssh_certificate_ttl_minutes,
        ) = if is_ssh {
            svc.ssh_config
                .as_ref()
                .map(|ssh| {
                    (
                        Some(ssh.host.clone()),
                        Some(ssh.port),
                        ssh.ca_public_key.clone(),
                        Some(ssh.allowed_principals.clone()),
                        Some(ssh.certificate_ttl_minutes),
                    )
                })
                .unwrap_or_default()
        } else {
            Default::default()
        };

        Ok(CreateKeyResult {
            endpoint,
            api_key,
            service,
            ssh_host,
            ssh_port,
            ssh_ca_public_key,
            ssh_allowed_principals,
            ssh_certificate_ttl_minutes,
        })
    } else if let Some(ssh) = ssh_params {
        // -- Custom SSH path --
        if node_id.is_none() {
            return Err(AppError::BadRequest(
                "SSH services must be routed through a node agent".to_string(),
            ));
        }

        let slug = match slug_override {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => generate_slug_from_label(label),
        };

        // Build SSH config (generates CA keypair)
        let ds_id = Uuid::new_v4().to_string();
        let built_ssh_config = ssh_service::build_ssh_config(
            encryption_keys,
            &ds_id,
            None,
            ssh_service::SshConfigInput {
                host: ssh.host,
                port: ssh.port,
                certificate_auth_enabled: ssh.certificate_auth,
                certificate_ttl_minutes: ssh.certificate_ttl_minutes,
                allowed_principals: &ssh.principals,
            },
        )
        .await?;

        let now = Utc::now();
        let base_url = ssh_service::target_base_url(&built_ssh_config.host, built_ssh_config.port);

        // Create DownstreamService with SSH config
        let ds = DownstreamService {
            id: ds_id.clone(),
            name: label.to_string(),
            slug: slug.to_string(),
            description: None,
            base_url: base_url.clone(),
            service_type: "ssh".to_string(),
            visibility: "private".to_string(),
            auth_method: "none".to_string(),
            auth_type: Some("ssh".to_string()),
            auth_key_name: String::new(),
            credential_encrypted: encryption_keys.encrypt(b"").await?,
            openapi_spec_url: None,
            asyncapi_spec_url: None,
            streaming_supported: false,
            ssh_config: Some(built_ssh_config.clone()),
            oauth_client_id: None,
            service_category: "internal".to_string(),
            requires_user_credential: false,
            is_active: true,
            created_by: user_id.to_string(),
            identity_propagation_mode: "none".to_string(),
            identity_include_user_id: false,
            identity_include_email: false,
            identity_include_name: false,
            identity_jwt_audience: None,
            inject_delegation_token: false,
            delegation_token_scope: "llm:proxy".to_string(),
            provider_config_id: None,
            created_at: now,
            updated_at: now,
        };

        db.collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .insert_one(&ds)
            .await?;

        let endpoint =
            user_endpoint_service::create_endpoint(db, user_id, label, &base_url, Some(&ds_id))
                .await?;

        let api_key = user_api_key_service::create_api_key(
            db,
            encryption_keys,
            user_id,
            user_api_key_service::CreateApiKeyParams {
                label,
                credential_type: "ssh_certificate",
                credential: "",
                access_token: None,
                refresh_token: None,
                token_scopes: None,
                expires_at: None,
                provider_config_id: None,
                status: "active",
                source: Some("user_created"),
                source_id: None,
            },
        )
        .await?;

        let unique_slug = resolve_unique_slug(db, user_id, &slug).await?;
        let service = user_service_service::create_user_service(
            db,
            user_id,
            &unique_slug,
            &endpoint.id,
            &api_key.id,
            "none",
            "",
            Some(&ds_id),
            node_id,
            0,
            "ssh",
        )
        .await?;

        Ok(CreateKeyResult {
            endpoint,
            api_key,
            service,
            ssh_host: Some(built_ssh_config.host),
            ssh_port: Some(built_ssh_config.port),
            ssh_ca_public_key: built_ssh_config.ca_public_key,
            ssh_allowed_principals: Some(built_ssh_config.allowed_principals),
            ssh_certificate_ttl_minutes: Some(built_ssh_config.certificate_ttl_minutes),
        })
    } else {
        // -- Custom HTTP path --
        let ep_url = endpoint_url.unwrap_or("");
        if ep_url.is_empty() && node_id.is_none() {
            return Err(AppError::BadRequest(
                "endpoint_url is required for custom endpoints without node routing".to_string(),
            ));
        }

        // Validate: credential required for direct routing
        if credential.is_empty() && node_id.is_none() {
            return Err(AppError::BadRequest(
                "Credential is required for direct routing (or select a node)".to_string(),
            ));
        }

        let slug = match slug_override {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => generate_slug_from_label(label),
        };
        let am = auth_method.unwrap_or("bearer");
        let akn = auth_key_name.unwrap_or("Authorization");

        let credential_type = if credential.is_empty() && node_id.is_some() {
            "node_managed"
        } else {
            "api_key"
        };

        let endpoint =
            user_endpoint_service::create_endpoint(db, user_id, label, ep_url, None).await?;

        let api_key = user_api_key_service::create_api_key(
            db,
            encryption_keys,
            user_id,
            user_api_key_service::CreateApiKeyParams {
                label,
                credential_type,
                credential,
                access_token: None,
                refresh_token: None,
                token_scopes: None,
                expires_at: None,
                provider_config_id: None,
                status: "active",
                source: Some("user_created"),
                source_id: None,
            },
        )
        .await?;

        let unique_slug = resolve_unique_slug(db, user_id, &slug).await?;
        let service = user_service_service::create_user_service(
            db,
            user_id,
            &unique_slug,
            &endpoint.id,
            &api_key.id,
            am,
            akn,
            None,
            node_id,
            0,
            "http",
        )
        .await?;

        Ok(CreateKeyResult {
            endpoint,
            api_key,
            service,
            ssh_host: None,
            ssh_port: None,
            ssh_ca_public_key: None,
            ssh_allowed_principals: None,
            ssh_certificate_ttl_minutes: None,
        })
    }
}

/// GET /api/v1/keys -- list all keys as combined views.
pub async fn list_keys(db: &mongodb::Database, user_id: &str) -> AppResult<Vec<KeyView>> {
    let services = user_service_service::list_user_services(db, user_id).await?;
    if services.is_empty() {
        return Ok(vec![]);
    }

    // Batch-load endpoints
    let endpoint_ids: Vec<&str> = services.iter().map(|s| s.endpoint_id.as_str()).collect();
    let endpoints: Vec<UserEndpoint> = db
        .collection::<UserEndpoint>(crate::models::user_endpoint::COLLECTION_NAME)
        .find(doc! { "_id": { "$in": &endpoint_ids } })
        .await?
        .try_collect()
        .await?;
    let ep_map: HashMap<&str, &UserEndpoint> =
        endpoints.iter().map(|e| (e.id.as_str(), e)).collect();

    // Batch-load api keys
    let api_key_ids: Vec<&str> = services.iter().map(|s| s.api_key_id.as_str()).collect();
    let api_keys: Vec<UserApiKey> = db
        .collection::<UserApiKey>(crate::models::user_api_key::COLLECTION_NAME)
        .find(doc! { "_id": { "$in": &api_key_ids } })
        .await?
        .try_collect()
        .await?;
    let ak_map: HashMap<&str, &UserApiKey> = api_keys.iter().map(|k| (k.id.as_str(), k)).collect();

    // Batch-load catalog services (for names + SSH config)
    let catalog_ids: Vec<&str> = services
        .iter()
        .filter_map(|s| s.catalog_service_id.as_deref())
        .collect();
    let catalog_services: Vec<DownstreamService> = if catalog_ids.is_empty() {
        vec![]
    } else {
        db.collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find(doc! { "_id": { "$in": &catalog_ids } })
            .await?
            .try_collect()
            .await?
    };
    let cat_map: HashMap<&str, &DownstreamService> = catalog_services
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    let views = services
        .iter()
        .filter_map(|svc| {
            let ep = ep_map.get(svc.endpoint_id.as_str())?;
            let ak = ak_map.get(svc.api_key_id.as_str())?;
            Some(build_key_view(svc, ep, ak, &cat_map))
        })
        .collect();

    Ok(views)
}

/// GET /api/v1/keys/:id -- get single combined view.
pub async fn get_key(
    db: &mongodb::Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<KeyView> {
    let svc = user_service_service::get_user_service(db, user_id, service_id).await?;
    let ep = user_endpoint_service::get_endpoint(db, user_id, &svc.endpoint_id).await?;
    let ak = user_api_key_service::get_api_key(db, user_id, &svc.api_key_id).await?;

    // Load catalog service if applicable (for name + SSH config)
    let catalog_ds = if let Some(ref csid) = svc.catalog_service_id {
        db.collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "_id": csid })
            .await?
    } else {
        None
    };

    let cat_map: HashMap<&str, &DownstreamService> = catalog_ds
        .as_ref()
        .and_then(|ds| svc.catalog_service_id.as_deref().map(|id| (id, ds)))
        .into_iter()
        .collect();

    Ok(build_key_view(&svc, &ep, &ak, &cat_map))
}

pub async fn reconcile_provider_key_for_service_routing(
    db: &mongodb::Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<()> {
    let service = user_service_service::get_user_service(db, user_id, service_id).await?;
    let api_key = user_api_key_service::get_api_key(db, user_id, &service.api_key_id).await?;

    if service.node_id.is_some() {
        user_api_key_service::activate_node_managed_api_key(db, user_id, &api_key.id).await?;
        return Ok(());
    }

    if user_api_key_service::has_server_credential(&api_key) || service.auth_method == "none" {
        return Ok(());
    }

    let provider = if let Some(provider_config_id) = api_key.provider_config_id.as_deref() {
        db.collection::<ProviderConfig>(PROVIDER_CONFIGS)
            .find_one(doc! { "_id": provider_config_id })
            .await?
    } else {
        None
    };
    let Some(direct_credential_type) =
        direct_credential_type_for_service(&api_key, &service, provider.as_ref())
    else {
        return Ok(());
    };

    if let Some(provider_config_id) = api_key.provider_config_id.as_deref()
        && find_existing_provider_token(db, user_id, provider_config_id)
            .await?
            .is_some()
    {
        user_api_key_service::mark_provider_connection_pending(
            db,
            user_id,
            &api_key.id,
            direct_credential_type,
        )
        .await?;
        user_api_key_service::sync_provider_token_to_api_keys(db, user_id, provider_config_id)
            .await?;
        return Ok(());
    }

    user_api_key_service::mark_provider_connection_pending(
        db,
        user_id,
        &api_key.id,
        direct_credential_type,
    )
    .await
}

/// DELETE /api/v1/keys/:id -- revoke key.
pub async fn revoke_key(db: &mongodb::Database, user_id: &str, service_id: &str) -> AppResult<()> {
    let svc = user_service_service::get_user_service(db, user_id, service_id).await?;
    user_service_service::deactivate_user_service(db, user_id, service_id).await?;
    user_api_key_service::revoke_api_key(db, user_id, &svc.api_key_id).await?;
    Ok(())
}

fn build_key_view(
    svc: &UserService,
    ep: &UserEndpoint,
    ak: &UserApiKey,
    cat_map: &HashMap<&str, &DownstreamService>,
) -> KeyView {
    let catalog_ds = svc
        .catalog_service_id
        .as_deref()
        .and_then(|id| cat_map.get(id).copied());

    // SSH fields from catalog service
    let (
        ssh_host,
        ssh_port,
        ssh_ca_public_key,
        ssh_allowed_principals,
        ssh_certificate_ttl_minutes,
    ) = if svc.service_type == "ssh" {
        if let Some(ds) = catalog_ds {
            if let Some(ref ssh) = ds.ssh_config {
                (
                    Some(ssh.host.clone()),
                    Some(ssh.port),
                    ssh.ca_public_key.clone(),
                    Some(ssh.allowed_principals.clone()),
                    Some(ssh.certificate_ttl_minutes),
                )
            } else {
                (None, None, None, None, None)
            }
        } else {
            (None, None, None, None, None)
        }
    } else {
        (None, None, None, None, None)
    };

    KeyView {
        id: svc.id.clone(),
        label: ak.label.clone(),
        slug: svc.slug.clone(),
        endpoint_url: ep.url.clone(),
        endpoint_id: ep.id.clone(),
        api_key_id: ak.id.clone(),
        credential_type: ak.credential_type.clone(),
        auth_method: svc.auth_method.clone(),
        auth_key_name: svc.auth_key_name.clone(),
        status: ak.status.clone(),
        catalog_service_id: svc.catalog_service_id.clone(),
        catalog_service_name: catalog_ds.map(|ds| ds.name.clone()),
        node_id: svc.node_id.clone(),
        node_priority: svc.node_priority,
        service_type: svc.service_type.clone(),
        is_active: svc.is_active,
        expires_at: ak.expires_at.map(|dt| dt.to_rfc3339()),
        last_used_at: ak.last_used_at.map(|dt| dt.to_rfc3339()),
        error_message: ak.error_message.clone(),
        created_at: svc.created_at.to_rfc3339(),
        ssh_host,
        ssh_port,
        ssh_ca_public_key,
        ssh_allowed_principals,
        ssh_certificate_ttl_minutes,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{direct_credential_type_for_service, direct_credential_type_from_auth_method};
    use crate::models::user_api_key::UserApiKey;
    use crate::models::user_service::UserService;

    fn sample_api_key(credential_type: &str) -> UserApiKey {
        UserApiKey {
            id: "key-1".to_string(),
            user_id: "user-1".to_string(),
            label: "Test".to_string(),
            credential_type: credential_type.to_string(),
            credential_encrypted: None,
            access_token_encrypted: None,
            refresh_token_encrypted: None,
            token_scopes: None,
            expires_at: None,
            provider_config_id: None,
            user_oauth_client_id_encrypted: None,
            user_oauth_client_secret_encrypted: None,
            status: "active".to_string(),
            last_used_at: None,
            error_message: None,
            source: None,
            source_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_service(auth_method: &str) -> UserService {
        UserService {
            id: "svc-1".to_string(),
            user_id: "user-1".to_string(),
            slug: "test".to_string(),
            endpoint_id: "ep-1".to_string(),
            api_key_id: "key-1".to_string(),
            auth_method: auth_method.to_string(),
            auth_key_name: "Authorization".to_string(),
            catalog_service_id: None,
            node_id: None,
            node_priority: 0,
            service_type: "http".to_string(),
            is_active: true,
            source: None,
            source_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn infers_direct_credential_type_from_auth_method() {
        assert_eq!(
            direct_credential_type_from_auth_method("bearer"),
            Some("bearer")
        );
        assert_eq!(
            direct_credential_type_from_auth_method("basic"),
            Some("basic")
        );
        assert_eq!(
            direct_credential_type_from_auth_method("header"),
            Some("api_key")
        );
        assert_eq!(
            direct_credential_type_from_auth_method("query"),
            Some("api_key")
        );
        assert_eq!(direct_credential_type_from_auth_method("none"), None);
    }

    #[test]
    fn restores_custom_node_managed_service_to_auth_specific_type() {
        let key = sample_api_key("node_managed");
        let service = sample_service("bearer");
        assert_eq!(
            direct_credential_type_for_service(&key, &service, None),
            Some("bearer")
        );
    }
}
