use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const COLLECTION_NAME: &str = "service_pools";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoolStrategy {
    #[default]
    RoundRobin,
    Weighted,
}

impl PoolStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::Weighted => "weighted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "round_robin" => Some(Self::RoundRobin),
            "weighted" => Some(Self::Weighted),
            _ => None,
        }
    }
}

fn default_member_weight() -> u32 {
    1
}

fn default_member_enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePoolMember {
    pub user_service_id: String,
    #[serde(default = "default_member_weight")]
    pub weight: u32,
    #[serde(default = "default_member_enabled")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServicePool {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub strategy: PoolStrategy,
    #[serde(default)]
    pub members: Vec<ServicePoolMember>,
    #[serde(default)]
    pub rr_counter: i64,
    pub is_active: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "service_pools");
    }

    #[test]
    fn strategy_strings_roundtrip() {
        assert_eq!(PoolStrategy::RoundRobin.as_str(), "round_robin");
        assert_eq!(PoolStrategy::Weighted.as_str(), "weighted");
        assert_eq!(
            PoolStrategy::parse("round_robin"),
            Some(PoolStrategy::RoundRobin)
        );
        assert_eq!(
            PoolStrategy::parse("weighted"),
            Some(PoolStrategy::Weighted)
        );
        assert_eq!(PoolStrategy::parse("least_inflight"), None);
    }

    #[test]
    fn bson_roundtrip() {
        let pool = ServicePool {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: uuid::Uuid::new_v4().to_string(),
            slug: "llm-pool".to_string(),
            name: "LLM Pool".to_string(),
            description: Some("Two interchangeable endpoints".to_string()),
            strategy: PoolStrategy::Weighted,
            members: vec![
                ServicePoolMember {
                    user_service_id: uuid::Uuid::new_v4().to_string(),
                    weight: 2,
                    enabled: true,
                },
                ServicePoolMember {
                    user_service_id: uuid::Uuid::new_v4().to_string(),
                    weight: 1,
                    enabled: false,
                },
            ],
            rr_counter: 42,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc = bson::to_document(&pool).expect("serialize");
        let restored: ServicePool = bson::from_document(doc).expect("deserialize");
        assert_eq!(pool.id, restored.id);
        assert_eq!(pool.user_id, restored.user_id);
        assert_eq!(pool.slug, restored.slug);
        assert_eq!(pool.strategy, restored.strategy);
        assert_eq!(pool.members, restored.members);
        assert_eq!(pool.rr_counter, restored.rr_counter);
        assert!(restored.is_active);
    }

    #[test]
    fn bson_serializes_none_description() {
        let pool = ServicePool {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: uuid::Uuid::new_v4().to_string(),
            slug: "null-description-pool".to_string(),
            name: "Null Description Pool".to_string(),
            description: None,
            strategy: PoolStrategy::RoundRobin,
            members: vec![],
            rr_counter: 0,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc = bson::to_document(&pool).expect("serialize");
        assert_eq!(doc.get("description"), Some(&bson::Bson::Null));
    }

    #[test]
    fn bson_defaults() {
        let mut doc = bson::doc! {
            "_id": "pool-id",
            "user_id": "user-id",
            "slug": "default-pool",
            "name": "Default Pool",
            "members": [
                { "user_service_id": "svc-1" }
            ],
            "is_active": true,
            "created_at": bson::DateTime::from_chrono(Utc::now()),
            "updated_at": bson::DateTime::from_chrono(Utc::now()),
        };
        doc.remove("strategy");
        doc.remove("rr_counter");

        let restored: ServicePool = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.strategy, PoolStrategy::RoundRobin);
        assert_eq!(restored.rr_counter, 0);
        assert_eq!(restored.members[0].weight, 1);
        assert!(restored.members[0].enabled);
    }
}
