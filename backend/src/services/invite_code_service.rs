use chrono::Utc;
use mongodb::bson::{self, doc};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use rand::Rng;
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::models::invite_code::{COLLECTION_NAME, InviteCode, InviteCodeUsage};

/// Fixed prefix used for all generated codes so admins can visually
/// distinguish an invite code from other credentials in logs / UI.
pub const CODE_PREFIX: &str = "NYX-";

/// Length (in characters) of the random suffix appended after `CODE_PREFIX`.
const CODE_SUFFIX_LEN: usize = 8;

/// Character set used to generate the random suffix. Excludes lowercase and
/// ambiguous characters to keep codes readable when shared verbally.
const CODE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Number of attempts when retrying `insert_one` on a unique-index collision.
/// 36^8 ≈ 2.8 trillion combinations so in practice the first attempt always
/// succeeds; the retry loop guards against the pathological case of repeated
/// collisions rather than leaking a raw DB error to the admin.
const CODE_INSERT_MAX_ATTEMPTS: usize = 5;

/// Generate a code like "NYX-XXXXXXXX" (8 random alphanumeric uppercase chars).
pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let suffix: String = (0..CODE_SUFFIX_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CODE_CHARSET.len());
            CODE_CHARSET[idx] as char
        })
        .collect();
    format!("{CODE_PREFIX}{suffix}")
}

/// Normalize an invite code to its canonical storage form.
///
/// Codes are stored uppercased and trimmed so that user input is matched
/// regardless of whitespace or casing. This pairs with the frontend's
/// uppercase-on-type behavior and the `nyxid` CLI's normalization.
pub fn normalize_code(input: &str) -> String {
    input.trim().to_uppercase()
}

/// Return true if the given MongoDB error represents a unique-index violation.
fn is_duplicate_key_error(e: &mongodb::error::Error) -> bool {
    matches!(
        e.kind.as_ref(),
        mongodb::error::ErrorKind::Write(mongodb::error::WriteFailure::WriteError(we))
            if we.code == 11000
    )
}

/// Create a new invite code.
///
/// Retries on unique-index collision (extremely unlikely in practice but
/// possible in principle, and we never want to surface a raw DB error to the
/// admin for this branch).
pub async fn create_invite_code(
    db: &mongodb::Database,
    created_by: &str,
    max_uses: i32,
    note: Option<&str>,
) -> AppResult<InviteCode> {
    let collection = db.collection::<InviteCode>(COLLECTION_NAME);

    for attempt in 0..CODE_INSERT_MAX_ATTEMPTS {
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
            usages: Vec::new(),
        };

        match collection.insert_one(&invite).await {
            Ok(_) => return Ok(invite),
            Err(e) if is_duplicate_key_error(&e) => {
                tracing::warn!(
                    attempt = attempt + 1,
                    code = %invite.code,
                    "Invite code generation collision, retrying"
                );
                continue;
            }
            Err(e) => return Err(AppError::DatabaseError(e)),
        }
    }

    Err(AppError::Internal(
        "Failed to generate unique invite code after retries".to_string(),
    ))
}

/// Atomically reserve one slot on an invite code.
///
/// Returns `Ok(invite_code_id)` on success, or `Err(AppError::BadRequest)` if
/// the code is unknown, inactive, or exhausted. This only touches `used_count`;
/// the caller is responsible for recording the usage (via `record_usage`) once
/// the downstream user creation succeeds, and releasing (via `release_reservation`)
/// if it fails.
pub async fn reserve_invite_code(db: &mongodb::Database, code: &str) -> AppResult<String> {
    let normalized = normalize_code(code);
    let now = bson::DateTime::from_chrono(Utc::now());

    let options = FindOneAndUpdateOptions::builder()
        .return_document(ReturnDocument::After)
        .build();

    let result = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .find_one_and_update(
            doc! {
                "code": &normalized,
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

/// Record which user redeemed a previously-reserved invite code.
///
/// Best-effort append to the `usages` array. Failures are logged but do not
/// propagate: the user has already been successfully created, and the atomic
/// reservation still accurately reflects that one slot was consumed.
pub async fn record_usage(db: &mongodb::Database, invite_code_id: &str, user_id: &str) {
    let usage = InviteCodeUsage {
        user_id: user_id.to_string(),
        used_at: Utc::now(),
    };
    let usage_bson = match bson::to_bson(&usage) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(
                error = %e,
                invite_code_id = %invite_code_id,
                "Failed to serialize invite code usage"
            );
            return;
        }
    };

    let now = bson::DateTime::from_chrono(Utc::now());

    if let Err(e) = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .update_one(
            doc! { "_id": invite_code_id },
            doc! {
                "$push": { "usages": usage_bson },
                "$set": { "updated_at": now },
            },
        )
        .await
    {
        tracing::error!(
            error = %e,
            invite_code_id = %invite_code_id,
            user_id = %user_id,
            "Failed to record invite code usage"
        );
    }
}

/// Release a previously-reserved slot, decrementing `used_count`.
///
/// Used to compensate when `reserve_invite_code` has succeeded but the
/// downstream user creation fails (or silently no-ops for email enumeration
/// protection). Best-effort: errors are logged but not surfaced to the caller
/// because the user-facing request has already failed for an unrelated reason.
pub async fn release_reservation(db: &mongodb::Database, invite_code_id: &str) {
    let now = bson::DateTime::from_chrono(Utc::now());

    if let Err(e) = db
        .collection::<InviteCode>(COLLECTION_NAME)
        .update_one(
            doc! {
                "_id": invite_code_id,
                "used_count": { "$gt": 0 },
            },
            doc! {
                "$inc": { "used_count": -1 },
                "$set": { "updated_at": now },
            },
        )
        .await
    {
        tracing::error!(
            error = %e,
            invite_code_id = %invite_code_id,
            "Failed to release invite code reservation"
        );
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
    let now = bson::DateTime::from_chrono(Utc::now());

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generate_code_has_expected_format() {
        for _ in 0..100 {
            let code = generate_code();
            assert!(code.starts_with(CODE_PREFIX));
            let suffix = &code[CODE_PREFIX.len()..];
            assert_eq!(suffix.len(), CODE_SUFFIX_LEN);
            assert!(
                suffix.chars().all(|c| CODE_CHARSET.contains(&(c as u8))),
                "suffix contained invalid character: {suffix}"
            );
        }
    }

    #[test]
    fn generate_code_is_sufficiently_random() {
        // With 36^8 combinations, 1000 samples should all be unique in practice.
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            let code = generate_code();
            assert!(
                seen.insert(code.clone()),
                "duplicate code generated within 1000 samples: {code}"
            );
        }
    }

    #[test]
    fn normalize_code_uppercases_and_trims() {
        assert_eq!(normalize_code("nyx-abc123"), "NYX-ABC123");
        assert_eq!(normalize_code("  NYX-abc123  "), "NYX-ABC123");
        assert_eq!(normalize_code("Nyx-AbC123"), "NYX-ABC123");
    }

    #[test]
    fn normalize_code_handles_empty_input() {
        assert_eq!(normalize_code(""), "");
        assert_eq!(normalize_code("   "), "");
    }

    #[test]
    fn normalize_matches_generated_code() {
        let generated = generate_code();
        // A newly-generated code is already in canonical form; normalizing
        // should be a no-op.
        assert_eq!(normalize_code(&generated), generated);
        // Lowercase variant of the same code should normalize to the generated form.
        assert_eq!(normalize_code(&generated.to_lowercase()), generated);
    }
}
