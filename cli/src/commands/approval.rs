use std::io::Write;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{ApprovalCommands, OutputFormat};

pub async fn run(command: ApprovalCommands) -> Result<()> {
    match command {
        ApprovalCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let requests: Value = api.get("/approvals/requests").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&requests)?);
                }
                OutputFormat::Table => {
                    let items = requests
                        .get("requests")
                        .and_then(|v| v.as_array())
                        .or_else(|| requests.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No approval requests.");
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["ID", "Service", "Status", "Requester", "Created"]);

                        for req in items {
                            let id = req["id"].as_str().or(req["_id"].as_str()).unwrap_or("-");
                            let short_id = if id.len() > 8 { &id[..8] } else { id };
                            let service = req["service_name"]
                                .as_str()
                                .or(req["service_slug"].as_str())
                                .unwrap_or("-");
                            let status = req["status"].as_str().unwrap_or("-");
                            let requester = req["requester_label"]
                                .as_str()
                                .or(req["requester_type"].as_str())
                                .unwrap_or("-");
                            let created = req["created_at"].as_str().unwrap_or("-");
                            table.add_row([short_id, service, status, requester, created]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        ApprovalCommands::Show { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let req: Value = api.get(&format!("/approvals/requests/{id}")).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&req)?);
                }
                OutputFormat::Table => {
                    let req_id = req["id"].as_str().or(req["_id"].as_str()).unwrap_or(&id);
                    let service = req["service_name"]
                        .as_str()
                        .or(req["service_slug"].as_str())
                        .unwrap_or("-");
                    let status = req["status"].as_str().unwrap_or("-");
                    let requester = req["requester_label"]
                        .as_str()
                        .or(req["requester_type"].as_str())
                        .unwrap_or("-");
                    let created = req["created_at"].as_str().unwrap_or("-");
                    let summary = req["operation_summary"].as_str().unwrap_or("-");

                    eprintln!("Approval Request");
                    eprintln!();
                    eprintln!("ID:        {req_id}");
                    eprintln!("Service:   {service}");
                    eprintln!("Status:    {status}");
                    eprintln!("Requester: {requester}");
                    eprintln!("Operation: {summary}");
                    eprintln!("Created:   {created}");
                }
            }
            Ok(())
        }

        ApprovalCommands::Approve { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let body = serde_json::json!({ "decision": "approved" });
            let result: Value = api
                .post(&format!("/approvals/requests/{id}/decide"), &body)
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    eprintln!("Request {id} approved.");
                }
            }
            Ok(())
        }

        ApprovalCommands::Deny { id, reason, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let mut body = serde_json::json!({ "decision": "denied" });
            if let Some(reason) = reason {
                body["reason"] = Value::String(reason);
            }
            let result: Value = api
                .post(&format!("/approvals/requests/{id}/decide"), &body)
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    eprintln!("Request {id} denied.");
                }
            }
            Ok(())
        }

        ApprovalCommands::Grants { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let grants: Value = api.get("/approvals/grants").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&grants)?);
                }
                OutputFormat::Table => {
                    let items = grants
                        .get("grants")
                        .and_then(|v| v.as_array())
                        .or_else(|| grants.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No approval grants.");
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["ID", "Service", "Requester", "Granted", "Expires"]);

                        for grant in items {
                            let gid = grant["id"]
                                .as_str()
                                .or(grant["_id"].as_str())
                                .unwrap_or("-");
                            let short_id = if gid.len() > 8 { &gid[..8] } else { gid };
                            let service = grant["service_name"].as_str().unwrap_or("-");
                            let requester = grant["requester_label"]
                                .as_str()
                                .or(grant["requester_type"].as_str())
                                .unwrap_or("-");
                            let granted = grant["granted_at"].as_str().unwrap_or("-");
                            let expires = grant["expires_at"].as_str().unwrap_or("never");
                            table.add_row([short_id, service, requester, granted, expires]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        ApprovalCommands::RevokeGrant { id, yes, auth } => {
            if !yes {
                eprint!("Revoke grant {id}? [y/N] ");
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Cancelled.");
                    return Ok(());
                }
            }

            let mut api = ApiClient::from_auth(&auth)?;
            api.delete_empty(&format!("/approvals/grants/{id}")).await?;
            eprintln!("Grant {id} revoked.");
            Ok(())
        }

        ApprovalCommands::ServiceConfigs { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let configs: Value = api.get("/approvals/service-configs").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&configs)?);
                }
                OutputFormat::Table => {
                    let items = configs
                        .get("configs")
                        .and_then(|v| v.as_array())
                        .or_else(|| configs.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No per-service approval configurations.");
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["Service ID", "Service", "Approval Required"]);

                        for cfg in items {
                            let cid = cfg["service_id"].as_str().unwrap_or("-");
                            let short_id = if cid.len() > 8 { &cid[..8] } else { cid };
                            let service = cfg["service_name"].as_str().unwrap_or("-");
                            let require = cfg["approval_required"]
                                .as_bool()
                                .map(|b| b.to_string())
                                .unwrap_or_else(|| "-".to_string());
                            table.add_row([short_id, service, &require]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        ApprovalCommands::SetConfig {
            id,
            require_approval,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let mut body = serde_json::Map::new();

            if let Some(v) = require_approval {
                body.insert("approval_required".into(), Value::Bool(v));
            }

            if body.is_empty() {
                eprintln!("No updates specified. Use --require-approval true/false.");
                return Ok(());
            }

            let result: Value = api
                .put(&format!("/approvals/service-configs/{id}"), &body)
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    eprintln!("Approval config updated for {id}.");
                }
            }
            Ok(())
        }
    }
}
