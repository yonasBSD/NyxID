use chrono::Utc;
use mongodb::bson::{self, doc};
use uuid::Uuid;

use crate::errors::AppResult;
use crate::models::service_billing::{BillingMetric, PlatformUsage, ResaleUsage};
use crate::models::usage_meter::{
    BillingLayer, COLLECTION_NAME as USAGE_METER, UsageMeterRow, UsageStatus,
};

use super::route_context::BillingRouteContext;

pub const PLATFORM_REQUESTS_METRIC_CODE: &str = "platform_requests";
pub const PLATFORM_BYTES_METRIC_CODE: &str = "platform_bytes";

#[derive(Clone, Debug, Default)]
pub struct MeteredProxyContext {
    pub route: Option<BillingRouteContext>,
}

impl MeteredProxyContext {
    pub fn disabled() -> Self {
        Self { route: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.route.is_some()
    }
}

pub async fn open(
    db: &mongodb::Database,
    ctx: &BillingRouteContext,
) -> AppResult<MeteredProxyContext> {
    if !ctx.is_metered() {
        return Ok(MeteredProxyContext::disabled());
    }

    if ctx.platform_enabled {
        insert_reserved_row(
            db,
            ctx,
            BillingLayer::Platform,
            ctx.platform_metric,
            platform_metric_code(ctx.platform_metric).to_string(),
            None,
        )
        .await?;
    }

    if let Some(resale) = &ctx.resale {
        insert_reserved_row(
            db,
            ctx,
            BillingLayer::Resale,
            resale.metric,
            resale.lago_metric_code.clone(),
            None,
        )
        .await?;
    }

    Ok(MeteredProxyContext {
        route: Some(ctx.clone()),
    })
}

pub async fn mark_forwarded(
    db: &mongodb::Database,
    metered: &MeteredProxyContext,
) -> AppResult<()> {
    let Some(ctx) = &metered.route else {
        return Ok(());
    };

    db.collection::<UsageMeterRow>(USAGE_METER)
        .update_many(
            doc! {
                "billing_request_id": &ctx.billing_request_id,
                "status": "reserved",
            },
            doc! {
                "$set": {
                    "status": "forwarded",
                    "forwarded": true,
                    "updated_at": bson::DateTime::from_chrono(Utc::now()),
                }
            },
        )
        .await?;
    Ok(())
}

pub async fn settle(
    db: &mongodb::Database,
    metered: &MeteredProxyContext,
    platform: PlatformUsage,
    resale: Option<ResaleUsage>,
    model: Option<String>,
) -> AppResult<()> {
    let Some(ctx) = &metered.route else {
        return Ok(());
    };

    if ctx.platform_enabled {
        finalize_layer(
            db,
            ctx,
            BillingLayer::Platform,
            platform_quantity(ctx.platform_metric, &platform),
            model.clone(),
        )
        .await?;
    }

    if let Some(resale_usage) = resale
        && ctx.resale.is_some()
    {
        finalize_layer(
            db,
            ctx,
            BillingLayer::Resale,
            resale_usage.quantity.max(0),
            model,
        )
        .await?;
    }

    Ok(())
}

pub async fn fail(
    db: &mongodb::Database,
    metered: &MeteredProxyContext,
    reason: &str,
) -> AppResult<()> {
    let Some(ctx) = &metered.route else {
        return Ok(());
    };

    db.collection::<UsageMeterRow>(USAGE_METER)
        .update_many(
            doc! {
                "billing_request_id": &ctx.billing_request_id,
                "forwarded": false,
                "status": "reserved",
            },
            doc! {
                "$set": {
                    "status": "failed",
                    "last_error": reason,
                    "updated_at": bson::DateTime::from_chrono(Utc::now()),
                    "finalized_at": bson::DateTime::from_chrono(Utc::now()),
                }
            },
        )
        .await?;
    Ok(())
}

async fn insert_reserved_row(
    db: &mongodb::Database,
    ctx: &BillingRouteContext,
    layer: BillingLayer,
    metric: BillingMetric,
    lago_metric_code: String,
    flush_seq: Option<i64>,
) -> AppResult<()> {
    let now = Utc::now();
    let transaction_id = transaction_id(&ctx.billing_request_id, layer, flush_seq);
    let row = UsageMeterRow {
        id: Uuid::new_v4().to_string(),
        transaction_id,
        billing_request_id: ctx.billing_request_id.clone(),
        layer,
        flush_seq,
        billing_owner_id: ctx.billing_owner_id.clone(),
        wallet_id: None,
        actor_user_id: ctx.actor_user_id.clone(),
        api_key_id: ctx.api_key_id.clone(),
        service_id: ctx
            .catalog_service_id
            .clone()
            .or_else(|| ctx.user_service_id.clone()),
        service_slug: ctx.service_slug.clone(),
        metric,
        lago_metric_code,
        credential_class: ctx.credential_class,
        model: None,
        reserved_credits: 0,
        quantity: None,
        status: UsageStatus::Reserved,
        forwarded: false,
        released: false,
        lago_acked: false,
        attempt: 0,
        created_at: now,
        updated_at: now,
        finalized_at: None,
        expires_at: None,
        last_error: None,
    };

    db.collection::<UsageMeterRow>(USAGE_METER)
        .insert_one(&row)
        .await
        .map(|_| ())
        .or_else(|error| {
            if is_duplicate_key_error(&error) {
                Ok(())
            } else {
                Err(error)
            }
        })?;
    Ok(())
}

async fn finalize_layer(
    db: &mongodb::Database,
    ctx: &BillingRouteContext,
    layer: BillingLayer,
    quantity: i64,
    model: Option<String>,
) -> AppResult<()> {
    let now = Utc::now();
    db.collection::<UsageMeterRow>(USAGE_METER)
        .update_one(
            doc! {
                "billing_request_id": &ctx.billing_request_id,
                "layer": layer.as_transaction_suffix(),
                "status": { "$in": ["reserved", "forwarded"] },
            },
            doc! {
                "$set": {
                    "status": "finalized",
                    "forwarded": true,
                    "quantity": quantity,
                    "released": true,
                    "model": model,
                    "updated_at": bson::DateTime::from_chrono(now),
                    "finalized_at": bson::DateTime::from_chrono(now),
                }
            },
        )
        .await?;
    Ok(())
}

fn transaction_id(billing_request_id: &str, layer: BillingLayer, flush_seq: Option<i64>) -> String {
    match flush_seq {
        Some(seq) => format!(
            "{}:{}:{}",
            billing_request_id,
            layer.as_transaction_suffix(),
            seq
        ),
        None => format!("{}:{}", billing_request_id, layer.as_transaction_suffix()),
    }
}

fn platform_metric_code(metric: BillingMetric) -> &'static str {
    match metric {
        BillingMetric::Requests | BillingMetric::Tokens => PLATFORM_REQUESTS_METRIC_CODE,
        BillingMetric::Bytes => PLATFORM_BYTES_METRIC_CODE,
    }
}

fn platform_quantity(metric: BillingMetric, usage: &PlatformUsage) -> i64 {
    match metric {
        BillingMetric::Bytes => usage.bytes.max(0),
        BillingMetric::Requests | BillingMetric::Tokens => usage.requests.max(0),
    }
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;
    use mongodb::bson::doc;
    use mongodb::options::IndexOptions;

    use crate::models::service_billing::{BillingMetric, PlatformUsage, ServiceBilling};
    use crate::models::usage_meter::{BillingLayer, CredentialClass, UsageMeterRow, UsageStatus};
    use crate::services::billing::meter::{mark_forwarded, open, settle};
    use crate::services::billing::route_context::{BillingRouteContext, NodeIntent};
    use crate::test_utils::connect_test_database;

    #[tokio::test]
    async fn ledger_open_mark_and_settle_are_durable_and_idempotent() {
        let Some(db) = connect_test_database("usage_meter_ledger").await else {
            return;
        };
        db.collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! { "transaction_id": 1 })
                    .options(IndexOptions::builder().unique(true).build())
                    .build(),
            )
            .await
            .expect("create transaction id index");

        let billing = ServiceBilling {
            resale_billable: true,
            resale_metric: BillingMetric::Tokens,
            lago_resale_metric_code: Some("resale_tokens".to_string()),
        };
        let ctx = BillingRouteContext::new(
            "billing-request-1".to_string(),
            "owner-1".to_string(),
            "actor-1".to_string(),
            Some("api-key-1".to_string()),
            Some("user-service-1".to_string()),
            Some("catalog-1".to_string()),
            Some("llm-test".to_string()),
            NodeIntent::Direct,
            "bearer".to_string(),
            CredentialClass::NyxidManagedMaster,
            BillingMetric::Bytes,
            Some(&billing),
            true,
        );

        let metered = open(&db, &ctx).await.expect("open meter");
        open(&db, &ctx).await.expect("idempotent open");
        mark_forwarded(&db, &metered).await.expect("mark forwarded");
        settle(
            &db,
            &metered,
            PlatformUsage::single_request(42),
            Some(crate::models::service_billing::ResaleUsage {
                metric: BillingMetric::Tokens,
                quantity: 17,
            }),
            Some("test-model".to_string()),
        )
        .await
        .expect("settle");

        let rows: Vec<UsageMeterRow> = db
            .collection(crate::models::usage_meter::COLLECTION_NAME)
            .find(doc! { "billing_request_id": "billing-request-1" })
            .await
            .expect("find rows")
            .try_collect()
            .await
            .expect("collect rows");

        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| {
            row.layer == BillingLayer::Platform
                && row.transaction_id == "billing-request-1:platform"
                && row.metric == BillingMetric::Bytes
                && row.quantity == Some(42)
                && row.status == UsageStatus::Finalized
                && row.forwarded
        }));
        assert!(rows.iter().any(|row| {
            row.layer == BillingLayer::Resale
                && row.transaction_id == "billing-request-1:resale"
                && row.metric == BillingMetric::Tokens
                && row.quantity == Some(17)
                && row.credential_class == CredentialClass::NyxidManagedMaster
        }));
    }

    #[test]
    fn transaction_id_is_per_layer_and_flush() {
        assert_eq!(
            super::transaction_id("req", BillingLayer::Platform, None),
            "req:platform"
        );
        assert_eq!(
            super::transaction_id("req", BillingLayer::Resale, None),
            "req:resale"
        );
        assert_eq!(
            super::transaction_id("req", BillingLayer::Platform, Some(7)),
            "req:platform:7"
        );
    }
}

fn is_duplicate_key_error(error: &mongodb::error::Error) -> bool {
    error
        .to_string()
        .to_ascii_lowercase()
        .contains("duplicate key")
}
