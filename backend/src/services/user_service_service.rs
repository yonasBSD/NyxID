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
    api_key_id: &str,
    auth_method: &str,
    auth_key_name: &str,
    catalog_service_id: Option<&str>,
    node_id: Option<&str>,
    node_priority: i32,
    service_type: &str,
) -> AppResult<UserService> {
    validate_slug(slug)?;
    validate_auth_method(auth_method)?;
    let node_id = node_id.filter(|nid| !nid.is_empty());

    if auth_key_name.len() > 200 || auth_key_name.contains('\r') || auth_key_name.contains('\n') {
        return Err(AppError::ValidationError(
            "Invalid auth_key_name".to_string(),
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

    // Verify api_key exists and belongs to user
    let ak_count = db
        .collection::<mongodb::bson::Document>(USER_API_KEYS)
        .count_documents(doc! { "_id": api_key_id, "user_id": user_id })
        .await?;
    if ak_count == 0 {
        return Err(AppError::NotFound(
            "API key not found or does not belong to user".to_string(),
        ));
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
        api_key_id: api_key_id.to_string(),
        auth_method: auth_method.to_string(),
        auth_key_name: auth_key_name.to_string(),
        catalog_service_id: catalog_service_id.map(|s| s.to_string()),
        node_id: node_id.map(|s| s.to_string()),
        node_priority,
        service_type: service_type.to_string(),
        is_active: true,
        source: None,
        source_id: None,
        created_at: now,
        updated_at: now,
    };

    db.collection::<UserService>(COLLECTION_NAME)
        .insert_one(&service)
        .await?;

    Ok(service)
}

/// Update service config (auth method, node routing, etc.).
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
) -> AppResult<()> {
    let mut set_doc = doc! {
        "updated_at": bson::DateTime::from_chrono(Utc::now()),
    };

    if let Some(am) = auth_method {
        validate_auth_method(am)?;
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
    update_user_service(db, user_id, service_id, None, None, None, None, Some(false)).await
}
