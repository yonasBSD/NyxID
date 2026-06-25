use crate::db::DbHandle;
use crate::errors::{AppError, AppResult};
use crate::services::org_service::{self, OwnerAccess};

#[derive(Clone)]
pub struct BillingOwnerResolver {
    db: DbHandle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaysFrom {
    Personal,
    OrgWallet { org_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBillingOwner {
    pub owner_id: String,
    pub pays: PaysFrom,
}

impl BillingOwnerResolver {
    pub fn new(db: DbHandle) -> Self {
        Self { db }
    }

    pub async fn resolve(
        &self,
        actor_user_id: &str,
        effective_owner_id: Option<&str>,
    ) -> AppResult<ResolvedBillingOwner> {
        let owner_id = effective_owner_id.unwrap_or(actor_user_id);
        let access = org_service::resolve_owner_access(&self.db, actor_user_id, owner_id).await?;
        Self::from_owner_access(actor_user_id, owner_id, &access)
    }

    pub fn from_owner_access(
        actor_user_id: &str,
        owner_id: &str,
        access: &OwnerAccess,
    ) -> AppResult<ResolvedBillingOwner> {
        match access {
            OwnerAccess::Direct => Ok(ResolvedBillingOwner {
                owner_id: actor_user_id.to_string(),
                pays: PaysFrom::Personal,
            }),
            OwnerAccess::AsOrgAdmin { org_user_id, .. }
            | OwnerAccess::AsOrgMember { org_user_id, .. } => Ok(ResolvedBillingOwner {
                owner_id: org_user_id.clone(),
                pays: PaysFrom::OrgWallet {
                    org_id: org_user_id.clone(),
                },
            }),
            OwnerAccess::Forbidden => Err(AppError::Forbidden(format!(
                "User is not allowed to bill owner '{owner_id}'"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BillingOwnerResolver, PaysFrom};
    use crate::models::org_membership::OrgRole;
    use crate::services::org_service::OwnerAccess;

    #[test]
    fn direct_access_bills_personal_owner() {
        let resolved =
            BillingOwnerResolver::from_owner_access("actor", "actor", &OwnerAccess::Direct)
                .expect("direct access");

        assert_eq!(resolved.owner_id, "actor");
        assert_eq!(resolved.pays, PaysFrom::Personal);
    }

    #[test]
    fn org_access_bills_org_wallet() {
        let resolved = BillingOwnerResolver::from_owner_access(
            "member",
            "org",
            &OwnerAccess::AsOrgMember {
                org_user_id: "org".to_string(),
                membership_id: "membership".to_string(),
                role: OrgRole::Member,
                allowed_service_ids: None,
            },
        )
        .expect("org access");

        assert_eq!(resolved.owner_id, "org");
        assert_eq!(
            resolved.pays,
            PaysFrom::OrgWallet {
                org_id: "org".to_string()
            }
        );
    }

    #[test]
    fn forbidden_access_is_not_rewritten_to_personal_billing() {
        let err =
            BillingOwnerResolver::from_owner_access("actor", "other", &OwnerAccess::Forbidden)
                .expect_err("forbidden owner access must fail");

        assert!(matches!(err, crate::errors::AppError::Forbidden(_)));
    }
}
