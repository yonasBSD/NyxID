use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::ApiClient;
use crate::cli::{BillingCommands, BillingUsagePeriodArg, OutputFormat};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct BillingWalletResponse {
    pub owner_id: String,
    pub plan_kind: String,
    pub collection_state: String,
    pub balance_credits: i64,
    pub reserved_credits: i64,
    pub pending_lago_debits: i64,
    pub available_credits: i64,
    pub available_with_overdraft_credits: i64,
    pub has_payment_instrument: bool,
    pub overdraft_cap_credits: i64,
    pub suspended: bool,
    pub lago_customer_id: String,
    pub lago_subscription_id: Option<String>,
    pub lago_wallet_id: Option<String>,
    pub balance_synced_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub created: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct BillingUsageResponse {
    pub owner_id: String,
    pub period: String,
    pub rows: Vec<BillingUsageRow>,
    pub totals: BillingUsageTotals,
    pub billing: BillingReadOnlyBlock,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct BillingUsageTotals {
    pub quantity: i64,
    pub requests: i64,
    pub bytes: i64,
    pub events: i64,
    pub estimated_credits_micros: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct BillingReadOnlyBlock {
    pub charging_enabled: bool,
    pub lago_configured: bool,
    pub source: String,
    pub rates_are_approximate: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct TopUpResponse {
    pub owner_id: String,
    pub amount_credits: i64,
    pub idempotency_key: String,
    pub checkout_url: String,
    pub payment_provider: Option<String>,
    pub lago_wallet_transaction_id: Option<String>,
    pub lago_invoice_id: Option<String>,
    pub status: String,
    pub reused: bool,
}

#[derive(Debug, Serialize)]
struct ProvisionWalletRequest {
    owner_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TopUpRequest {
    amount_credits: i64,
    idempotency_key: String,
    owner_id: Option<String>,
}

pub fn usage_path(period: Option<BillingUsagePeriodArg>) -> String {
    match period {
        Some(period) => format!("/billing/usage?period={}", period.as_query_value()),
        None => "/billing/usage".to_string(),
    }
}

pub async fn get_usage(
    api: &mut ApiClient,
    period: Option<BillingUsagePeriodArg>,
) -> Result<BillingUsageResponse> {
    api.get(&usage_path(period)).await
}

pub async fn run(command: BillingCommands) -> Result<()> {
    match command {
        BillingCommands::Wallet {
            provision,
            owner_id,
            auth,
        } => {
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let wallet: BillingWalletResponse = if provision {
                api.post(
                    "/billing/wallet",
                    &ProvisionWalletRequest {
                        owner_id: owner_id.clone(),
                    },
                )
                .await?
            } else {
                api.get("/billing/wallet").await?
            };
            print_wallet(&wallet, auth.output)
        }
        BillingCommands::Usage { period, auth } => {
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let usage = get_usage(&mut api, period).await?;
            print_usage(&usage, auth.output)
        }
        BillingCommands::Topup {
            amount_credits,
            idempotency_key,
            owner_id,
            open,
            auth,
        } => {
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let key =
                idempotency_key.unwrap_or_else(|| format!("nyxid-cli-topup-{}", Uuid::new_v4()));
            let response: TopUpResponse = api
                .post(
                    "/billing/topup",
                    &TopUpRequest {
                        amount_credits,
                        idempotency_key: key,
                        owner_id,
                    },
                )
                .await?;
            if open && let Err(error) = crate::browser::open_browser(&response.checkout_url) {
                eprintln!("Could not open checkout URL: {error}");
            }
            print_topup(&response, auth.output)
        }
    }
}

fn print_wallet(wallet: &BillingWalletResponse, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(wallet)?);
        }
        OutputFormat::Table => {
            eprintln!("Billing Wallet");
            eprintln!();
            eprintln!("Owner:             {}", wallet.owner_id);
            eprintln!("Plan:              {}", wallet.plan_kind);
            eprintln!("Status:            {}", wallet.collection_state);
            eprintln!("Balance:           {} credits", wallet.balance_credits);
            eprintln!("Available:         {} credits", wallet.available_credits);
            eprintln!("Reserved:          {} credits", wallet.reserved_credits);
            eprintln!("Pending Debits:    {} credits", wallet.pending_lago_debits);
            eprintln!(
                "Overdraft Cap:     {} credits",
                wallet.overdraft_cap_credits
            );
            eprintln!("Suspended:         {}", wallet.suspended);
            if wallet.created {
                eprintln!("Created:           true");
            }
        }
    }
    Ok(())
}

fn print_usage(usage: &BillingUsageResponse, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(usage)?);
        }
        OutputFormat::Table => {
            eprintln!("Billing Usage ({})", usage.period);
            eprintln!();
            eprintln!("Owner:             {}", usage.owner_id);
            eprintln!(
                "Charging Enabled:  {}",
                usage.billing.charging_enabled && usage.billing.lago_configured
            );
            eprintln!(
                "Estimated Cost:    {}",
                format_estimated_credits(usage.totals.estimated_credits_micros)
            );
            eprintln!();

            if usage.rows.is_empty() {
                eprintln!("No usage in this period.");
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(["Service", "Layer", "Metric", "Quantity", "Events", "Cost"]);
            for row in &usage.rows {
                table.add_row([
                    row.service_slug
                        .as_deref()
                        .or(row.service_id.as_deref())
                        .unwrap_or("-")
                        .to_string(),
                    row.layer.clone(),
                    row.metric.clone(),
                    row.quantity.to_string(),
                    row.events.to_string(),
                    format_estimated_credits(row.estimated_credits_micros),
                ]);
            }
            eprintln!("{table}");
        }
    }
    Ok(())
}

fn print_topup(response: &TopUpResponse, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(response)?);
        }
        OutputFormat::Table => {
            eprintln!("Billing Top-up");
            eprintln!();
            eprintln!("Owner:             {}", response.owner_id);
            eprintln!("Amount:            {} credits", response.amount_credits);
            eprintln!("Status:            {}", response.status);
            eprintln!("Reused:            {}", response.reused);
            eprintln!("Checkout URL:      {}", response.checkout_url);
            if let Some(invoice_id) = &response.lago_invoice_id {
                eprintln!("Lago Invoice:      {invoice_id}");
            }
        }
    }
    Ok(())
}

fn format_estimated_credits(value: Option<i64>) -> String {
    match value {
        Some(micros) => format!("{:.6} credits", micros as f64 / 1_000_000.0),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{BillingCommands, BillingUsagePeriodArg, OutputFormat};
    use crate::test_support::{mock_auth, mock_auth_with_output};
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn usage_path_targets_billing_usage() {
        assert_eq!(usage_path(None), "/billing/usage");
        assert_eq!(
            usage_path(Some(BillingUsagePeriodArg::Last7Days)),
            "/billing/usage?period=7d"
        );
    }

    #[tokio::test]
    async fn wallet_show_calls_wallet_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/billing/wallet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(wallet_json(false)))
            .expect(1)
            .mount(&server)
            .await;

        run(BillingCommands::Wallet {
            provision: false,
            owner_id: None,
            auth: mock_auth(server.uri()),
        })
        .await
        .expect("wallet should succeed");
    }

    #[tokio::test]
    async fn wallet_provision_posts_owner_scope() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/billing/wallet"))
            .and(body_json(serde_json::json!({ "owner_id": "owner-1" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(wallet_json(true)))
            .expect(1)
            .mount(&server)
            .await;

        run(BillingCommands::Wallet {
            provision: true,
            owner_id: Some("owner-1".to_string()),
            auth: mock_auth(server.uri()),
        })
        .await
        .expect("wallet provision should succeed");
    }

    #[tokio::test]
    async fn usage_reads_selected_period() {
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
                    "charging_enabled": true,
                    "lago_configured": true,
                    "source": "usage_meter",
                    "rates_are_approximate": true
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        run(BillingCommands::Usage {
            period: Some(BillingUsagePeriodArg::Last7Days),
            auth: mock_auth(server.uri()),
        })
        .await
        .expect("usage should succeed");
    }

    #[tokio::test]
    async fn topup_posts_amount_and_idempotency_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/billing/topup"))
            .and(body_json(serde_json::json!({
                "amount_credits": 50,
                "idempotency_key": "topup-key-123",
                "owner_id": null
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "owner_id": "owner-1",
                "amount_credits": 50,
                "idempotency_key": "topup-key-123",
                "checkout_url": "https://checkout.example.com/session",
                "payment_provider": "stripe",
                "lago_wallet_transaction_id": "txn-1",
                "lago_invoice_id": "invoice-1",
                "status": "checkout_created",
                "reused": false
            })))
            .expect(1)
            .mount(&server)
            .await;

        run(BillingCommands::Topup {
            amount_credits: 50,
            idempotency_key: Some("topup-key-123".to_string()),
            owner_id: None,
            open: false,
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("topup should succeed");
    }

    fn wallet_json(created: bool) -> serde_json::Value {
        serde_json::json!({
            "owner_id": "owner-1",
            "plan_kind": "prepaid",
            "collection_state": "good",
            "balance_credits": 100,
            "reserved_credits": 10,
            "pending_lago_debits": 5,
            "available_credits": 85,
            "available_with_overdraft_credits": 85,
            "has_payment_instrument": false,
            "overdraft_cap_credits": 0,
            "suspended": false,
            "lago_customer_id": "customer-1",
            "lago_subscription_id": "subscription-1",
            "lago_wallet_id": "wallet-1",
            "balance_synced_at": "2026-06-26T00:00:00Z",
            "created_at": "2026-06-26T00:00:00Z",
            "updated_at": "2026-06-26T00:00:00Z",
            "created": created
        })
    }
}
