pub mod meter;
pub mod owner_resolver;
pub mod route_context;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::db::DbHandle;
use crate::errors::AppResult;

pub use meter::MeteredProxyContext;
pub use owner_resolver::BillingOwnerResolver;
pub use route_context::{BillingRouteContext, NodeIntent};

#[derive(Clone)]
pub struct BillingService {
    db: DbHandle,
    config: Arc<AppConfig>,
    owner_resolver: BillingOwnerResolver,
}

impl BillingService {
    pub fn new(db: DbHandle, config: Arc<AppConfig>) -> Self {
        Self {
            db: db.clone(),
            config,
            owner_resolver: BillingOwnerResolver::new(db),
        }
    }

    pub fn owner_resolver(&self) -> &BillingOwnerResolver {
        &self.owner_resolver
    }

    pub fn billing_enabled(&self) -> bool {
        self.config.billing_enabled
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
