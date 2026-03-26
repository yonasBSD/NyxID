use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;
use crate::models::service_approval_config::{ApprovalMode, legacy_approval_mode_default};

pub const COLLECTION_NAME: &str = "approval_requests";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// UUID v4 string
    #[serde(rename = "_id")]
    pub id: String,

    /// The user who must approve this request
    pub user_id: String,

    /// The downstream service being accessed
    pub service_id: String,

    /// Human-readable service name (denormalized for Telegram message)
    pub service_name: String,

    /// The service slug (denormalized for display)
    pub service_slug: String,

    /// Who is making the request: "user", "service_account", or "delegated"
    pub requester_type: String,

    /// ID of the requester (user_id, service_account_id, or client_id)
    pub requester_id: String,

    /// Human-readable requester label (e.g. SA name, OAuth client name)
    pub requester_label: Option<String>,

    /// What operation is being performed (e.g. "proxy:GET /v1/chat/completions")
    pub operation_summary: String,

    /// Rich human-readable description of what the API request does.
    /// e.g., "POST /v1/chat/completions (model: gpt-4, max_tokens: 1000)"
    /// Falls back to operation_summary if not generated.
    #[serde(default)]
    pub action_description: Option<String>,

    /// Approval semantics captured at request creation time.
    /// Legacy requests created before this field existed default to grant mode
    /// so their original behavior is preserved when decided later.
    #[serde(default = "legacy_approval_mode_default")]
    pub approval_mode: ApprovalMode,

    /// "pending" | "approved" | "rejected" | "expired"
    pub status: String,

    /// Pending request dedupe key.
    /// Grant mode uses a stable hash for `(user, service, requester)`;
    /// per-request mode uses a unique value per incoming API call.
    pub idempotency_key: String,

    /// Which notification channel delivered this request (e.g. "telegram")
    #[serde(default)]
    pub notification_channel: Option<String>,

    /// Telegram message_id for editing the message after decision
    #[serde(default)]
    pub telegram_message_id: Option<i64>,

    /// Telegram chat_id where the notification was sent
    #[serde(default)]
    pub telegram_chat_id: Option<i64>,

    /// When the approval request expires (auto-reject after this time)
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub expires_at: DateTime<Utc>,

    /// When the user made their decision (approved/rejected)
    #[serde(default, with = "bson_datetime::optional")]
    pub decided_at: Option<DateTime<Utc>>,

    /// Channel through which the decision was made (e.g. "telegram", "web")
    #[serde(default)]
    pub decision_channel: Option<String>,

    /// Idempotency key used for the final decision submission.
    /// System-generated expiry sweeps may also stamp an internal marker here
    /// to identify the rows they expired.
    #[serde(default)]
    pub decision_idempotency_key: Option<String>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "approval_requests");
    }

    fn make_approval_request() -> ApprovalRequest {
        ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: uuid::Uuid::new_v4().to_string(),
            service_id: uuid::Uuid::new_v4().to_string(),
            service_name: "OpenAI API".to_string(),
            service_slug: "openai".to_string(),
            requester_type: "service_account".to_string(),
            requester_id: uuid::Uuid::new_v4().to_string(),
            requester_label: Some("CI Pipeline".to_string()),
            operation_summary: "proxy:POST /v1/chat/completions".to_string(),
            action_description: Some(
                "POST /v1/chat/completions (model: gpt-4, 3 messages)".to_string(),
            ),
            approval_mode: ApprovalMode::PerRequest,
            status: "pending".to_string(),
            idempotency_key: "abc123".to_string(),
            notification_channel: Some("telegram".to_string()),
            telegram_message_id: Some(12345),
            telegram_chat_id: Some(67890),
            expires_at: Utc::now(),
            decided_at: None,
            decision_channel: None,
            decision_idempotency_key: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn bson_roundtrip() {
        let req = make_approval_request();
        let doc = bson::to_document(&req).expect("serialize");
        assert!(doc.get_str("_id").is_ok());
        assert!(doc.get("id").is_none(), "raw 'id' should not exist in bson");
        let restored: ApprovalRequest = bson::from_document(doc).expect("deserialize");
        assert_eq!(req.id, restored.id);
        assert_eq!(req.user_id, restored.user_id);
        assert_eq!(req.status, restored.status);
    }

    #[test]
    fn bson_roundtrip_with_optional_datetime() {
        let mut req = make_approval_request();
        req.decided_at = Some(Utc::now());
        req.decision_channel = Some("telegram".to_string());
        let doc = bson::to_document(&req).expect("serialize");
        let restored: ApprovalRequest = bson::from_document(doc).expect("deserialize");
        assert!(restored.decided_at.is_some());
    }

    #[test]
    fn missing_action_description_defaults_to_none() {
        let req = make_approval_request();
        let mut doc = bson::to_document(&req).expect("serialize");
        doc.remove("action_description");
        let restored: ApprovalRequest = bson::from_document(doc).expect("deserialize");
        assert!(restored.action_description.is_none());
    }

    #[test]
    fn action_description_roundtrips() {
        let req = make_approval_request();
        let doc = bson::to_document(&req).expect("serialize");
        let restored: ApprovalRequest = bson::from_document(doc).expect("deserialize");
        assert_eq!(
            restored.action_description.as_deref(),
            Some("POST /v1/chat/completions (model: gpt-4, 3 messages)")
        );
    }

    #[test]
    fn missing_approval_mode_defaults_to_grant_for_legacy_requests() {
        let req = make_approval_request();
        let mut doc = bson::to_document(&req).expect("serialize");
        doc.remove("approval_mode");
        let restored: ApprovalRequest = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.approval_mode, ApprovalMode::Grant);
    }

    #[test]
    fn bson_all_fields_serialized() {
        let req = make_approval_request();
        let doc = bson::to_document(&req).expect("serialize");
        let keys: Vec<&str> = doc.keys().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"_id"));
        assert!(keys.contains(&"user_id"));
        assert!(keys.contains(&"service_id"));
        assert!(keys.contains(&"service_name"));
        assert!(keys.contains(&"service_slug"));
        assert!(keys.contains(&"requester_type"));
        assert!(keys.contains(&"requester_id"));
        assert!(keys.contains(&"operation_summary"));
        assert!(keys.contains(&"approval_mode"));
        assert!(keys.contains(&"status"));
        assert!(keys.contains(&"idempotency_key"));
        assert!(keys.contains(&"expires_at"));
        assert!(keys.contains(&"created_at"));
    }
}
