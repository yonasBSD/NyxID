use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::bson_datetime;

pub const COLLECTION_NAME: &str = "node_pending_credentials";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InjectionMethod {
    Header,
    QueryParam,
    PathPrefix,
}

impl InjectionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::QueryParam => "query-param",
            Self::PathPrefix => "path-prefix",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodePendingCredential {
    #[serde(rename = "_id")]
    pub id: String,
    pub node_id: String,
    pub service_slug: String,
    pub injection_method: InjectionMethod,
    pub field_name: String,
    pub target_url: Option<String>,
    pub label: Option<String>,
    pub created_by_user_id: String,
    pub owner_user_id: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub expires_at: DateTime<Utc>,
    #[serde(default, with = "bson_datetime::optional")]
    pub consumed_at: Option<DateTime<Utc>>,
    #[serde(default, with = "bson_datetime::optional")]
    pub declined_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "node_pending_credentials");
    }

    #[test]
    fn injection_method_as_str() {
        assert_eq!(InjectionMethod::Header.as_str(), "header");
        assert_eq!(InjectionMethod::QueryParam.as_str(), "query-param");
        assert_eq!(InjectionMethod::PathPrefix.as_str(), "path-prefix");
    }

    #[test]
    fn injection_method_serde_kebab_case() {
        let json = serde_json::to_string(&InjectionMethod::QueryParam).unwrap();
        assert_eq!(json, "\"query-param\"");
        let back: InjectionMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InjectionMethod::QueryParam);
    }

    #[test]
    fn bson_roundtrip() {
        let cred = NodePendingCredential {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: "node-1".to_string(),
            service_slug: "openai".to_string(),
            injection_method: InjectionMethod::Header,
            field_name: "Authorization".to_string(),
            target_url: Some("https://api.openai.com".to_string()),
            label: Some("OpenAI key".to_string()),
            created_by_user_id: uuid::Uuid::new_v4().to_string(),
            owner_user_id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            consumed_at: None,
            declined_at: None,
            is_active: true,
        };
        let doc = bson::to_document(&cred).expect("serialize");
        let restored: NodePendingCredential = bson::from_document(doc).expect("deserialize");
        assert_eq!(cred.id, restored.id);
        assert_eq!(cred.service_slug, restored.service_slug);
        assert_eq!(restored.injection_method, InjectionMethod::Header);
        assert!(restored.consumed_at.is_none());
    }

    #[test]
    fn bson_roundtrip_with_consumed_and_declined() {
        let cred = NodePendingCredential {
            id: "id".to_string(),
            node_id: "n".to_string(),
            service_slug: "s".to_string(),
            injection_method: InjectionMethod::PathPrefix,
            field_name: "bot".to_string(),
            target_url: None,
            label: None,
            created_by_user_id: "u".to_string(),
            owner_user_id: "u".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now(),
            consumed_at: Some(Utc::now()),
            declined_at: Some(Utc::now()),
            is_active: false,
        };
        let doc = bson::to_document(&cred).expect("serialize");
        let restored: NodePendingCredential = bson::from_document(doc).expect("deserialize");
        assert!(restored.consumed_at.is_some());
        assert!(restored.declined_at.is_some());
        assert!(!restored.is_active);
    }
}
