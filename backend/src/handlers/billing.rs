use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{Duration, Utc};
use futures::TryStreamExt;
use hmac::{Hmac, Mac};
use mongodb::bson::{self, Bson, Document, doc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::billing_rate_cache::{BillingRateCache, COLLECTION_NAME as BILLING_RATE_CACHE};
use crate::models::service_billing::BillingMetric;
use crate::models::usage_meter::COLLECTION_NAME as USAGE_METER;
use crate::mw::auth::AuthUser;

type HmacSha256 = Hmac<Sha256>;

const LAGO_SIGNATURE_HEADER: &str = "x-lago-signature";
const LAGO_SIGNATURE_ALGORITHM_HEADER: &str = "x-lago-signature-algorithm";
const LAGO_UNIQUE_KEY_HEADER: &str = "x-lago-unique-key";
const LAGO_HMAC_SHA256_ALGORITHM: &str = "hmac";

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub period: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BillingUsageResponse {
    pub owner_id: String,
    pub period: String,
    pub rows: Vec<BillingUsageRow>,
    pub totals: BillingUsageTotals,
    pub billing: BillingReadOnlyBlock,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BillingUsageRow {
    pub service_slug: Option<String>,
    pub service_id: Option<String>,
    pub metric: BillingMetric,
    pub lago_metric_code: String,
    pub layer: String,
    pub quantity: i64,
    pub requests: i64,
    pub bytes: i64,
    pub events: i64,
    pub lago_acked: bool,
    pub estimated_credits_micros: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BillingUsageTotals {
    pub quantity: i64,
    pub requests: i64,
    pub bytes: i64,
    pub events: i64,
    pub estimated_credits_micros: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BillingReadOnlyBlock {
    pub charging_enabled: bool,
    pub lago_configured: bool,
    pub source: String,
    pub rates_are_approximate: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LagoWebhookResponse {
    pub ok: bool,
    pub action: String,
}

pub async fn get_usage(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<UsageQuery>,
) -> AppResult<Json<BillingUsageResponse>> {
    let owner_id = auth_user.user_id.to_string();
    let period = query.period.unwrap_or_else(|| "30d".to_string());
    let since = period_start(&period);
    let mut match_doc = doc! {
        "billing_owner_id": &owner_id,
        "quantity": { "$ne": null },
        "created_at": { "$gte": bson::DateTime::from_chrono(since) },
        "status": { "$in": ["finalized", "dead_letter"] },
    };
    match_doc.insert(
        "$or",
        vec![
            doc! { "status": "finalized" },
            doc! { "status": "dead_letter", "forwarded": true },
        ],
    );

    let pipeline = vec![
        doc! { "$match": match_doc },
        doc! {
            "$group": {
                "_id": {
                    "service_slug": "$service_slug",
                    "service_id": "$service_id",
                    "metric": "$metric",
                    "lago_metric_code": "$lago_metric_code",
                    "layer": "$layer",
                    "lago_acked": "$lago_acked",
                },
                "quantity": { "$sum": "$quantity" },
                "events": { "$sum": 1 },
            }
        },
        doc! { "$sort": { "_id.service_slug": 1, "_id.layer": 1, "_id.metric": 1 } },
    ];

    let mut cursor = state
        .db
        .collection::<Document>(USAGE_METER)
        .aggregate(pipeline)
        .await?;
    let mut rows = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        let Some(id_doc) = doc.get_document("_id").ok() else {
            continue;
        };
        let metric = id_doc
            .get_str("metric")
            .ok()
            .and_then(parse_metric)
            .unwrap_or_default();
        let quantity = doc_i64(&doc, "quantity").unwrap_or(0);
        let lago_metric_code = id_doc.get_str("lago_metric_code").unwrap_or("").to_string();
        let model_rate = find_rate(&state.db, &lago_metric_code, None).await?;
        let estimated_credits_micros =
            model_rate.map(|rate| rate.credits_per_unit_micros.saturating_mul(quantity));
        rows.push(BillingUsageRow {
            service_slug: id_doc.get_str("service_slug").ok().map(ToString::to_string),
            service_id: id_doc.get_str("service_id").ok().map(ToString::to_string),
            metric,
            lago_metric_code,
            layer: id_doc.get_str("layer").unwrap_or("platform").to_string(),
            quantity,
            requests: if metric == BillingMetric::Requests {
                quantity
            } else {
                0
            },
            bytes: if metric == BillingMetric::Bytes {
                quantity
            } else {
                0
            },
            events: doc_i64(&doc, "events").unwrap_or(0),
            lago_acked: id_doc.get_bool("lago_acked").unwrap_or(false),
            estimated_credits_micros,
        });
    }

    let totals = BillingUsageTotals {
        quantity: rows.iter().map(|row| row.quantity).sum(),
        requests: rows.iter().map(|row| row.requests).sum(),
        bytes: rows.iter().map(|row| row.bytes).sum(),
        events: rows.iter().map(|row| row.events).sum(),
        estimated_credits_micros: sum_optional(rows.iter().map(|row| row.estimated_credits_micros)),
    };

    Ok(Json(BillingUsageResponse {
        owner_id,
        period,
        rows,
        totals,
        billing: BillingReadOnlyBlock {
            charging_enabled: false,
            lago_configured: state.billing.lago_configured(),
            source: "usage_meter".to_string(),
            rates_are_approximate: true,
        },
    }))
}

pub async fn lago_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<(StatusCode, Json<LagoWebhookResponse>)> {
    let expected_secret = state.config.lago_webhook_secret.as_deref().ok_or_else(|| {
        tracing::warn!("Lago webhook received but LAGO_WEBHOOK_SECRET is not configured");
        AppError::Unauthorized("Lago webhook signature verification failed".to_string())
    })?;

    verify_lago_signature(&headers, &body, expected_secret).map_err(|error| {
        tracing::warn!(error = %error, "Lago webhook signature verification failed");
        AppError::Unauthorized("Lago webhook signature verification failed".to_string())
    })?;

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("Lago webhook payload must be valid JSON".to_string()))?;
    let event_type = lago_event_type(&payload).ok_or_else(|| {
        AppError::BadRequest("Lago webhook payload is missing webhook_type".to_string())
    })?;
    let unique_key = headers
        .get(LAGO_UNIQUE_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let lago = state.billing.lago_client();
    let outcome = crate::services::billing::webhook::handle_lago_webhook_event(
        &state.db,
        lago.as_deref(),
        event_type,
        &payload,
    )
    .await?;

    tracing::info!(
        event_type,
        unique_key,
        action = outcome.action.as_str(),
        owner_id = outcome.owner_id.as_deref().unwrap_or(""),
        customer_id = outcome.customer_id.as_deref().unwrap_or(""),
        "Lago webhook processed"
    );

    Ok((
        StatusCode::OK,
        Json(LagoWebhookResponse {
            ok: true,
            action: outcome.action.as_str().to_string(),
        }),
    ))
}

fn verify_lago_signature(
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
) -> Result<(), &'static str> {
    if secret.trim().is_empty() {
        return Err("missing expected secret");
    }

    let algorithm =
        header_str(headers, LAGO_SIGNATURE_ALGORITHM_HEADER).unwrap_or(LAGO_HMAC_SHA256_ALGORITHM);
    if !algorithm.eq_ignore_ascii_case(LAGO_HMAC_SHA256_ALGORITHM) {
        return Err("unsupported signature algorithm");
    }

    let received = header_str(headers, LAGO_SIGNATURE_HEADER).ok_or("missing signature")?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| "invalid expected secret")?;
    mac.update(body);
    let expected = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    if constant_time_str_eq(received.trim(), &expected) {
        Ok(())
    } else {
        Err("signature mismatch")
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn constant_time_str_eq(received: &str, expected: &str) -> bool {
    let received_hash = Sha256::digest(received.as_bytes());
    let expected_hash = Sha256::digest(expected.as_bytes());
    received_hash.ct_eq(&expected_hash).into()
}

fn lago_event_type(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("webhook_type")
        .or_else(|| payload.get("event_type"))
        .or_else(|| payload.get("event"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn period_start(period: &str) -> chrono::DateTime<Utc> {
    let now = Utc::now();
    match period {
        "24h" | "1d" => now - Duration::days(1),
        "7d" => now - Duration::days(7),
        "90d" => now - Duration::days(90),
        "all" => now - Duration::days(3650),
        _ => now - Duration::days(30),
    }
}

async fn find_rate(
    db: &mongodb::Database,
    lago_metric_code: &str,
    model: Option<&str>,
) -> AppResult<Option<BillingRateCache>> {
    let specific_id = BillingRateCache::cache_id(lago_metric_code, model);
    if let Some(rate) = db
        .collection::<BillingRateCache>(BILLING_RATE_CACHE)
        .find_one(doc! { "_id": specific_id })
        .await?
    {
        return Ok(Some(rate));
    }
    db.collection::<BillingRateCache>(BILLING_RATE_CACHE)
        .find_one(doc! { "_id": BillingRateCache::cache_id(lago_metric_code, None) })
        .await
        .map_err(Into::into)
}

fn parse_metric(value: &str) -> Option<BillingMetric> {
    match value {
        "tokens" => Some(BillingMetric::Tokens),
        "requests" => Some(BillingMetric::Requests),
        "bytes" => Some(BillingMetric::Bytes),
        _ => None,
    }
}

fn doc_i64(doc: &Document, key: &str) -> Option<i64> {
    match doc.get(key)? {
        Bson::Int32(v) => Some(i64::from(*v)),
        Bson::Int64(v) => Some(*v),
        Bson::Double(v) => Some(v.round() as i64),
        _ => None,
    }
}

fn sum_optional(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut saw_value = false;
    let mut total = 0_i64;
    for value in values.flatten() {
        saw_value = true;
        total = total.saturating_add(value);
    }
    saw_value.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_headers(secret: &str, body: &[u8]) -> HeaderMap {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = BASE64_STANDARD.encode(mac.finalize().into_bytes());
        let mut headers = HeaderMap::new();
        headers.insert(LAGO_SIGNATURE_HEADER, signature.parse().unwrap());
        headers.insert(
            LAGO_SIGNATURE_ALGORITHM_HEADER,
            LAGO_HMAC_SHA256_ALGORITHM.parse().unwrap(),
        );
        headers
    }

    #[test]
    fn lago_signature_accepts_valid_hmac() {
        let body = br#"{"webhook_type":"wallet.updated"}"#;
        let headers = signed_headers("secret", body);

        assert!(verify_lago_signature(&headers, body, "secret").is_ok());
    }

    #[test]
    fn lago_signature_rejects_tampered_body() {
        let headers = signed_headers("secret", br#"{"webhook_type":"wallet.updated"}"#);

        assert!(
            verify_lago_signature(
                &headers,
                br#"{"webhook_type":"subscription.started"}"#,
                "secret"
            )
            .is_err()
        );
    }

    #[test]
    fn lago_event_type_reads_webhook_type() {
        let payload = serde_json::json!({ "webhook_type": "wallet.updated" });

        assert_eq!(lago_event_type(&payload), Some("wallet.updated"));
    }
}
