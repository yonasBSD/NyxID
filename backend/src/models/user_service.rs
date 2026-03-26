use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const COLLECTION_NAME: &str = "user_services";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserService {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    /// Proxy path slug (e.g., "llm-openai", "my-custom-api")
    pub slug: String,
    /// FK to UserEndpoint
    pub endpoint_id: String,
    /// FK to UserApiKey (None for no-auth auto-connected services)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<String>,
    /// "bearer" | "header" | "query" | "basic" | "none"
    pub auth_method: String,
    /// Header name or query param name (e.g., "Authorization", "x-api-key", "key")
    pub auth_key_name: String,
    /// Optional: populated when auto-provisioned from catalog
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    /// Optional: route requests through this node agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Failover priority (lower = higher priority, default 0)
    #[serde(default)]
    pub node_priority: i32,
    /// "http" (default) | "ssh"
    #[serde(default = "default_service_type")]
    pub service_type: String,

    pub is_active: bool,

    /// Source tracking for migration idempotency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

fn default_service_type() -> String {
    "http".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "user_services");
    }

    #[test]
    fn bson_roundtrip() {
        let svc = UserService {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: uuid::Uuid::new_v4().to_string(),
            slug: "llm-openai".to_string(),
            endpoint_id: uuid::Uuid::new_v4().to_string(),
            api_key_id: Some(uuid::Uuid::new_v4().to_string()),
            auth_method: "bearer".to_string(),
            auth_key_name: "Authorization".to_string(),
            catalog_service_id: Some("svc-id".to_string()),
            node_id: Some("node-1".to_string()),
            node_priority: 0,
            service_type: "http".to_string(),
            is_active: true,
            source: None,
            source_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let doc = bson::to_document(&svc).expect("serialize");
        let restored: UserService = bson::from_document(doc).expect("deserialize");
        assert_eq!(svc.id, restored.id);
        assert_eq!(svc.slug, restored.slug);
        assert_eq!(svc.node_priority, restored.node_priority);
        assert_eq!(restored.service_type, "http");
        assert!(restored.is_active);
    }

    #[test]
    fn bson_defaults() {
        let svc = UserService {
            id: "id".to_string(),
            user_id: "uid".to_string(),
            slug: "test".to_string(),
            endpoint_id: "ep".to_string(),
            api_key_id: Some("ak".to_string()),
            auth_method: "header".to_string(),
            auth_key_name: "X-API-Key".to_string(),
            catalog_service_id: None,
            node_id: None,
            node_priority: 0,
            service_type: "http".to_string(),
            is_active: true,
            source: None,
            source_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut doc = bson::to_document(&svc).expect("serialize");
        doc.remove("node_priority");
        doc.remove("service_type");
        let restored: UserService = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.node_priority, 0);
        assert_eq!(restored.service_type, "http");
    }

    #[test]
    fn bson_roundtrip_no_api_key() {
        let svc = UserService {
            id: "id".to_string(),
            user_id: "uid".to_string(),
            slug: "auto-svc".to_string(),
            endpoint_id: "ep".to_string(),
            api_key_id: None,
            auth_method: "none".to_string(),
            auth_key_name: String::new(),
            catalog_service_id: Some("cat-1".to_string()),
            node_id: None,
            node_priority: 0,
            service_type: "http".to_string(),
            is_active: true,
            source: None,
            source_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let doc = bson::to_document(&svc).expect("serialize");
        assert!(!doc.contains_key("api_key_id"), "None should be skipped");
        let restored: UserService = bson::from_document(doc).expect("deserialize");
        assert!(restored.api_key_id.is_none());
        assert_eq!(restored.auth_method, "none");
    }
}
