pub mod lago_client;
pub mod meter;
pub mod owner_resolver;
pub mod reconcile;
pub mod reservation;
pub mod route_context;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::db::DbHandle;
use crate::errors::AppResult;
use lago_client::{LagoApi, LagoClient};

pub use meter::MeteredProxyContext;
pub use owner_resolver::BillingOwnerResolver;
pub use route_context::{BillingRouteContext, NodeIntent};

#[derive(Clone)]
pub struct BillingService {
    db: DbHandle,
    config: Arc<AppConfig>,
    owner_resolver: BillingOwnerResolver,
    lago: Option<Arc<dyn LagoApi>>,
}

impl BillingService {
    pub fn new(db: DbHandle, config: Arc<AppConfig>) -> Self {
        let lago = match (&config.lago_api_url, &config.lago_api_key) {
            (Some(url), Some(key)) => match LagoClient::new(url.clone(), key.clone()) {
                Ok(client) => Some(Arc::new(client) as Arc<dyn LagoApi>),
                Err(error) => {
                    tracing::warn!(error = %error, "Lago billing client is not configured");
                    None
                }
            },
            _ => None,
        };

        Self {
            db: db.clone(),
            config,
            owner_resolver: BillingOwnerResolver::new(db),
            lago,
        }
    }

    pub fn owner_resolver(&self) -> &BillingOwnerResolver {
        &self.owner_resolver
    }

    pub fn billing_enabled(&self) -> bool {
        self.config.billing_enabled
    }

    pub fn lago_configured(&self) -> bool {
        self.lago.is_some()
    }

    pub fn lago_client(&self) -> Option<Arc<dyn LagoApi>> {
        self.lago.clone()
    }

    pub fn reconciler(&self) -> reconcile::BillingReconciler {
        reconcile::BillingReconciler::new(self.db.clone(), self.lago.clone(), self.config.clone())
    }

    pub async fn open(&self, ctx: &BillingRouteContext) -> AppResult<MeteredProxyContext> {
        let reservation = if self.config.billing_enabled {
            reservation::gate_and_reserve(
                &self.db,
                self.lago.as_deref(),
                ctx,
                self.config.billing_fail_closed,
            )
            .await?
        } else {
            None
        };
        meter::open(&self.db, ctx, reservation.as_ref()).await
    }

    pub async fn mark_forwarded(&self, metered: &MeteredProxyContext) -> AppResult<()> {
        meter::mark_forwarded(&self.db, metered).await
    }

    pub async fn settle(
        &self,
        metered: &MeteredProxyContext,
        platform: crate::models::service_billing::PlatformUsage,
        resale: Option<crate::models::service_billing::ResaleUsage>,
        model: Option<String>,
    ) -> AppResult<()> {
        meter::settle(&self.db, metered, platform, resale, model).await
    }

    pub async fn fail(&self, metered: &MeteredProxyContext, reason: &str) -> AppResult<()> {
        meter::fail(&self.db, metered, reason).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use mongodb::bson::doc;
    use uuid::Uuid;

    use crate::models::billing_wallet::{BillingWallet, CollectionState, PlanKind};
    use crate::models::service_billing::{BillingMetric, ServiceBilling};
    use crate::models::usage_meter::{CredentialClass, UsageMeterRow};
    use crate::services::billing::{BillingRouteContext, BillingService, NodeIntent};
    use crate::test_utils::{connect_test_database, test_app_config};

    #[tokio::test]
    async fn billing_disabled_keeps_metering_dark_for_wallet_charges() {
        let Some(db) = connect_test_database("billing_disabled_no_charge").await else {
            return;
        };
        let owner_id = "owner-dark-billing";
        insert_wallet(&db, owner_id).await;
        let service = BillingService::new(db.clone(), std::sync::Arc::new(test_app_config()));
        let billing = ServiceBilling {
            resale_billable: true,
            resale_metric: BillingMetric::Requests,
            lago_resale_metric_code: Some("resale_requests".to_string()),
        };
        let ctx = BillingRouteContext::new(
            Uuid::new_v4().to_string(),
            owner_id.to_string(),
            "actor-1".to_string(),
            None,
            Some("user-service-1".to_string()),
            Some("catalog-1".to_string()),
            Some("service-one".to_string()),
            NodeIntent::Direct,
            "bearer".to_string(),
            CredentialClass::NyxidManagedMaster,
            BillingMetric::Requests,
            Some(&billing),
            false,
        );

        let metered = service.open(&ctx).await.expect("open metering");
        assert!(metered.is_enabled());
        let wallet = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");
        let row = db
            .collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .find_one(doc! { "billing_owner_id": owner_id })
            .await
            .expect("find usage row")
            .expect("row exists");

        assert_eq!(wallet.reserved_credits, 0);
        assert_eq!(wallet.pending_lago_debits, 0);
        assert_eq!(row.reserved_credits, 0);
        assert!(row.wallet_id.is_none());
    }

    async fn insert_wallet(db: &mongodb::Database, owner_id: &str) {
        let now = Utc::now();
        db.collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .insert_one(BillingWallet {
                id: format!("wallet-{owner_id}"),
                owner_id: owner_id.to_string(),
                lago_customer_id: owner_id.to_string(),
                lago_subscription_id: Some(format!("{owner_id}:plan")),
                plan_kind: PlanKind::Prepaid,
                balance_credits: 100,
                reserved_credits: 0,
                pending_lago_debits: 0,
                has_payment_instrument: false,
                overdraft_cap_credits: 0,
                suspended: false,
                collection_state: CollectionState::Good,
                balance_synced_at: now,
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("insert wallet");
    }
}
