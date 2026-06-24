use std::collections::BTreeSet;

use chrono::Utc;
use futures::TryStreamExt;
use mongodb::bson::{self, doc};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};

use crate::errors::{AppError, AppResult};
use crate::models::billing_rate_cache::{BillingRateCache, COLLECTION_NAME as BILLING_RATE_CACHE};
use crate::models::billing_wallet::{BillingWallet, COLLECTION_NAME as BILLING_WALLET, PlanKind};
use crate::models::usage_meter::{
    BillingLayer, COLLECTION_NAME as USAGE_METER, UsageMeterRow, UsageStatus,
};

use super::lago_client::{Entitlement, LagoApi};
use super::meter::platform_metric_code;
use super::route_context::BillingRouteContext;

const CREDIT_MICROS: i64 = 1_000_000;
const RECOVERY_BATCH_SIZE: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerReservation {
    pub layer: BillingLayer,
    pub reserved_credits: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BillingReservation {
    pub owner_id: String,
    pub wallet_id: String,
    pub total_reserved_credits: i64,
    pub layers: Vec<LayerReservation>,
}

impl BillingReservation {
    pub fn reserved_for(&self, layer: BillingLayer) -> i64 {
        self.layers
            .iter()
            .find(|reservation| reservation.layer == layer)
            .map(|reservation| reservation.reserved_credits)
            .unwrap_or(0)
    }
}

pub async fn gate_and_reserve(
    db: &mongodb::Database,
    lago: Option<&dyn LagoApi>,
    ctx: &BillingRouteContext,
    billing_fail_closed: bool,
) -> AppResult<Option<BillingReservation>> {
    if !ctx.is_metered() {
        return Ok(None);
    }
    if billing_fail_closed {
        return Err(AppError::BillingProviderUnavailable(
            "billing fail-closed override is enabled".to_string(),
        ));
    }

    let wallet = db
        .collection::<BillingWallet>(BILLING_WALLET)
        .find_one(doc! { "owner_id": &ctx.billing_owner_id })
        .await?
        .ok_or_else(|| {
            AppError::BillingNotConfigured(format!(
                "billing wallet is missing for owner {}",
                ctx.billing_owner_id
            ))
        })?;

    if wallet.is_suspended() {
        return Err(AppError::WalletSuspended);
    }
    let subscription_id = wallet.lago_subscription_id.as_deref().ok_or_else(|| {
        AppError::BillingNotConfigured(format!(
            "billing subscription is missing for owner {}",
            ctx.billing_owner_id
        ))
    })?;
    let lago = lago.ok_or_else(|| {
        AppError::BillingNotConfigured("Lago client is not configured".to_string())
    })?;
    let entitlements = lago.entitlements(subscription_id).await.map_err(|error| {
        tracing::warn!(
            owner_id = %ctx.billing_owner_id,
            subscription_id,
            error = %error,
            "Billing entitlement lookup failed closed"
        );
        AppError::PlanEntitlementRequired("billing entitlement could not be verified".to_string())
    })?;
    if !is_entitled(ctx, &entitlements) {
        return Err(AppError::PlanEntitlementRequired(
            "owner plan does not include this service".to_string(),
        ));
    }

    let layers = estimate_layer_reservations(db, ctx).await?;
    let total_reserved_credits = layers
        .iter()
        .map(|reservation| reservation.reserved_credits)
        .sum::<i64>();

    if total_reserved_credits == 0 {
        return Ok(Some(BillingReservation {
            owner_id: wallet.owner_id,
            wallet_id: wallet.id,
            total_reserved_credits,
            layers,
        }));
    }

    if wallet.available_with_overdraft_credits() <= 0 && wallet.plan_kind != PlanKind::Prepaid {
        suspend_wallet(db, &wallet.owner_id).await?;
        return Err(AppError::WalletSuspended);
    }
    if wallet.available_credits() <= 0 && wallet.plan_kind == PlanKind::Prepaid {
        return Err(AppError::InsufficientCredits);
    }

    if try_reserve_prepaid(db, &wallet.owner_id, total_reserved_credits)
        .await?
        .is_some()
    {
        return Ok(Some(BillingReservation {
            owner_id: wallet.owner_id,
            wallet_id: wallet.id,
            total_reserved_credits,
            layers,
        }));
    }

    if wallet.plan_kind == PlanKind::Prepaid {
        return Err(AppError::InsufficientCredits);
    }

    if wallet.has_payment_instrument
        && try_reserve_overdraft(db, &wallet.owner_id, total_reserved_credits)
            .await?
            .is_some()
    {
        return Ok(Some(BillingReservation {
            owner_id: wallet.owner_id,
            wallet_id: wallet.id,
            total_reserved_credits,
            layers,
        }));
    }

    if wallet.has_payment_instrument {
        suspend_wallet(db, &wallet.owner_id).await?;
        return Err(AppError::WalletSuspended);
    }

    Err(AppError::InsufficientCredits)
}

pub async fn try_reserve_prepaid(
    db: &mongodb::Database,
    owner_id: &str,
    credits: i64,
) -> AppResult<Option<BillingWallet>> {
    if credits <= 0 {
        return db
            .collection::<BillingWallet>(BILLING_WALLET)
            .find_one(doc! { "owner_id": owner_id, "suspended": false })
            .await
            .map_err(Into::into);
    }

    let now = Utc::now();
    db.collection::<BillingWallet>(BILLING_WALLET)
        .find_one_and_update(
            doc! {
                "owner_id": owner_id,
                "suspended": false,
                "$expr": {
                    "$gte": [
                        {
                            "$subtract": [
                                {
                                    "$subtract": [
                                        "$balance_credits",
                                        "$reserved_credits"
                                    ]
                                },
                                "$pending_lago_debits"
                            ]
                        },
                        credits
                    ]
                },
            },
            doc! {
                "$inc": { "reserved_credits": credits },
                "$set": { "updated_at": bson::DateTime::from_chrono(now) },
            },
        )
        .with_options(
            FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build(),
        )
        .await
        .map_err(Into::into)
}

pub async fn try_reserve_overdraft(
    db: &mongodb::Database,
    owner_id: &str,
    credits: i64,
) -> AppResult<Option<BillingWallet>> {
    if credits <= 0 {
        return try_reserve_prepaid(db, owner_id, credits).await;
    }

    let now = Utc::now();
    db.collection::<BillingWallet>(BILLING_WALLET)
        .find_one_and_update(
            doc! {
                "owner_id": owner_id,
                "suspended": false,
                "has_payment_instrument": true,
                "$expr": {
                    "$gte": [
                        {
                            "$subtract": [
                                {
                                    "$add": [
                                        "$balance_credits",
                                        "$overdraft_cap_credits"
                                    ]
                                },
                                {
                                    "$add": [
                                        "$reserved_credits",
                                        "$pending_lago_debits"
                                    ]
                                }
                            ]
                        },
                        credits
                    ]
                },
            },
            doc! {
                "$inc": { "reserved_credits": credits },
                "$set": { "updated_at": bson::DateTime::from_chrono(now) },
            },
        )
        .with_options(
            FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build(),
        )
        .await
        .map_err(Into::into)
}

pub async fn release_wallet_hold(
    db: &mongodb::Database,
    owner_id: &str,
    credits: i64,
) -> AppResult<()> {
    if credits <= 0 {
        return Ok(());
    }

    db.collection::<BillingWallet>(BILLING_WALLET)
        .update_one(
            doc! {
                "owner_id": owner_id,
                "$expr": { "$gte": [ "$reserved_credits", credits ] },
            },
            doc! {
                "$inc": { "reserved_credits": -credits },
                "$set": { "updated_at": bson::DateTime::from_chrono(Utc::now()) },
            },
        )
        .await?;
    Ok(())
}

pub async fn actual_credits_for_row(
    db: &mongodb::Database,
    row: &UsageMeterRow,
    quantity: i64,
    model: Option<&str>,
) -> AppResult<i64> {
    if row.wallet_id.is_none() {
        return Ok(0);
    }

    match estimate_credits(db, &row.lago_metric_code, model, quantity.max(0)).await {
        Ok(credits) => Ok(credits),
        Err(AppError::BillingNotConfigured(_)) => Ok(row.reserved_credits.max(0)),
        Err(error) => Err(error),
    }
}

pub async fn apply_settlement_for_row(
    db: &mongodb::Database,
    row: &UsageMeterRow,
    actual_credits: i64,
) -> AppResult<bool> {
    let usage_rows = db.collection::<UsageMeterRow>(USAGE_METER);
    if row.released
        || usage_rows
            .count_documents(doc! { "_id": &row.id, "released": false })
            .await?
            == 0
    {
        return Ok(false);
    }

    if row.wallet_id.is_some() {
        let reserved_credits = row.reserved_credits.max(0);
        let actual_credits = actual_credits.max(0);
        if reserved_credits > 0 || actual_credits > 0 {
            let wallet_id = row.wallet_id.as_deref().unwrap_or_default();
            let mut wallet_filter = doc! {
                "_id": wallet_id,
                "owner_id": &row.billing_owner_id,
            };
            wallet_filter.insert("settled_usage_row_ids", doc! { "$ne": &row.id });

            let wallets = db.collection::<BillingWallet>(BILLING_WALLET);
            // MongoDB transactions are not assumed available in local deployments, so
            // the wallet document carries the durable idempotency key for this debit.
            let wallet_update = wallets
                .update_one(
                    wallet_filter,
                    doc! {
                        "$inc": {
                            "reserved_credits": -reserved_credits,
                            "pending_lago_debits": actual_credits,
                        },
                        "$addToSet": { "settled_usage_row_ids": &row.id },
                        "$set": { "updated_at": bson::DateTime::from_chrono(Utc::now()) },
                    },
                )
                .await?;
            if wallet_update.matched_count == 0 {
                let already_settled = wallets
                    .count_documents(doc! {
                        "_id": wallet_id,
                        "owner_id": &row.billing_owner_id,
                        "settled_usage_row_ids": &row.id,
                    })
                    .await?
                    > 0;
                if !already_settled {
                    return Err(AppError::Internal(format!(
                        "billing wallet settlement guard missing for usage row {}",
                        row.id
                    )));
                }
            }
        }
    }

    let update = usage_rows
        .update_one(
            doc! { "_id": &row.id, "released": false },
            doc! {
                "$set": {
                    "released": true,
                    "updated_at": bson::DateTime::from_chrono(Utc::now()),
                }
            },
        )
        .await?;
    Ok(update.modified_count > 0)
}

pub async fn release_unforwarded_rows(
    db: &mongodb::Database,
    billing_request_id: &str,
    terminal_status: UsageStatus,
    reason: Option<&str>,
) -> AppResult<u64> {
    let mut cursor = db
        .collection::<UsageMeterRow>(USAGE_METER)
        .find(doc! {
            "billing_request_id": billing_request_id,
            "forwarded": false,
            "status": "reserved",
            "released": false,
        })
        .await?;
    let mut released_count = 0;
    while let Some(row) = cursor.try_next().await? {
        if release_one_unforwarded_row(db, &row, terminal_status, reason).await? {
            released_count += 1;
        }
    }
    Ok(released_count)
}

pub async fn abandon_stale_unforwarded(
    db: &mongodb::Database,
    cutoff: chrono::DateTime<Utc>,
) -> AppResult<u64> {
    let mut cursor = db
        .collection::<UsageMeterRow>(USAGE_METER)
        .find(doc! {
            "status": "reserved",
            "forwarded": false,
            "released": false,
            "updated_at": { "$lt": bson::DateTime::from_chrono(cutoff) },
        })
        .await?;
    let mut rows = Vec::new();
    while let Some(row) = cursor.try_next().await? {
        rows.push(row);
    }

    let mut released_count = 0;
    for row in rows {
        if release_one_unforwarded_row(db, &row, UsageStatus::Abandoned, None).await? {
            released_count += 1;
        }
    }
    Ok(released_count)
}

pub async fn recover_unreleased_finalized(db: &mongodb::Database) -> AppResult<u64> {
    let rows: Vec<UsageMeterRow> = db
        .collection::<UsageMeterRow>(USAGE_METER)
        .find(doc! {
            "status": "finalized",
            "released": false,
            "wallet_id": { "$ne": null },
        })
        .limit(RECOVERY_BATCH_SIZE)
        .await?
        .try_collect()
        .await?;

    let mut recovered = 0;
    for row in rows {
        let Some(quantity) = row.quantity else {
            continue;
        };
        let actual_credits =
            actual_credits_for_row(db, &row, quantity, row.model.as_deref()).await?;
        if apply_settlement_for_row(db, &row, actual_credits).await? {
            recovered += 1;
        }
    }
    Ok(recovered)
}

async fn estimate_layer_reservations(
    db: &mongodb::Database,
    ctx: &BillingRouteContext,
) -> AppResult<Vec<LayerReservation>> {
    let mut reservations = Vec::new();

    if ctx.platform_enabled {
        let metric_code = platform_metric_code(ctx.platform_metric);
        reservations.push(LayerReservation {
            layer: BillingLayer::Platform,
            reserved_credits: estimate_credits(db, metric_code, None, 1).await?,
        });
    }

    if let Some(resale) = &ctx.resale {
        reservations.push(LayerReservation {
            layer: BillingLayer::Resale,
            reserved_credits: estimate_credits(db, &resale.lago_metric_code, None, 1).await?,
        });
    }

    Ok(reservations)
}

async fn estimate_credits(
    db: &mongodb::Database,
    lago_metric_code: &str,
    model: Option<&str>,
    quantity: i64,
) -> AppResult<i64> {
    if quantity <= 0 {
        return Ok(0);
    }

    let rate = find_rate(db, lago_metric_code, model)
        .await?
        .ok_or_else(|| {
            AppError::BillingNotConfigured(format!(
                "billing rate cache is missing for metric {lago_metric_code}"
            ))
        })?;
    Ok(credits_from_micros(rate.credits_per_unit_micros, quantity))
}

async fn find_rate(
    db: &mongodb::Database,
    lago_metric_code: &str,
    model: Option<&str>,
) -> AppResult<Option<BillingRateCache>> {
    let collection = db.collection::<BillingRateCache>(BILLING_RATE_CACHE);
    if let Some(model) = model
        && let Some(rate) = collection
            .find_one(doc! { "_id": BillingRateCache::cache_id(lago_metric_code, Some(model)) })
            .await?
    {
        return Ok(Some(rate));
    }
    collection
        .find_one(doc! { "_id": BillingRateCache::cache_id(lago_metric_code, None) })
        .await
        .map_err(Into::into)
}

fn credits_from_micros(credits_per_unit_micros: i64, quantity: i64) -> i64 {
    if credits_per_unit_micros <= 0 || quantity <= 0 {
        return 0;
    }

    let micros = i128::from(credits_per_unit_micros) * i128::from(quantity);
    let credits = (micros + i128::from(CREDIT_MICROS - 1)) / i128::from(CREDIT_MICROS);
    credits.min(i128::from(i64::MAX)) as i64
}

async fn release_one_unforwarded_row(
    db: &mongodb::Database,
    row: &UsageMeterRow,
    terminal_status: UsageStatus,
    reason: Option<&str>,
) -> AppResult<bool> {
    let now = Utc::now();
    let mut set_doc = doc! {
        "status": bson::to_bson(&terminal_status)
            .unwrap_or_else(|_| bson::Bson::String("failed".to_string())),
        "released": true,
        "updated_at": bson::DateTime::from_chrono(now),
        "finalized_at": bson::DateTime::from_chrono(now),
    };
    if let Some(reason) = reason {
        set_doc.insert("last_error", reason);
    }

    let claimed = db
        .collection::<UsageMeterRow>(USAGE_METER)
        .find_one_and_update(
            doc! {
                "_id": &row.id,
                "forwarded": false,
                "status": "reserved",
                "released": false,
            },
            doc! { "$set": set_doc },
        )
        .with_options(
            FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::Before)
                .build(),
        )
        .await?;

    let Some(claimed) = claimed else {
        return Ok(false);
    };

    release_wallet_hold(db, &claimed.billing_owner_id, claimed.reserved_credits).await?;
    Ok(true)
}

async fn suspend_wallet(db: &mongodb::Database, owner_id: &str) -> AppResult<()> {
    db.collection::<BillingWallet>(BILLING_WALLET)
        .update_one(
            doc! { "owner_id": owner_id },
            doc! {
                "$set": {
                    "suspended": true,
                    "collection_state": "suspended",
                    "updated_at": bson::DateTime::from_chrono(Utc::now()),
                }
            },
        )
        .await?;
    Ok(())
}

fn is_entitled(ctx: &BillingRouteContext, entitlements: &[Entitlement]) -> bool {
    let candidates = entitlement_candidates(ctx);
    entitlements.iter().any(|entitlement| {
        entitlement.code == "*"
            || entitlement.code == "all_services"
            || candidates.contains(&entitlement.code)
    })
}

fn entitlement_candidates(ctx: &BillingRouteContext) -> BTreeSet<String> {
    [
        ctx.service_slug.clone(),
        ctx.catalog_service_id.clone(),
        ctx.user_service_id.clone(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use mongodb::bson::doc;
    use serde_json::json;
    use uuid::Uuid;

    use crate::models::billing_rate_cache::BillingRateCache;
    use crate::models::billing_wallet::{BillingWallet, CollectionState, PlanKind};
    use crate::models::service_billing::{BillingMetric, ServiceBilling};
    use crate::models::usage_meter::CredentialClass;
    use crate::services::billing::lago_client::{
        Entitlement, LagoAck, LagoError, LagoEvent, LagoUsage, OwnerProvisionInput,
    };
    use crate::services::billing::route_context::{BillingRouteContext, NodeIntent};
    use crate::test_utils::connect_test_database;

    use super::{LagoApi, gate_and_reserve, try_reserve_prepaid};

    #[derive(Clone)]
    struct EntitledLago {
        entitlements: Vec<Entitlement>,
    }

    #[async_trait]
    impl LagoApi for EntitledLago {
        async fn ensure_customer(
            &self,
            owner: &OwnerProvisionInput,
        ) -> crate::errors::AppResult<String> {
            Ok(owner.external_customer_id.clone())
        }

        async fn ensure_subscription(
            &self,
            customer_id: &str,
            _plan_code: &str,
        ) -> crate::errors::AppResult<String> {
            Ok(customer_id.to_string())
        }

        async fn record_event(&self, event: &LagoEvent) -> Result<LagoAck, LagoError> {
            Ok(LagoAck {
                transaction_id: event.transaction_id.clone(),
            })
        }

        async fn record_events_batch(
            &self,
            events: &[LagoEvent],
        ) -> Result<Vec<LagoAck>, LagoError> {
            Ok(events
                .iter()
                .map(|event| LagoAck {
                    transaction_id: event.transaction_id.clone(),
                })
                .collect())
        }

        async fn current_usage(
            &self,
            customer_id: &str,
            subscription_id: &str,
        ) -> crate::errors::AppResult<LagoUsage> {
            Ok(LagoUsage {
                customer_id: customer_id.to_string(),
                subscription_id: subscription_id.to_string(),
                raw: json!({}),
            })
        }

        async fn entitlements(
            &self,
            _subscription_id: &str,
        ) -> crate::errors::AppResult<Vec<Entitlement>> {
            Ok(self.entitlements.clone())
        }
    }

    fn wallet(owner_id: &str, balance_credits: i64) -> BillingWallet {
        let now = Utc::now();
        BillingWallet {
            id: Uuid::new_v4().to_string(),
            owner_id: owner_id.to_string(),
            lago_customer_id: owner_id.to_string(),
            lago_subscription_id: Some(format!("{owner_id}:plan")),
            plan_kind: PlanKind::Prepaid,
            balance_credits,
            reserved_credits: 0,
            pending_lago_debits: 0,
            settled_usage_row_ids: Vec::new(),
            has_payment_instrument: false,
            overdraft_cap_credits: 0,
            suspended: false,
            collection_state: CollectionState::Good,
            balance_synced_at: now,
            created_at: now,
            updated_at: now,
        }
    }

    async fn insert_platform_rate(db: &mongodb::Database, credits: i64) {
        db.collection::<BillingRateCache>(crate::models::billing_rate_cache::COLLECTION_NAME)
            .insert_one(BillingRateCache {
                id: BillingRateCache::cache_id("platform_requests", None),
                lago_metric_code: "platform_requests".to_string(),
                model: None,
                credits_per_unit_micros: credits * 1_000_000,
                synced_at: Utc::now(),
            })
            .await
            .expect("insert platform rate");
    }

    fn route_context(owner_id: &str) -> BillingRouteContext {
        BillingRouteContext::new(
            Uuid::new_v4().to_string(),
            owner_id.to_string(),
            "actor-1".to_string(),
            None,
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

    #[tokio::test]
    async fn concurrent_prepaid_reserves_never_overcommit_wallet() {
        let Some(db) = connect_test_database("billing_reserve_concurrency").await else {
            return;
        };
        let owner_id = "owner-concurrent";
        db.collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .insert_one(wallet(owner_id, 10))
            .await
            .expect("insert wallet");

        let db = Arc::new(db);
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                try_reserve_prepaid(&db, owner_id, 1)
                    .await
                    .expect("reserve query")
                    .is_some()
            }));
        }

        let mut successes = 0;
        for task in tasks {
            if task.await.expect("reserve task") {
                successes += 1;
            }
        }
        let saved = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");

        assert_eq!(successes, 10);
        assert_eq!(saved.reserved_credits, 10);
        assert_eq!(saved.available_credits(), 0);
    }

    #[tokio::test]
    async fn gate_fails_closed_when_entitlement_is_missing() {
        let Some(db) = connect_test_database("billing_gate_entitlement").await else {
            return;
        };
        let owner_id = "owner-no-entitlement";
        db.collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .insert_one(wallet(owner_id, 10))
            .await
            .expect("insert wallet");
        insert_platform_rate(&db, 1).await;
        let lago = EntitledLago {
            entitlements: Vec::new(),
        };

        let err = gate_and_reserve(&db, Some(&lago), &route_context(owner_id), false)
            .await
            .expect_err("missing entitlement must deny");

        assert!(matches!(
            err,
            crate::errors::AppError::PlanEntitlementRequired(_)
        ));
    }

    #[tokio::test]
    async fn gate_reserves_against_cached_rate_when_entitled() {
        let Some(db) = connect_test_database("billing_gate_reserves").await else {
            return;
        };
        let owner_id = "owner-entitled";
        db.collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .insert_one(wallet(owner_id, 10))
            .await
            .expect("insert wallet");
        insert_platform_rate(&db, 3).await;
        let lago = EntitledLago {
            entitlements: vec![Entitlement {
                code: "service-one".to_string(),
                raw: json!({}),
            }],
        };

        let reservation = gate_and_reserve(&db, Some(&lago), &route_context(owner_id), false)
            .await
            .expect("gate")
            .expect("reservation");
        let saved = db
            .collection::<BillingWallet>(crate::models::billing_wallet::COLLECTION_NAME)
            .find_one(doc! { "owner_id": owner_id })
            .await
            .expect("find wallet")
            .expect("wallet exists");

        assert_eq!(reservation.total_reserved_credits, 3);
        assert_eq!(saved.reserved_credits, 3);
        assert_eq!(saved.available_credits(), 7);
    }
}
