use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;

pub const COLLECTION_NAME: &str = "org_memberships";

/// Role of a member within an org. Controls what they can do with the
/// org's shared resources via the proxy and management APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    /// Manages org metadata, members, services. Can write all org-owned resources.
    Admin,
    /// Can use org services through the proxy. Cannot manage org resources.
    Member,
    /// Can see org services exist but cannot proxy through them.
    Viewer,
}

impl OrgRole {
    pub fn can_proxy(&self) -> bool {
        matches!(self, Self::Admin | Self::Member)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgMembership {
    #[serde(rename = "_id")]
    pub id: String,
    /// User where `user_type = Org`. The owner of all org-shared resources.
    pub org_user_id: String,
    /// The person user who is a member of the org.
    pub member_user_id: String,
    pub role: OrgRole,
    /// Optional scope: when set, this member can only access these
    /// `UserService` ids inside the org. `None` means access to all org
    /// services (subject to role).
    #[serde(default)]
    pub allowed_service_ids: Option<Vec<String>>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    /// Soft-delete timestamp. Active memberships have `revoked_at = None`.
    #[serde(default, with = "bson_datetime::optional")]
    pub revoked_at: Option<DateTime<Utc>>,
}

impl OrgMembership {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Whether this member is allowed to access the given user_service id
    /// based on their `allowed_service_ids` scope. Does not check role.
    pub fn allows_service(&self, user_service_id: &str) -> bool {
        match &self.allowed_service_ids {
            None => true,
            Some(ids) => ids.iter().any(|id| id == user_service_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_membership() -> OrgMembership {
        OrgMembership {
            id: uuid::Uuid::new_v4().to_string(),
            org_user_id: uuid::Uuid::new_v4().to_string(),
            member_user_id: uuid::Uuid::new_v4().to_string(),
            role: OrgRole::Member,
            allowed_service_ids: None,
            created_at: Utc::now(),
            revoked_at: None,
        }
    }

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "org_memberships");
    }

    #[test]
    fn role_capabilities() {
        assert!(OrgRole::Admin.can_proxy());
        assert!(OrgRole::Admin.can_admin());
        assert!(OrgRole::Member.can_proxy());
        assert!(!OrgRole::Member.can_admin());
        assert!(!OrgRole::Viewer.can_proxy());
        assert!(!OrgRole::Viewer.can_admin());
    }

    #[test]
    fn role_serializes_snake_case() {
        let admin = bson::to_bson(&OrgRole::Admin).expect("ser admin");
        let member = bson::to_bson(&OrgRole::Member).expect("ser member");
        let viewer = bson::to_bson(&OrgRole::Viewer).expect("ser viewer");
        assert_eq!(admin.as_str().unwrap(), "admin");
        assert_eq!(member.as_str().unwrap(), "member");
        assert_eq!(viewer.as_str().unwrap(), "viewer");
    }

    #[test]
    fn bson_roundtrip_active() {
        let m = make_membership();
        let doc = bson::to_document(&m).expect("serialize");
        let restored: OrgMembership = bson::from_document(doc).expect("deserialize");
        assert_eq!(m.id, restored.id);
        assert_eq!(m.org_user_id, restored.org_user_id);
        assert_eq!(m.member_user_id, restored.member_user_id);
        assert!(restored.is_active());
    }

    #[test]
    fn bson_roundtrip_revoked() {
        let mut m = make_membership();
        m.revoked_at = Some(Utc::now());
        let doc = bson::to_document(&m).expect("serialize");
        let restored: OrgMembership = bson::from_document(doc).expect("deserialize");
        assert!(!restored.is_active());
    }

    #[test]
    fn allows_service_with_no_scope() {
        let m = make_membership();
        assert!(m.allows_service("any-service-id"));
    }

    #[test]
    fn allows_service_with_scope() {
        let mut m = make_membership();
        m.allowed_service_ids = Some(vec!["svc-1".to_string(), "svc-2".to_string()]);
        assert!(m.allows_service("svc-1"));
        assert!(m.allows_service("svc-2"));
        assert!(!m.allows_service("svc-3"));
    }

    #[test]
    fn allows_service_with_empty_scope_blocks_all() {
        let mut m = make_membership();
        m.allowed_service_ids = Some(vec![]);
        assert!(!m.allows_service("svc-1"));
    }
}
