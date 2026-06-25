#![allow(dead_code)]
// Intentionally unregistered until the billing settle contract is proven idempotent.

use anyhow::Result;
use serde::Deserialize;

use crate::api::ApiClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingUsagePeriod {
    Last24Hours,
    Last7Days,
    Last30Days,
    Last90Days,
    All,
}

impl BillingUsagePeriod {
    pub fn as_query_value(self) -> &'static str {
        match self {
            Self::Last24Hours => "24h",
            Self::Last7Days => "7d",
            Self::Last30Days => "30d",
            Self::Last90Days => "90d",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BillingUsageResponse {
    pub owner_id: String,
    pub period: String,
    pub rows: Vec<BillingUsageRow>,
    pub totals: BillingUsageTotals,
    pub billing: BillingReadOnlyBlock,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BillingUsageRow {
    pub service_slug: Option<String>,
    pub service_id: Option<String>,
    pub metric: String,
    pub lago_metric_code: String,
    pub layer: String,
    pub quantity: i64,
    pub requests: i64,
    pub bytes: i64,
    pub events: i64,
    pub lago_acked: bool,
    pub estimated_credits_micros: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BillingUsageTotals {
    pub quantity: i64,
    pub requests: i64,
    pub bytes: i64,
    pub events: i64,
    pub estimated_credits_micros: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BillingReadOnlyBlock {
    pub charging_enabled: bool,
    pub lago_configured: bool,
    pub source: String,
    pub rates_are_approximate: bool,
}

pub fn usage_path(period: Option<BillingUsagePeriod>) -> String {
    match period {
        Some(period) => format!("/billing/usage?period={}", period.as_query_value()),
        None => "/billing/usage".to_string(),
    }
}

pub async fn get_usage(
    api: &mut ApiClient,
    period: Option<BillingUsagePeriod>,
) -> Result<BillingUsageResponse> {
    api.get(&usage_path(period)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn usage_path_is_read_only_usage_only() {
        assert_eq!(usage_path(None), "/billing/usage");
        assert_eq!(
            usage_path(Some(BillingUsagePeriod::Last7Days)),
            "/billing/usage?period=7d"
        );
    }

    #[tokio::test]
    async fn get_usage_calls_read_only_billing_usage_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/billing/usage"))
            .and(query_param("period", "7d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "owner_id": "owner-1",
                "period": "7d",
                "rows": [],
                "totals": {
                    "quantity": 0,
                    "requests": 0,
                    "bytes": 0,
                    "events": 0,
                    "estimated_credits_micros": null
                },
                "billing": {
                    "charging_enabled": false,
                    "lago_configured": false,
                    "source": "usage_meter",
                    "rates_are_approximate": true
                }
            })))
            .mount(&server)
            .await;

        let mut api = ApiClient::new(&server.uri(), "test-token".to_string()).unwrap();
        let response = get_usage(&mut api, Some(BillingUsagePeriod::Last7Days))
            .await
            .expect("read-only billing usage should parse");

        assert_eq!(response.owner_id, "owner-1");
        assert!(!response.billing.charging_enabled);
        assert_eq!(response.billing.source, "usage_meter");
    }
}
