use std::io::Write;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{ApiKeyCommands, OutputFormat};

pub async fn run(command: ApiKeyCommands) -> Result<()> {
    match command {
        ApiKeyCommands::Create {
            name,
            scopes,
            expires_in_days,
            allowed_services,
            allowed_nodes,
            allow_all_services,
            allow_all_nodes,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let key_name = match name {
                Some(n) => n,
                None => {
                    eprint!("Key name: ");
                    std::io::stderr().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let trimmed = input.trim().to_string();
                    if trimmed.is_empty() {
                        "CLI Key".to_string()
                    } else {
                        trimmed
                    }
                }
            };

            let scope_str = scopes.as_deref().unwrap_or("read write");

            let mut body = serde_json::json!({
                "name": key_name,
                "scopes": scope_str,
            });

            if let Some(days) = expires_in_days
                && days > 0
            {
                let expires = chrono::Utc::now() + chrono::Duration::days(i64::from(days));
                body["expires_at"] = Value::String(expires.to_rfc3339());
            }

            if let Some(services) = allowed_services {
                let ids: Vec<&str> = services.split(',').map(|s| s.trim()).collect();
                body["allowed_service_ids"] = serde_json::json!(ids);
            }
            if let Some(nodes) = allowed_nodes {
                let ids: Vec<&str> = nodes.split(',').map(|s| s.trim()).collect();
                body["allowed_node_ids"] = serde_json::json!(ids);
            }
            if allow_all_services {
                body["allow_all_services"] = Value::Bool(true);
            }
            if allow_all_nodes {
                body["allow_all_nodes"] = Value::Bool(true);
            }

            let result: Value = api.post("/api-keys", &body).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let name = result["name"].as_str().unwrap_or("-");
                    let key = result["full_key"].as_str().unwrap_or("-");
                    let scopes = result["scopes"].as_str().unwrap_or("-");
                    let expires = result["expires_at"].as_str().unwrap_or("never");

                    eprintln!("API key created!");
                    eprintln!();
                    eprintln!("Name:    {name}");
                    eprintln!("Key:     {key}  (save this -- shown only once)");
                    eprintln!("Scopes:  {scopes}");
                    eprintln!("Expires: {expires}");
                    eprintln!();
                    eprintln!("Set as environment variable:");
                    eprintln!("  export NYXID_API_KEY=\"{key}\"");
                }
            }
            Ok(())
        }

        ApiKeyCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let keys: Value = api.get("/api-keys").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&keys)?);
                }
                OutputFormat::Table => {
                    let items = keys
                        .get("keys")
                        .and_then(|v| v.as_array())
                        .or_else(|| keys.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No API keys.");
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header([
                            "ID",
                            "Name",
                            "Scopes",
                            "Services",
                            "Nodes",
                            "Last Used",
                        ]);

                        for key in items {
                            let id = key["id"].as_str().or(key["_id"].as_str()).unwrap_or("-");
                            let name = key["name"].as_str().unwrap_or("-");
                            let scopes = key["scopes"].as_str().unwrap_or("-");
                            let services = if key["allow_all_services"].as_bool().unwrap_or(true) {
                                "all".to_string()
                            } else {
                                key["allowed_services"]
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|s| {
                                                s["slug"].as_str().or(s["label"].as_str())
                                            })
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_else(|| "-".to_string())
                            };
                            let nodes = if key["allow_all_nodes"].as_bool().unwrap_or(true) {
                                "all".to_string()
                            } else {
                                key["allowed_nodes"]
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|n| n["name"].as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_else(|| "-".to_string())
                            };
                            let last_used = key["last_used_at"].as_str().unwrap_or("never");
                            table.add_row([id, name, scopes, &services, &nodes, last_used]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        ApiKeyCommands::Show { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let key: Value = api.get(&format!("/api-keys/{id}")).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&key)?);
                }
                OutputFormat::Table => {
                    let name = key["name"].as_str().unwrap_or("-");
                    let key_id = key["id"].as_str().or(key["_id"].as_str()).unwrap_or(&id);
                    let scopes = key["scopes"].as_str().unwrap_or("-");
                    let expires = key["expires_at"].as_str().unwrap_or("never");
                    let last_used = key["last_used_at"].as_str().unwrap_or("never");
                    let prefix = key["key_prefix"].as_str().unwrap_or("-");

                    eprintln!("Name:       {name}");
                    eprintln!("ID:         {key_id}");
                    eprintln!("Prefix:     {prefix}");
                    eprintln!("Scopes:     {scopes}");
                    eprintln!("Expires:    {expires}");
                    eprintln!("Last Used:  {last_used}");

                    if let Some(services) = key["allowed_service_ids"].as_array()
                        && !services.is_empty()
                    {
                        let ids: Vec<&str> = services.iter().filter_map(|v| v.as_str()).collect();
                        eprintln!("Allowed Services: {}", ids.join(", "));
                    }
                    if key["allow_all_services"].as_bool().unwrap_or(false) {
                        eprintln!("Allowed Services: all");
                    }
                    if let Some(nodes) = key["allowed_node_ids"].as_array()
                        && !nodes.is_empty()
                    {
                        let ids: Vec<&str> = nodes.iter().filter_map(|v| v.as_str()).collect();
                        eprintln!("Allowed Nodes:    {}", ids.join(", "));
                    }
                    if key["allow_all_nodes"].as_bool().unwrap_or(false) {
                        eprintln!("Allowed Nodes:    all");
                    }
                }
            }
            Ok(())
        }

        ApiKeyCommands::Rotate { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let result: Value = api
                .post(&format!("/api-keys/{id}/rotate"), &serde_json::json!({}))
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let key = result["full_key"].as_str().unwrap_or("-");
                    eprintln!("Key rotated!");
                    eprintln!("New Key: {key}  (save this -- shown only once)");
                }
            }
            Ok(())
        }

        ApiKeyCommands::Delete { id, yes, auth } => {
            if !yes {
                eprint!("Revoke API key {id}? [y/N] ");
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Cancelled.");
                    return Ok(());
                }
            }

            let mut api = ApiClient::from_auth(&auth)?;
            api.delete_empty(&format!("/api-keys/{id}")).await?;
            eprintln!("API key revoked.");
            Ok(())
        }

        ApiKeyCommands::Update {
            id,
            name,
            scopes,
            allowed_services,
            allowed_nodes,
            allow_all_services,
            allow_all_nodes,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let mut body = serde_json::Map::new();

            if let Some(name) = name {
                body.insert("name".into(), Value::String(name));
            }
            if let Some(scopes) = scopes {
                body.insert("scopes".into(), Value::String(scopes));
            }
            if let Some(services) = allowed_services {
                let ids: Vec<&str> = services.split(',').map(|s| s.trim()).collect();
                body.insert("allowed_service_ids".into(), serde_json::json!(ids));
            }
            if let Some(nodes) = allowed_nodes {
                let ids: Vec<&str> = nodes.split(',').map(|s| s.trim()).collect();
                body.insert("allowed_node_ids".into(), serde_json::json!(ids));
            }
            if let Some(v) = allow_all_services {
                body.insert("allow_all_services".into(), Value::Bool(v));
            }
            if let Some(v) = allow_all_nodes {
                body.insert("allow_all_nodes".into(), Value::Bool(v));
            }

            let _: Value = api.put(&format!("/api-keys/{id}"), &body).await?;
            eprintln!("API key updated.");
            Ok(())
        }
    }
}
