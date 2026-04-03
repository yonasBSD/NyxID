use chrono::Utc;
use mongodb::bson::doc;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use rand::Rng;
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::models::invite_code::{COLLECTION_NAME, InviteCode};

/// Generate a code like "NYX-XXXXXXXX" (8 random alphanumeric uppercase chars).
pub fn generate_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    format!("NYX-{suffix}")
}

/// Create a new invite code.
pub async fn create_invite_code(
    db: &mongodb::Database,
    created_by: &str,
    max_uses: i32,
    note: Option<&str>,
) -> AppResult<InviteCode> {
    let now = Utc::now();
    let invite = InviteCode {
        id: Uuid::new_v4().to_string(),
        code: generate_code(),
        max_uses,
        used_count: 0,
        created_by: created_by.to_string(),
        note: note.map(String::from),
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    db.collection::<InviteCode>(COLLECTION_NAME)
        .insert_one(&invite)
        .await?;

    Ok(invite)
}

/// Validate an invite code and atomically consume one use.
///
/// Returns `Ok(invite_code_id)` on success, or `Err(AppError::BadRequest)` if
/// the code is invalid, inactive, or exhausted.
pub async fn validate_and_consume(db: &mongodb::Database, code: &str) -> AppResult<String> {
    let now = mongodb::bson::DateTime::from_chrono(Utc::now());

    let options = FindOneAndUpdateOptions::builder()
        .return_document(ReturnDocument::After)
        .build();

    let result = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .find_one_and_update(
            doc! {
                "code": code,
                "is_active": true,
                "$expr": { "$lt": ["$used_count", "$max_uses"] },
            },
            doc! {
                "$inc": { "used_count": 1 },
                "$set": { "updated_at": now },
            },
        )
        .with_options(options)
        .await?;

    match result {
        Some(invite) => Ok(invite.id),
        None => Err(AppError::BadRequest(
            "Invalid or exhausted invite code".to_string(),
        )),
    }
}

/// List all invite codes (admin).
pub async fn list_invite_codes(db: &mongodb::Database) -> AppResult<Vec<InviteCode>> {
    use futures::TryStreamExt;

    let cursor = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .find(doc! {})
        .sort(doc! { "created_at": -1 })
        .await?;

    let codes: Vec<InviteCode> = cursor.try_collect().await?;
    Ok(codes)
}

/// Deactivate an invite code by ID.
pub async fn deactivate_invite_code(db: &mongodb::Database, id: &str) -> AppResult<()> {
    let now = mongodb::bson::DateTime::from_chrono(Utc::now());

    let result = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .update_one(
            doc! { "_id": id },
            doc! { "$set": { "is_active": false, "updated_at": now } },
        )
        .await?;

    if result.matched_count == 0 {
        return Err(AppError::NotFound("Invite code not found".to_string()));
    }

    Ok(())
}
