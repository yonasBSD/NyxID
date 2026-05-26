use chrono::Utc;
use uuid::Uuid;

use crate::models::audit_log::{AuditLog, COLLECTION_NAME as AUDIT_LOG};
use crate::mw::auth::AuthUser;

/// Fire-and-forget audit log entry.
///
/// Spawns a background task to write the audit record so that the calling
/// handler is not blocked by the database write. Errors are logged but
/// do not propagate.
#[allow(clippy::too_many_arguments)]
pub fn log_async(
    db: mongodb::Database,
    user_id: Option<String>,
    event_type: String,
    event_data: Option<serde_json::Value>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    api_key_id: Option<String>,
    api_key_name: Option<String>,
) {
    tokio::spawn(async move {
        let entry = AuditLog {
            id: Uuid::new_v4().to_string(),
            user_id,
            event_type: event_type.clone(),
            event_data,
            ip_address,
            user_agent,
            api_key_id,
            api_key_name,
            created_at: Utc::now(),
        };

        if let Err(e) = db
            .collection::<AuditLog>(AUDIT_LOG)
            .insert_one(&entry)
            .await
        {
            tracing::error!(event_type = %event_type, error = %e, "Failed to write audit log");
        }
    });
}

/// Fire-and-forget audit log entry attributed to an authenticated request.
///
/// Pulls user_id, IP, User-Agent, and API key identity from `AuthUser` so
/// every audit record carries consistent forensic context. Prefer this over
/// `log_async` when the call site already has an `AuthUser` extractor.
pub fn log_for_user(
    db: mongodb::Database,
    auth_user: &AuthUser,
    event_type: impl Into<String>,
    event_data: Option<serde_json::Value>,
) {
    log_async(
        db,
        Some(auth_user.user_id.to_string()),
        event_type.into(),
        event_data,
        auth_user.ip_address.clone(),
        auth_user.user_agent.clone(),
        auth_user.api_key_id.clone(),
        auth_user.api_key_name.clone(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::audit_log::{AuditLog, COLLECTION_NAME as AUDIT_LOG};
    use crate::test_utils::{connect_test_database, test_auth_user};
    use futures::TryStreamExt;

    #[tokio::test]
    async fn log_async_inserts_audit_entry() {
        let Some(db) = connect_test_database("audit_log_async").await else {
            return;
        };

        log_async(
            db.clone(),
            Some("user-123".to_string()),
            "test_event".to_string(),
            Some(serde_json::json!({"key": "value"})),
            Some("127.0.0.1".to_string()),
            Some("test-agent".to_string()),
            None,
            None,
        );

        // Wait for the spawned task to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let entries: Vec<AuditLog> = db
            .collection::<AuditLog>(AUDIT_LOG)
            .find(mongodb::bson::doc! { "event_type": "test_event" })
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.user_id.as_deref(), Some("user-123"));
        assert_eq!(entry.event_type, "test_event");
        assert_eq!(entry.ip_address.as_deref(), Some("127.0.0.1"));
        assert_eq!(entry.user_agent.as_deref(), Some("test-agent"));
        assert!(entry.api_key_id.is_none());
        assert!(entry.api_key_name.is_none());
    }

    #[tokio::test]
    async fn log_async_with_api_key_fields() {
        let Some(db) = connect_test_database("audit_log_apikey").await else {
            return;
        };

        log_async(
            db.clone(),
            Some("user-456".to_string()),
            "proxy_request".to_string(),
            None,
            None,
            None,
            Some("key-id-1".to_string()),
            Some("my-agent".to_string()),
        );

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let entries: Vec<AuditLog> = db
            .collection::<AuditLog>(AUDIT_LOG)
            .find(mongodb::bson::doc! { "event_type": "proxy_request" })
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].api_key_id.as_deref(), Some("key-id-1"));
        assert_eq!(entries[0].api_key_name.as_deref(), Some("my-agent"));
    }

    #[tokio::test]
    async fn log_for_user_extracts_auth_user_fields() {
        let Some(db) = connect_test_database("audit_log_for_user").await else {
            return;
        };

        let mut auth = test_auth_user("550e8400-e29b-41d4-a716-446655440099");
        auth.ip_address = Some("10.0.0.1".to_string());
        auth.user_agent = Some("Mozilla/5.0".to_string());
        auth.api_key_id = Some("ak-1".to_string());
        auth.api_key_name = Some("agent-name".to_string());

        log_for_user(
            db.clone(),
            &auth,
            "user_action",
            Some(serde_json::json!({"detail": "test"})),
        );

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let entries: Vec<AuditLog> = db
            .collection::<AuditLog>(AUDIT_LOG)
            .find(mongodb::bson::doc! { "event_type": "user_action" })
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(
            entry.user_id.as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440099")
        );
        assert_eq!(entry.ip_address.as_deref(), Some("10.0.0.1"));
        assert_eq!(entry.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(entry.api_key_id.as_deref(), Some("ak-1"));
        assert_eq!(entry.api_key_name.as_deref(), Some("agent-name"));
    }
}
