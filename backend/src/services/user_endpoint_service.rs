use chrono::Utc;
use futures::TryStreamExt;
use mongodb::bson::{self, doc};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::handlers::services_helpers::validate_base_url;
use crate::models::user_endpoint::{COLLECTION_NAME, UserEndpoint};
use crate::models::user_service::COLLECTION_NAME as USER_SERVICES;

/// List all endpoints for a user, sorted by created_at descending.
pub async fn list_endpoints(db: &mongodb::Database, user_id: &str) -> AppResult<Vec<UserEndpoint>> {
    let endpoints: Vec<UserEndpoint> = db
        .collection::<UserEndpoint>(COLLECTION_NAME)
        .find(doc! { "user_id": user_id })
        .sort(doc! { "created_at": -1 })
        .await?
        .try_collect()
        .await?;
    Ok(endpoints)
}

/// Get single endpoint by ID, verifying ownership.
pub async fn get_endpoint(
    db: &mongodb::Database,
    user_id: &str,
    endpoint_id: &str,
) -> AppResult<UserEndpoint> {
    db.collection::<UserEndpoint>(COLLECTION_NAME)
        .find_one(doc! { "_id": endpoint_id, "user_id": user_id })
        .await?
        .ok_or_else(|| AppError::NotFound("Endpoint not found".to_string()))
}

/// Create a new endpoint.
pub async fn create_endpoint(
    db: &mongodb::Database,
    user_id: &str,
    label: &str,
    url: &str,
    catalog_service_id: Option<&str>,
) -> AppResult<UserEndpoint> {
    if label.is_empty() || label.len() > 200 {
        return Err(AppError::ValidationError(
            "Label must be between 1 and 200 characters".to_string(),
        ));
    }
    // Skip URL validation for node-resolved endpoints (empty URL)
    if !url.is_empty() {
        validate_base_url(url)?;
    }

    let now = Utc::now();
    let endpoint = UserEndpoint {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        label: label.to_string(),
        url: url.to_string(),
        catalog_service_id: catalog_service_id.map(|s| s.to_string()),
        created_at: now,
        updated_at: now,
    };

    db.collection::<UserEndpoint>(COLLECTION_NAME)
        .insert_one(&endpoint)
        .await?;

    Ok(endpoint)
}

/// Update endpoint URL and/or label.
pub async fn update_endpoint(
    db: &mongodb::Database,
    user_id: &str,
    endpoint_id: &str,
    url: Option<&str>,
    label: Option<&str>,
) -> AppResult<()> {
    if url.is_none() && label.is_none() {
        return Err(AppError::BadRequest(
            "At least one field must be provided".to_string(),
        ));
    }

    let mut set_doc = doc! {
        "updated_at": bson::DateTime::from_chrono(Utc::now()),
    };

    if let Some(u) = url {
        validate_base_url(u)?;
        set_doc.insert("url", u);
    }
    if let Some(l) = label {
        if l.is_empty() || l.len() > 200 {
            return Err(AppError::ValidationError(
                "Label must be between 1 and 200 characters".to_string(),
            ));
        }
        set_doc.insert("label", l);
    }

    let result = db
        .collection::<UserEndpoint>(COLLECTION_NAME)
        .update_one(
            doc! { "_id": endpoint_id, "user_id": user_id },
            doc! { "$set": set_doc },
        )
        .await?;

    if result.matched_count == 0 {
        return Err(AppError::NotFound("Endpoint not found".to_string()));
    }

    Ok(())
}

/// Delete endpoint. Fails if any active UserService references it.
pub async fn delete_endpoint(
    db: &mongodb::Database,
    user_id: &str,
    endpoint_id: &str,
) -> AppResult<()> {
    // Verify ownership
    let _ = get_endpoint(db, user_id, endpoint_id).await?;

    // Check for active references
    let ref_count = db
        .collection::<mongodb::bson::Document>(USER_SERVICES)
        .count_documents(doc! {
            "endpoint_id": endpoint_id,
            "is_active": true,
        })
        .await?;

    if ref_count > 0 {
        return Err(AppError::Conflict(
            "Endpoint is in use by active services".to_string(),
        ));
    }

    db.collection::<UserEndpoint>(COLLECTION_NAME)
        .delete_one(doc! { "_id": endpoint_id, "user_id": user_id })
        .await?;

    Ok(())
}
