pub mod lago_client;
pub mod meter;
pub mod owner_resolver;
pub mod reconcile;
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
        meter::open(&self.db, ctx).await
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
