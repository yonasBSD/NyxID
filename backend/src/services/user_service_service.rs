use chrono::Utc;
use futures::TryStreamExt;
use mongodb::bson::{self, doc};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::models::user_api_key::COLLECTION_NAME as USER_API_KEYS;
use crate::models::user_endpoint::COLLECTION_NAME as USER_ENDPOINTS;
use crate::models::user_service::{COLLECTION_NAME, UserService};
use crate::services::node_service;

/// Valid auth methods for user services.
const VALID_AUTH_METHODS: &[&str] = &["bearer", "header", "query", "basic", "none"];

/// Valid identity propagation modes.
const VALID_IDENTITY_MODES: &[&str] = &["none", "headers", "jwt", "both"];
const VALID_DELEGATION_SCOPES: &[&str] = &["llm:proxy", "proxy:*", "llm:status"];

/// Identity propagation and delegation token configuration.
#[derive(Clone, Debug)]
pub struct IdentityConfig {
    pub identity_propagation_mode: String,
    pub identity_include_user_id: bool,
    pub identity_include_email: bool,
    pub identity_include_name: bool,
    pub identity_jwt_audience: Option<String>,
    pub forward_access_token: bool,
    pub inject_delegation_token: bool,
    pub delegation_token_scope: String,
}

impl IdentityConfig {
    pub fn none() -> Self {
        Self {
            identity_propagation_mode: "none".to_string(),
            identity_include_user_id: false,
            identity_include_email: false,
            identity_include_name: false,
            identity_jwt_audience: None,
            forward_access_token: false,
            inject_delegation_token: false,
            delegation_token_scope: "llm:proxy".to_string(),
        }
    }
}

fn validate_identity_config(config: &IdentityConfig) -> AppResult<()> {
    if !VALID_IDENTITY_MODES.contains(&config.identity_propagation_mode.as_str()) {
        return Err(AppError::ValidationError(format!(
            "Invalid identity_propagation_mode '{}'. Valid: {}",
            config.identity_propagation_mode,
            VALID_IDENTITY_MODES.join(", ")
        )));
    }

    if let Some(audience) = config.identity_jwt_audience.as_deref()
        && audience.len() > 2048
    {
        return Err(AppError::ValidationError(
            "identity_jwt_audience must not exceed 2048 characters".to_string(),
        ));
    }

    for scope in config.delegation_token_scope.split_whitespace() {
        if !VALID_DELEGATION_SCOPES.contains(&scope) {
            return Err(AppError::ValidationError(format!(
                "Invalid delegation_token_scope '{}'. Must be one of: {}",
                scope,
                VALID_DELEGATION_SCOPES.join(", ")
            )));
        }
    }

    Ok(())
}

fn normalize_identity_config(config: &IdentityConfig) -> AppResult<IdentityConfig> {
    validate_identity_config(config)?;

    let normalized_scope = {
        let scopes: Vec<&str> = config.delegation_token_scope.split_whitespace().collect();
        if scopes.is_empty() {
            "llm:proxy".to_string()
        } else {
            scopes.join(" ")
        }
    };

    Ok(IdentityConfig {
        identity_propagation_mode: config.identity_propagation_mode.clone(),
        identity_include_user_id: config.identity_include_user_id,
        identity_include_email: config.identity_include_email,
        identity_include_name: config.identity_include_name,
        identity_jwt_audience: config.identity_jwt_audience.clone(),
        forward_access_token: config.forward_access_token,
        inject_delegation_token: config.inject_delegation_token,
        delegation_token_scope: normalized_scope,
    })
}

/// Validate a slug: 1-64 chars, lowercase alphanumeric + hyphens.
fn validate_slug(slug: &str) -> AppResult<()> {
    if slug.is_empty() || slug.len() > 64 {
        return Err(AppError::ValidationError(
            "Slug must be between 1 and 64 characters".to_string(),
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::ValidationError(
            "Slug must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(AppError::ValidationError(
            "Slug must not start or end with a hyphen".to_string(),
        ));
    }
    Ok(())
}

fn validate_auth_method(method: &str) -> AppResult<()> {
    if !VALID_AUTH_METHODS.contains(&method) {
        return Err(AppError::ValidationError(format!(
            "Invalid auth_method '{}'. Valid: {}",
            method,
            VALID_AUTH_METHODS.join(", ")
        )));
    }
    Ok(())
}

/// List all active user services for a user.
pub async fn list_user_services(
    db: &mongodb::Database,
    user_id: &str,
) -> AppResult<Vec<UserService>> {
    let services: Vec<UserService> = db
        .collection::<UserService>(COLLECTION_NAME)
        .find(doc! { "user_id": user_id, "is_active": true })
        .sort(doc! { "created_at": -1 })
        .await?
        .try_collect()
        .await?;
    Ok(services)
}

/// Get single user service by ID, verifying ownership.
pub async fn get_user_service(
    db: &mongodb::Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<UserService> {
    db.collection::<UserService>(COLLECTION_NAME)
        .find_one(doc! { "_id": service_id, "user_id": user_id })
        .await?
        .ok_or_else(|| AppError::NotFound("User service not found".to_string()))
}

/// Find a user service by slug for a given user.
pub async fn find_by_slug(
    db: &mongodb::Database,
    user_id: &str,
    slug: &str,
) -> AppResult<Option<UserService>> {
    Ok(db
        .collection::<UserService>(COLLECTION_NAME)
        .find_one(doc! { "user_id": user_id, "slug": slug, "is_active": true })
        .await?)
}

/// Find a user service by catalog_service_id for a given user.
pub async fn find_by_catalog_service_id(
    db: &mongodb::Database,
    user_id: &str,
    catalog_service_id: &str,
) -> AppResult<Option<UserService>> {
    Ok(db
        .collection::<UserService>(COLLECTION_NAME)
        .find_one(doc! {
            "user_id": user_id,
            "catalog_service_id": catalog_service_id,
            "is_active": true,
        })
        .await?)
}

/// Create a new user service.
#[allow(clippy::too_many_arguments)]
pub async fn create_user_service(
    db: &mongodb::Database,
    user_id: &str,
    slug: &str,
    endpoint_id: &str,
    api_key_id: Option<&str>,
    auth_method: &str,
    auth_key_name: &str,
    catalog_service_id: Option<&str>,
    node_id: Option<&str>,
    node_priority: i32,
    service_type: &str,
    source: Option<&str>,
    source_id: Option<&str>,
    identity: &IdentityConfig,
) -> AppResult<UserService> {
    validate_slug(slug)?;
    validate_auth_method(auth_method)?;
    let identity = normalize_identity_config(identity)?;
    let node_id = node_id.filter(|nid| !nid.is_empty());

    if source.is_some() != source_id.is_some() {
        return Err(AppError::ValidationError(
            "source and source_id must be provided together".to_string(),
        ));
    }

    if auth_key_name.len() > 200 || auth_key_name.contains('\r') || auth_key_name.contains('\n') {
        return Err(AppError::ValidationError(
            "Invalid auth_key_name".to_string(),
        ));
    }

    if api_key_id.is_none() && auth_method != "none" {
        return Err(AppError::ValidationError(
            "Services without an API key must use auth_method 'none'".to_string(),
        ));
    }

    // Verify endpoint exists and belongs to user
    let ep_count = db
        .collection::<mongodb::bson::Document>(USER_ENDPOINTS)
        .count_documents(doc! { "_id": endpoint_id, "user_id": user_id })
        .await?;
    if ep_count == 0 {
        return Err(AppError::NotFound(
            "Endpoint not found or does not belong to user".to_string(),
        ));
    }

    // Verify api_key exists and belongs to user (skip for no-auth services)
    if let Some(ak_id) = api_key_id {
        let ak_count = db
            .collection::<mongodb::bson::Document>(USER_API_KEYS)
            .count_documents(doc! { "_id": ak_id, "user_id": user_id })
            .await?;
        if ak_count == 0 {
            return Err(AppError::NotFound(
                "API key not found or does not belong to user".to_string(),
            ));
        }
    }

    // Check slug uniqueness for active services
    let existing = find_by_slug(db, user_id, slug).await?;
    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "You already have an active service with slug '{slug}'"
        )));
    }

    if let Some(node_id) = node_id {
        node_service::get_node(db, user_id, node_id).await?;
    }

    let now = Utc::now();
    let service = UserService {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        slug: slug.to_string(),
        endpoint_id: endpoint_id.to_string(),
        api_key_id: api_key_id.map(|s| s.to_string()),
        auth_method: auth_method.to_string(),
        auth_key_name: auth_key_name.to_string(),
        catalog_service_id: catalog_service_id.map(|s| s.to_string()),
        node_id: node_id.map(|s| s.to_string()),
        node_priority,
        service_type: service_type.to_string(),
        identity_propagation_mode: identity.identity_propagation_mode,
        identity_include_user_id: identity.identity_include_user_id,
        identity_include_email: identity.identity_include_email,
        identity_include_name: identity.identity_include_name,
        identity_jwt_audience: identity.identity_jwt_audience,
        forward_access_token: identity.forward_access_token,
        inject_delegation_token: identity.inject_delegation_token,
        delegation_token_scope: identity.delegation_token_scope,
        is_active: true,
        source: source.map(str::to_string),
        source_id: source_id.map(str::to_string),
        created_at: now,
        updated_at: now,
    };

    db.collection::<UserService>(COLLECTION_NAME)
        .insert_one(&service)
        .await?;

    Ok(service)
}

/// Update service config (auth method, node routing, identity propagation, etc.).
#[allow(clippy::too_many_arguments)]
pub async fn update_user_service(
    db: &mongodb::Database,
    user_id: &str,
    service_id: &str,
    auth_method: Option<&str>,
    auth_key_name: Option<&str>,
    node_id: Option<&str>,
    node_priority: Option<i32>,
    is_active: Option<bool>,
    identity: Option<&IdentityConfig>,
) -> AppResult<()> {
    let current = get_user_service(db, user_id, service_id).await?;
    let mut set_doc = doc! {
        "updated_at": bson::DateTime::from_chrono(Utc::now()),
    };

    if let Some(am) = auth_method {
        validate_auth_method(am)?;
        if am != "none" && current.api_key_id.is_none() {
            return Err(AppError::BadRequest(
                "This service has no stored credential. Add one before changing auth_method."
                    .to_string(),
            ));
        }
        set_doc.insert("auth_method", am);
    }
    if let Some(akn) = auth_key_name {
        set_doc.insert("auth_key_name", akn);
    }
    if let Some(nid) = node_id {
        if nid.is_empty() {
            // Empty string clears the node_id
            set_doc.insert("node_id", bson::Bson::Null);
        } else {
            node_service::get_node(db, user_id, nid).await?;
            set_doc.insert("node_id", nid);
        }
    }
    if let Some(np) = node_priority {
        set_doc.insert("node_priority", np);
    }
    if let Some(active) = is_active {
        set_doc.insert("is_active", active);
    }
    if let Some(id_config) = identity {
        let id_config = normalize_identity_config(id_config)?;
        set_doc.insert(
            "identity_propagation_mode",
            &id_config.identity_propagation_mode,
        );
        set_doc.insert(
            "identity_include_user_id",
            id_config.identity_include_user_id,
        );
        set_doc.insert("identity_include_email", id_config.identity_include_email);
        set_doc.insert("identity_include_name", id_config.identity_include_name);
        match &id_config.identity_jwt_audience {
            Some(aud) => {
                set_doc.insert("identity_jwt_audience", aud);
            }
            None => {
                set_doc.insert("identity_jwt_audience", bson::Bson::Null);
            }
        }
        set_doc.insert("forward_access_token", id_config.forward_access_token);
        set_doc.insert("inject_delegation_token", id_config.inject_delegation_token);
        set_doc.insert("delegation_token_scope", &id_config.delegation_token_scope);
    }

    let result = db
        .collection::<UserService>(COLLECTION_NAME)
        .update_one(
            doc! { "_id": service_id, "user_id": user_id },
            doc! { "$set": set_doc },
        )
        .await?;

    if result.matched_count == 0 {
        return Err(AppError::NotFound("User service not found".to_string()));
    }

    Ok(())
}

/// Deactivate a user service (soft delete).
pub async fn deactivate_user_service(
    db: &mongodb::Database,
    user_id: &str,
    service_id: &str,
) -> AppResult<()> {
    update_user_service(
        db,
        user_id,
        service_id,
        None,
        None,
        None,
        None,
        Some(false),
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity_config() -> IdentityConfig {
        IdentityConfig {
            identity_propagation_mode: "headers".to_string(),
            identity_include_user_id: true,
            identity_include_email: true,
            identity_include_name: false,
            identity_jwt_audience: None,
            forward_access_token: false,
            inject_delegation_token: true,
            delegation_token_scope: "llm:proxy".to_string(),
        }
    }

    #[test]
    fn normalize_identity_config_defaults_blank_scope() {
        let mut config = sample_identity_config();
        config.delegation_token_scope = "   ".to_string();

        let normalized = normalize_identity_config(&config).expect("scope should normalize");
        assert_eq!(normalized.delegation_token_scope, "llm:proxy");
    }

    #[test]
    fn normalize_identity_config_rejects_invalid_scope() {
        let mut config = sample_identity_config();
        config.delegation_token_scope = "admin:full".to_string();

        let error = normalize_identity_config(&config).expect_err("scope should be rejected");
        assert!(matches!(
            error,
            AppError::ValidationError(message)
                if message.contains("Invalid delegation_token_scope")
        ));
    }

    #[test]
    fn normalize_identity_config_rejects_overlong_audience() {
        let mut config = sample_identity_config();
        config.identity_jwt_audience = Some("a".repeat(2049));

        let error =
            normalize_identity_config(&config).expect_err("audience length should be enforced");
        assert!(matches!(
            error,
            AppError::ValidationError(message)
                if message.contains("identity_jwt_audience must not exceed 2048 characters")
        ));
    }

    #[test]
    fn normalize_identity_config_preserves_valid_multiple_scopes() {
        let mut config = sample_identity_config();
        config.delegation_token_scope = "proxy:*   llm:status".to_string();

        let normalized = normalize_identity_config(&config).expect("scopes should validate");
        assert_eq!(normalized.delegation_token_scope, "proxy:* llm:status");
    }
}
