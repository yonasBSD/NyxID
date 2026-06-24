use chrono::Utc;
use mongodb::bson::{self, doc};
use uuid::Uuid;

use crate::errors::AppResult;
use crate::models::service_billing::{BillingMetric, PlatformUsage, ResaleUsage};
use crate::models::usage_meter::{
    BillingLayer, COLLECTION_NAME as USAGE_METER, UsageMeterRow, UsageStatus,
};

use super::reservation::{self, BillingReservation};
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
    reservation: Option<&BillingReservation>,
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
            reservation,
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
            reservation,
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

    reservation::release_unforwarded_rows(
        db,
        &ctx.billing_request_id,
        UsageStatus::Failed,
        Some(reason),
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
    reservation: Option<&BillingReservation>,
    flush_seq: Option<i64>,
) -> AppResult<()> {
    let now = Utc::now();
    let transaction_id = transaction_id(&ctx.billing_request_id, layer, flush_seq);
    let wallet_id = reservation.map(|reservation| reservation.wallet_id.clone());
    let reserved_credits = reservation
        .map(|reservation| reservation.reserved_for(layer))
        .unwrap_or(0);
    let row = UsageMeterRow {
        id: Uuid::new_v4().to_string(),
        transaction_id,
        billing_request_id: ctx.billing_request_id.clone(),
        layer,
        flush_seq,
        billing_owner_id: ctx.billing_owner_id.clone(),
        wallet_id,
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
        reserved_credits,
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
    let model_for_row = model.clone();
    let collection = db.collection::<UsageMeterRow>(USAGE_METER);
    let claimed = collection
        .find_one_and_update(
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
                    "released": false,
                    "model": model_for_row,
                    "updated_at": bson::DateTime::from_chrono(now),
                    "finalized_at": bson::DateTime::from_chrono(now),
                }
            },
        )
        .with_options(
            mongodb::options::FindOneAndUpdateOptions::builder()
                .return_document(mongodb::options::ReturnDocument::After)
                .build(),
        )
        .await?;

    let Some(claimed) = claimed else {
        return Ok(());
    };

    let actual_credits =
        reservation::actual_credits_for_row(db, &claimed, quantity, model.as_deref()).await?;
    reservation::apply_settlement_for_row(db, &claimed, actual_credits).await?;
    Ok(())
}

pub(crate) fn transaction_id(
    billing_request_id: &str,
    layer: BillingLayer,
    flush_seq: Option<i64>,
) -> String {
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

pub(crate) fn platform_metric_code(metric: BillingMetric) -> &'static str {
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
    use chrono::Utc;
    use futures::TryStreamExt;
    use mongodb::bson::doc;
    use mongodb::options::IndexOptions;
    use uuid::Uuid;

    use crate::models::billing_rate_cache::BillingRateCache;
    use crate::models::billing_wallet::{BillingWallet, CollectionState, PlanKind};
    use crate::models::service_billing::{BillingMetric, PlatformUsage, ServiceBilling};
    use crate::models::usage_meter::{BillingLayer, CredentialClass, UsageMeterRow, UsageStatus};
    use crate::services::billing::meter::{mark_forwarded, open, settle};
    use crate::services::billing::reservation::BillingReservation;
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

        let metered = open(&db, &ctx, None).await.expect("open meter");
        open(&db, &ctx, None).await.expect("idempotent open");
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

    #[tokio::test]
    async fn settle_moves_wallet_once_and_blocks_double_spend_before_lago_sync() {
        let Some(db) = connect_test_database("billing_settle_wallet_once").await else {
            return;
        };
        create_usage_transaction_index(&db).await;
        insert_rate(&db, "platform_requests", 5).await;
        let owner_id = "owner-wallet-settle";
        insert_wallet(&db, owner_id, 10, 5).await;

        let ctx = platform_context("billing-wallet-1", owner_id);
        let reservation = BillingReservation {
            owner_id: owner_id.to_string(),
            wallet_id: "wallet-owner-wallet-settle".to_string(),
            total_reserved_credits: 5,
            layers: vec![crate::services::billing::reservation::LayerReservation {
                layer: BillingLayer::Platform,
                reserved_credits: 5,
            }],
        };
        crate::services::billing::reservation::try_reserve_prepaid(&db, owner_id, 5)
            .await
            .expect("reserve")
            .expect("reserved");

        let metered = open(&db, &ctx, Some(&reservation)).await.expect("open");
        mark_forwarded(&db, &metered).await.expect("mark forwarded");
        settle(&db, &metered, PlatformUsage::single_request(1), None, None)
            .await
            .expect("settle first time");
        settle(&db, &metered, PlatformUsage::single_request(1), None, None)
            .await
            .expect("settle replay");

        let wallet = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");
        assert_eq!(wallet.reserved_credits, 0);
        assert_eq!(wallet.pending_lago_debits, 5);
        assert_eq!(wallet.available_credits(), 5);

        let second_reservation =
            crate::services::billing::reservation::try_reserve_prepaid(&db, owner_id, 6)
                .await
                .expect("second reserve query");
        assert!(
            second_reservation.is_none(),
            "pending_lago_debits must reduce availability before Lago sync"
        );
    }

    #[tokio::test]
    async fn recovery_after_settle_debit_gap_does_not_debit_wallet_twice() {
        let Some(db) = connect_test_database("billing_settle_recovery_overlap").await else {
            return;
        };
        create_usage_transaction_index(&db).await;
        insert_rate(&db, "platform_requests", 5).await;
        let owner_id = "owner-wallet-recovery";
        insert_wallet(&db, owner_id, 10, 0).await;

        let ctx = platform_context("billing-wallet-recovery", owner_id);
        let reservation = BillingReservation {
            owner_id: owner_id.to_string(),
            wallet_id: "wallet-owner-wallet-recovery".to_string(),
            total_reserved_credits: 5,
            layers: vec![crate::services::billing::reservation::LayerReservation {
                layer: BillingLayer::Platform,
                reserved_credits: 5,
            }],
        };
        crate::services::billing::reservation::try_reserve_prepaid(&db, owner_id, 5)
            .await
            .expect("reserve")
            .expect("reserved");

        let metered = open(&db, &ctx, Some(&reservation)).await.expect("open");
        mark_forwarded(&db, &metered).await.expect("mark forwarded");
        settle(&db, &metered, PlatformUsage::single_request(1), None, None)
            .await
            .expect("settle");

        let settled_row = db
            .collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .find_one(doc! { "billing_request_id": "billing-wallet-recovery" })
            .await
            .expect("find settled row")
            .expect("row exists");
        db.collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .update_one(
                doc! { "_id": &settled_row.id },
                doc! {
                    "$set": {
                        "released": false,
                        "updated_at": mongodb::bson::DateTime::from_chrono(Utc::now()),
                    }
                },
            )
            .await
            .expect("simulate missing release marker after wallet debit");

        let recovered = crate::services::billing::reservation::recover_unreleased_finalized(&db)
            .await
            .expect("recover unreleased");
        assert_eq!(recovered, 1);

        let wallet = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");
        let row = db
            .collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .find_one(doc! { "billing_request_id": "billing-wallet-recovery" })
            .await
            .expect("find row")
            .expect("row exists");

        assert_eq!(wallet.reserved_credits, 0);
        assert_eq!(wallet.pending_lago_debits, 5);
        assert_eq!(
            wallet.settled_usage_row_ids,
            vec![settled_row.id],
            "the wallet idempotency key must be written once"
        );
        assert!(row.released);
    }

    #[tokio::test]
    async fn fail_releases_only_never_forwarded_wallet_hold() {
        let Some(db) = connect_test_database("billing_fail_releases_hold").await else {
            return;
        };
        create_usage_transaction_index(&db).await;
        let owner_id = "owner-fail-release";
        insert_wallet(&db, owner_id, 10, 0).await;

        let ctx = platform_context("billing-wallet-fail", owner_id);
        let reservation = BillingReservation {
            owner_id: owner_id.to_string(),
            wallet_id: "wallet-owner-fail-release".to_string(),
            total_reserved_credits: 4,
            layers: vec![crate::services::billing::reservation::LayerReservation {
                layer: BillingLayer::Platform,
                reserved_credits: 4,
            }],
        };
        crate::services::billing::reservation::try_reserve_prepaid(&db, owner_id, 4)
            .await
            .expect("reserve")
            .expect("reserved");

        let metered = open(&db, &ctx, Some(&reservation)).await.expect("open");
        super::fail(&db, &metered, "before send")
            .await
            .expect("fail");

        let wallet = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");
        let row = db
            .collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .find_one(doc! { "billing_request_id": "billing-wallet-fail" })
            .await
            .expect("find row")
            .expect("row exists");

        assert_eq!(wallet.reserved_credits, 0);
        assert_eq!(wallet.pending_lago_debits, 0);
        assert_eq!(row.status, UsageStatus::Failed);
        assert!(row.released);
    }

    async fn create_usage_transaction_index(db: &mongodb::Database) {
        db.collection::<UsageMeterRow>(crate::models::usage_meter::COLLECTION_NAME)
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! { "transaction_id": 1 })
                    .options(IndexOptions::builder().unique(true).build())
                    .build(),
            )
            .await
            .expect("create transaction id index");
    }

    async fn insert_rate(db: &mongodb::Database, metric_code: &str, credits: i64) {
        db.collection::<BillingRateCache>(crate::models::billing_rate_cache::COLLECTION_NAME)
            .insert_one(BillingRateCache {
                id: BillingRateCache::cache_id(metric_code, None),
                lago_metric_code: metric_code.to_string(),
                model: None,
                credits_per_unit_micros: credits * 1_000_000,
                synced_at: Utc::now(),
            })
            .await
            .expect("insert rate");
    }

    async fn insert_wallet(
        db: &mongodb::Database,
        owner_id: &str,
        balance_credits: i64,
        overdraft_cap_credits: i64,
    ) {
        let now = Utc::now();
        db.collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .insert_one(BillingWallet {
                id: format!("wallet-{owner_id}"),
                owner_id: owner_id.to_string(),
                lago_customer_id: owner_id.to_string(),
                lago_subscription_id: Some(format!("{owner_id}:plan")),
                plan_kind: PlanKind::Prepaid,
                balance_credits,
                reserved_credits: 0,
                pending_lago_debits: 0,
                settled_usage_row_ids: Vec::new(),
                has_payment_instrument: false,
                overdraft_cap_credits,
                suspended: false,
                collection_state: CollectionState::Good,
                balance_synced_at: now,
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("insert wallet");
    }

    fn platform_context(request_id: &str, owner_id: &str) -> BillingRouteContext {
        BillingRouteContext::new(
            request_id.to_string(),
            owner_id.to_string(),
            "actor-1".to_string(),
            Some(Uuid::new_v4().to_string()),
            Some("user-service-1".to_string()),
            Some("catalog-1".to_string()),
            Some("service-one".to_string()),
            NodeIntent::Direct,
            "bearer".to_string(),
            CredentialClass::UserOwned,
            BillingMetric::Requests,
            None::<&ServiceBilling>,
            true,
        )
    }
}

fn is_duplicate_key_error(error: &mongodb::error::Error) -> bool {
    error
        .to_string()
        .to_ascii_lowercase()
        .contains("duplicate key")
}
