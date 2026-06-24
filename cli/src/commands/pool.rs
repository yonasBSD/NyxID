use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{OutputFormat, PoolCommands};
use crate::org_resolver::resolve_org_id;

pub async fn run(command: PoolCommands) -> Result<()> {
    match command {
        PoolCommands::Create {
            slug,
            name,
            description,
            strategy,
            members,
            org,
            auth,
        } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let org_id = match org {
                Some(raw) => Some(resolve_org_id(&mut api, &raw).await?),
                None => None,
            };

            let mut body = serde_json::json!({
                "slug": slug,
                "name": name,
                "strategy": strategy.as_str(),
                "members": members
                    .into_iter()
                    .map(|user_service_id| serde_json::json!({ "user_service_id": user_service_id }))
                    .collect::<Vec<_>>(),
            });
            insert_opt_str(&mut body, "description", description.as_deref());
            insert_opt_str(&mut body, "org_id", org_id.as_deref());

            let pool: Value = api.post("/service-pools", &body).await?;
            print_pool_created(output, &pool)
        }
        PoolCommands::List { org, auth } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let org_id = match org {
                Some(raw) => Some(resolve_org_id(&mut api, &raw).await?),
                None => None,
            };
            let path = service_pools_path(org_id.as_deref());
            let pools: Value = api.get(&path).await?;
            print_pool_list(output, &pools)
        }
        PoolCommands::Show { pool, auth } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let pool: Value = api
                .get(&format!("/service-pools/{}", urlencoding::encode(&pool)))
                .await?;
            print_pool(output, &pool)
        }
        PoolCommands::Delete { pool_id, auth } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            api.delete_empty(&format!("/service-pools/{pool_id}"))
                .await?;
            match output {
                OutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "ok": true }))?
                ),
                OutputFormat::Table => eprintln!("Service pool deleted."),
            }
            Ok(())
        }
        PoolCommands::AddMember {
            pool,
            service,
            weight,
            enabled,
            auth,
        } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let mut body = serde_json::json!({ "user_service_id": service });
            if let Some(weight) = weight {
                body["weight"] = serde_json::json!(weight);
            }
            if let Some(enabled) = enabled {
                body["enabled"] = Value::Bool(enabled);
            }
            let pool: Value = api
                .post(
                    &format!("/service-pools/{}/members", urlencoding::encode(&pool)),
                    &body,
                )
                .await?;
            print_pool(output, &pool)
        }
        PoolCommands::RemoveMember {
            pool,
            service,
            auth,
        } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let pool: Value = api
                .delete(&format!(
                    "/service-pools/{}/members/{}",
                    urlencoding::encode(&pool),
                    urlencoding::encode(&service)
                ))
                .await?;
            print_pool(output, &pool)
        }
        PoolCommands::SetStrategy {
            pool,
            strategy,
            auth,
        } => {
            let output = auth.output;
            let mut api = ApiClient::from_auth_checked(&auth).await?;
            let body = serde_json::json!({ "strategy": strategy.as_str() });
            let pool: Value = api
                .put(
                    &format!("/service-pools/{}", urlencoding::encode(&pool)),
                    &body,
                )
                .await?;
            print_pool(output, &pool)
        }
    }
}

fn service_pools_path(org_id: Option<&str>) -> String {
    match org_id {
        Some(org_id) => format!("/service-pools?org_id={}", urlencoding::encode(org_id)),
        None => "/service-pools".to_string(),
    }
}

fn insert_opt_str(body: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        body[key] = Value::String(value.to_string());
    }
}

fn print_pool_created(output: OutputFormat, pool: &Value) -> Result<()> {
    match output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(pool)?),
        OutputFormat::Table => {
            eprintln!(
                "Service pool '{}' created.",
                pool.get("slug").and_then(Value::as_str).unwrap_or("-")
            );
            print_pool_table_summary(pool);
        }
    }
    Ok(())
}

fn print_pool(output: OutputFormat, pool: &Value) -> Result<()> {
    match output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(pool)?),
        OutputFormat::Table => print_pool_detail(pool),
    }
    Ok(())
}

fn print_pool_list(output: OutputFormat, pools: &Value) -> Result<()> {
    match output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(pools)?),
        OutputFormat::Table => {
            let items = pools
                .get("pools")
                .and_then(Value::as_array)
                .or_else(|| pools.as_array());
            if let Some(items) = items {
                if items.is_empty() {
                    eprintln!("No service pools.");
                    return Ok(());
                }

                let mut table = Table::new();
                table.load_preset(UTF8_FULL_CONDENSED);
                table.set_header(["ID", "Slug", "Name", "Strategy", "Members", "Active"]);
                for pool in items {
                    let id = pool
                        .get("id")
                        .or_else(|| pool.get("_id"))
                        .and_then(Value::as_str)
                        .unwrap_or("-");
                    table.add_row([
                        crate::commands::short_id(id).to_string(),
                        pool.get("slug")
                            .and_then(Value::as_str)
                            .unwrap_or("-")
                            .to_string(),
                        pool.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("-")
                            .to_string(),
                        pool.get("strategy")
                            .and_then(Value::as_str)
                            .unwrap_or("-")
                            .to_string(),
                        member_count(pool).to_string(),
                        yes_no(
                            pool.get("is_active")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        ),
                    ]);
                }
                eprintln!("{table}");
            }
        }
    }
    Ok(())
}

fn print_pool_detail(pool: &Value) {
    eprintln!(
        "ID:        {}",
        pool.get("id")
            .or_else(|| pool.get("_id"))
            .and_then(Value::as_str)
            .unwrap_or("-")
    );
    eprintln!(
        "Slug:      {}",
        pool.get("slug").and_then(Value::as_str).unwrap_or("-")
    );
    eprintln!(
        "Name:      {}",
        pool.get("name").and_then(Value::as_str).unwrap_or("-")
    );
    if let Some(description) = pool.get("description").and_then(Value::as_str) {
        eprintln!("Description: {description}");
    }
    eprintln!(
        "Strategy:  {}",
        pool.get("strategy").and_then(Value::as_str).unwrap_or("-")
    );
    eprintln!(
        "Active:    {}",
        yes_no(
            pool.get("is_active")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        )
    );
    eprintln!("Members:   {}", member_count(pool));

    if let Some(members) = pool.get("members").and_then(Value::as_array)
        && !members.is_empty()
    {
        eprintln!();
        let mut table = Table::new();
        table.load_preset(UTF8_FULL_CONDENSED);
        table.set_header(["Service ID", "Weight", "Enabled"]);
        for member in members {
            table.add_row([
                member
                    .get("user_service_id")
                    .and_then(Value::as_str)
                    .map(crate::commands::short_id)
                    .unwrap_or("-")
                    .to_string(),
                member
                    .get("weight")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .to_string(),
                yes_no(
                    member
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                ),
            ]);
        }
        eprintln!("{table}");
    }
}

fn print_pool_table_summary(pool: &Value) {
    eprintln!(
        "ID:       {}",
        pool.get("id")
            .or_else(|| pool.get("_id"))
            .and_then(Value::as_str)
            .unwrap_or("-")
    );
    eprintln!(
        "Slug:     {}",
        pool.get("slug").and_then(Value::as_str).unwrap_or("-")
    );
    eprintln!(
        "Strategy: {}",
        pool.get("strategy").and_then(Value::as_str).unwrap_or("-")
    );
    eprintln!("Members:  {}", member_count(pool));
}

fn member_count(pool: &Value) -> usize {
    pool.get("members")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{OutputFormat, PoolCommands, PoolStrategyArg};
    use crate::test_support::mock_auth_with_output;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ORG_UUID: &str = "11111111-1111-4111-8111-111111111111";

    fn pool_response() -> Value {
        serde_json::json!({
            "id": "pool-1",
            "owner_user_id": "owner-1",
            "slug": "llm-pool",
            "name": "LLM Pool",
            "strategy": "weighted",
            "members": [
                { "user_service_id": "svc-1", "weight": 2, "enabled": true }
            ],
            "rr_counter": 0,
            "is_active": true,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        })
    }

    #[test]
    fn strategy_arg_uses_backend_wire_values() {
        assert_eq!(PoolStrategyArg::RoundRobin.as_str(), "round_robin");
        assert_eq!(PoolStrategyArg::Weighted.as_str(), "weighted");
    }

    #[tokio::test]
    async fn create_posts_pool_body_with_resolved_org_and_unresolved_members() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": ORG_UUID,
                "slug": "acme",
                "display_name": "Acme"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/service-pools"))
            .and(body_json(serde_json::json!({
                "slug": "llm-pool",
                "name": "LLM Pool",
                "description": "primary pool",
                "strategy": "weighted",
                "members": [{ "user_service_id": "svc-1" }],
                "org_id": ORG_UUID
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(pool_response()))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::Create {
            slug: "llm-pool".to_string(),
            name: "LLM Pool".to_string(),
            description: Some("primary pool".to_string()),
            strategy: PoolStrategyArg::Weighted,
            members: vec!["llm-openai".to_string()],
            org: Some("acme".to_string()),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool create should succeed");
    }

    #[tokio::test]
    async fn list_appends_resolved_org_query_param() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/orgs/acme"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": ORG_UUID,
                "slug": "acme",
                "display_name": "Acme"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/service-pools"))
            .and(query_param("org_id", ORG_UUID))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "pools": [pool_response()]
            })))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::List {
            org: Some("acme".to_string()),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool list should succeed");
    }

    #[tokio::test]
    async fn show_fetches_pool_by_id_or_slug() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/service-pools/llm-pool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pool_response()))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::Show {
            pool: "llm-pool".to_string(),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool show should succeed");
    }

    #[tokio::test]
    async fn add_member_posts_identifiers_to_backend_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/service-pools/llm-pool/members"))
            .and(body_json(serde_json::json!({
                "user_service_id": "llm-openai",
                "weight": 3,
                "enabled": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(pool_response()))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::AddMember {
            pool: "llm-pool".to_string(),
            service: "llm-openai".to_string(),
            weight: Some(3),
            enabled: Some(false),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool add-member should succeed");
    }

    #[tokio::test]
    async fn remove_member_deletes_identifier_path() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/service-pools/llm-pool/members/llm-openai"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pool_response()))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::RemoveMember {
            pool: "llm-pool".to_string(),
            service: "llm-openai".to_string(),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool remove-member should succeed");
    }

    #[tokio::test]
    async fn set_strategy_puts_strategy_update() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/service-pools/llm-pool"))
            .and(body_json(serde_json::json!({ "strategy": "round_robin" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(pool_response()))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::SetStrategy {
            pool: "llm-pool".to_string(),
            strategy: PoolStrategyArg::RoundRobin,
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool set-strategy should succeed");
    }

    #[tokio::test]
    async fn delete_uses_id_without_slug_resolution() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/service-pools/pool-1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        run(PoolCommands::Delete {
            pool_id: "pool-1".to_string(),
            auth: mock_auth_with_output(server.uri(), OutputFormat::Json),
        })
        .await
        .expect("pool delete should succeed");
    }
}
