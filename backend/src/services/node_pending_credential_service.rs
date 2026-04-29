use chrono::{Duration, Utc};
use futures::TryStreamExt;
use mongodb::bson::{self, doc};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::models::node_pending_credential::{
    COLLECTION_NAME as NODE_PENDING_CREDENTIALS, InjectionMethod, NodePendingCredential,
};
use crate::services::{node_service, url_validation};

pub struct CreatePendingCredentialInput {
    pub service_slug: String,
    pub injection_method: InjectionMethod,
    pub field_name: String,
    pub target_url: Option<String>,
    pub label: Option<String>,
    pub ttl_secs: i64,
}

pub async fn create_pending_credential(
    db: &mongodb::Database,
    actor_user_id: &str,
    node_id: &str,
    input: CreatePendingCredentialInput,
) -> AppResult<NodePendingCredential> {
    validate_service_slug(&input.service_slug)?;
    validate_field_name(&input.field_name)?;
    let target_url = clean_optional_string(input.target_url);
    if let Some(url) = target_url.as_deref() {
        url_validation::validate_public_http_url(url, "target_url").await?;
    }
    let label = clean_optional_string(input.label);
    if let Some(label) = label.as_deref()
        && label.len() > 128
    {
        return Err(AppError::ValidationError(
            "label must be 128 characters or fewer".to_string(),
        ));
    }

    let node = node_service::ensure_node_writable_by_actor(db, actor_user_id, node_id).await?;
    let existing = db
        .collection::<NodePendingCredential>(NODE_PENDING_CREDENTIALS)
        .find_one(doc! {
            "node_id": node_id,
            "service_slug": &input.service_slug,
            "is_active": true,
        })
        .await?;
    if let Some(existing) = existing {
        return Err(AppError::Conflict(format!(
            "A pending credential already exists for service '{}' on this node (id: {})",
            input.service_slug, existing.id
        )));
    }

    let now = Utc::now();
    let expires_at = now + Duration::seconds(input.ttl_secs.max(1));
    let pending = NodePendingCredential {
        id: Uuid::new_v4().to_string(),
        node_id: node_id.to_string(),
        service_slug: input.service_slug,
        injection_method: input.injection_method,
        field_name: input.field_name,
        target_url,
        label,
        created_by_user_id: actor_user_id.to_string(),
        owner_user_id: node.user_id,
        created_at: now,
        expires_at,
        consumed_at: None,
        declined_at: None,
        is_active: true,
    };

    db.collection::<NodePendingCredential>(NODE_PENDING_CREDENTIALS)
        .insert_one(&pending)
        .await?;

    Ok(pending)
}

pub async fn list_pending_credentials_for_admin(
    db: &mongodb::Database,
    actor_user_id: &str,
    node_id: &str,
    include_history: bool,
) -> AppResult<Vec<NodePendingCredential>> {
    node_service::ensure_node_writable_by_actor(db, actor_user_id, node_id).await?;

    let mut filter = doc! { "node_id": node_id };
    if !include_history {
        filter.insert("is_active", true);
        filter.insert(
            "expires_at",
            doc! { "$gt": bson::DateTime::from_chrono(Utc::now()) },
        );
    }

    db.collection::<NodePendingCredential>(NODE_PENDING_CREDENTIALS)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .await?
        .try_collect()
        .await
        .map_err(AppError::from)
}

pub async fn cancel_pending_credential(
    db: &mongodb::Database,
    actor_user_id: &str,
    node_id: &str,
    pending_id: &str,
) -> AppResult<NodePendingCredential> {
    node_service::ensure_node_writable_by_actor(db, actor_user_id, node_id).await?;

    let now = bson::DateTime::from_chrono(Utc::now());
    db.collection::<NodePendingCredential>(NODE_PENDING_CREDENTIALS)
        .find_one_and_update(
            doc! {
                "_id": pending_id,
                "node_id": node_id,
                "is_active": true,
            },
            doc! { "$set": { "is_active": false, "updated_at": &now } },
        )
        .await?
        .ok_or_else(|| AppError::NotFound("Pending credential not found".to_string()))
}

fn validate_service_slug(slug: &str) -> AppResult<()> {
    if slug.is_empty() || slug.len() > 64 {
        return Err(AppError::ValidationError(
            "service_slug must be 1-64 characters".to_string(),
        ));
    }
    let valid = slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && slug
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !valid {
        return Err(AppError::ValidationError(
            "service_slug must be lowercase alphanumeric with optional hyphens, and cannot start or end with hyphen".to_string(),
        ));
    }
    Ok(())
}

fn validate_field_name(field_name: &str) -> AppResult<()> {
    if field_name.is_empty() || field_name.len() > 128 {
        return Err(AppError::ValidationError(
            "field_name must be 1-128 characters".to_string(),
        ));
    }
    if field_name.chars().any(char::is_control) {
        return Err(AppError::ValidationError(
            "field_name must not contain control characters".to_string(),
        ));
    }
    Ok(())
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}
