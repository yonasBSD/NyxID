use std::io::Write;

use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{NodeCommands, NodeDaemonCommands, OutputFormat};

pub async fn run(command: NodeCommands) -> Result<()> {
    match command {
        // --- User-side commands (API calls) ---
        NodeCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let nodes: Value = api.get("/nodes").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&nodes)?);
                }
                OutputFormat::Table => {
                    let items = nodes
                        .get("nodes")
                        .and_then(|v| v.as_array())
                        .or_else(|| nodes.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No nodes registered.");
                            eprintln!(
                                "Use `nyxid node register-token` to create a registration token."
                            );
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["ID", "Name", "Status", "Last Seen"]);

                        for node in items {
                            let id = node["id"].as_str().or(node["_id"].as_str()).unwrap_or("-");
                            let name = node["name"].as_str().unwrap_or("-");
                            let status = node["status"].as_str().unwrap_or("-");
                            let last_seen = node["last_heartbeat_at"].as_str().unwrap_or("-");
                            table.add_row([id, name, status, last_seen]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        NodeCommands::Show { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;

            // Try direct ID first; if 404, resolve by name
            let node: Value = match api.get_value(&format!("/nodes/{id}")).await {
                Ok(n) => n,
                Err(_) => {
                    // Try to find by name
                    let nodes: Value = api.get("/nodes").await?;
                    let items = nodes
                        .get("nodes")
                        .and_then(|v| v.as_array())
                        .or_else(|| nodes.as_array());
                    let found =
                        items.and_then(|arr| arr.iter().find(|n| n["name"].as_str() == Some(&id)));
                    match found {
                        Some(n) => {
                            let node_id =
                                n["id"].as_str().or(n["_id"].as_str()).ok_or_else(|| {
                                    anyhow::anyhow!("Node '{id}' found but has no ID")
                                })?;
                            api.get(&format!("/nodes/{node_id}")).await?
                        }
                        None => anyhow::bail!("Node '{id}' not found (tried as ID and name)"),
                    }
                }
            };

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&node)?);
                }
                OutputFormat::Table => {
                    let name = node["name"].as_str().unwrap_or("-");
                    let node_id = node["id"].as_str().or(node["_id"].as_str()).unwrap_or(&id);
                    let status = node["status"].as_str().unwrap_or("-");
                    let last_seen = node["last_heartbeat_at"].as_str().unwrap_or("-");
                    let version = node
                        .get("metadata")
                        .and_then(|m| m["agent_version"].as_str())
                        .unwrap_or("-");

                    eprintln!("Name:       {name}");
                    eprintln!("ID:         {node_id}");
                    eprintln!("Status:     {status}");
                    eprintln!("Last Seen:  {last_seen}");
                    eprintln!("Version:    {version}");

                    if let Some(metrics) = node.get("metrics") {
                        let total = metrics["total_requests"].as_u64().unwrap_or(0);
                        let success = metrics["success_count"].as_u64().unwrap_or(0);
                        let errors = metrics["error_count"].as_u64().unwrap_or(0);
                        eprintln!();
                        eprintln!("Metrics:");
                        eprintln!("  Total Requests: {total}");
                        eprintln!("  Success:        {success}");
                        eprintln!("  Errors:         {errors}");
                    }
                }
            }
            Ok(())
        }

        NodeCommands::RegisterToken { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;

            eprint!("Node name: ");
            std::io::stderr().flush()?;
            let mut name_input = String::new();
            std::io::stdin().read_line(&mut name_input)?;
            let node_name = name_input.trim();
            let node_name = if node_name.is_empty() {
                "my-node"
            } else {
                node_name
            };

            let result: Value = api
                .post(
                    "/nodes/register-token",
                    &serde_json::json!({ "name": node_name }),
                )
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let token = result["token"].as_str().unwrap_or("-");
                    let expires = result["expires_at"].as_str().unwrap_or("in 1 hour");

                    eprintln!("Registration token created.");
                    eprintln!();
                    eprintln!("Token:   {token}");
                    eprintln!("Expires: {expires}");
                    eprintln!();
                    eprintln!("Register a node:");
                    eprintln!(
                        "  nyxid node register --token {token} --url ws://<server>/api/v1/nodes/ws"
                    );
                }
            }
            Ok(())
        }

        NodeCommands::Delete { id, yes, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let resolved_id = resolve_node_id(&mut api, &id).await?;

            if !yes {
                eprint!("Delete node {id}? [y/N] ");
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Cancelled.");
                    return Ok(());
                }
            }

            api.delete_empty(&format!("/nodes/{resolved_id}")).await?;
            eprintln!("Node deleted.");
            Ok(())
        }

        NodeCommands::RotateToken { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let resolved_id = resolve_node_id(&mut api, &id).await?;
            let result: Value = api
                .post(
                    &format!("/nodes/{resolved_id}/rotate-token"),
                    &serde_json::json!({}),
                )
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let token = result["token"]
                        .as_str()
                        .or(result["auth_token"].as_str())
                        .unwrap_or("-");
                    eprintln!("Node token rotated.");
                    eprintln!("New Token: {token}  (save this -- shown only once)");
                    eprintln!();
                    eprintln!("Update the node agent configuration:");
                    eprintln!("  nyxid node rekey --auth-token {token} --signing-secret <HEX>");
                }
            }
            Ok(())
        }

        // --- Agent-side commands (local node operations) ---
        NodeCommands::Register {
            token,
            url,
            config,
            keychain,
        } => crate::node::agent::cmd_register(&token, url.as_deref(), config.as_deref(), keychain)
            .await
            .map_err(anyhow::Error::from),

        NodeCommands::Start { config, log_level } => {
            crate::node::agent::cmd_start(config.as_deref(), log_level.as_deref())
                .await
                .map_err(anyhow::Error::from)
        }

        NodeCommands::AgentStatus { config } => {
            crate::node::agent::cmd_status(config.as_deref()).map_err(anyhow::Error::from)
        }

        NodeCommands::Rekey {
            auth_token,
            signing_secret,
            config,
        } => crate::node::agent::cmd_rekey(&auth_token, &signing_secret, config.as_deref())
            .map_err(anyhow::Error::from),

        NodeCommands::Credentials { command, config } => {
            crate::node::agent::cmd_credentials(command, config.as_deref())
                .await
                .map_err(anyhow::Error::from)
        }

        NodeCommands::Migrate { to, config } => {
            crate::node::agent::cmd_migrate(&to, config.as_deref()).map_err(anyhow::Error::from)
        }

        NodeCommands::NodeOpenclaw { command, config } => {
            crate::node::agent::cmd_openclaw(command, config.as_deref())
                .await
                .map_err(anyhow::Error::from)
        }

        NodeCommands::AgentVersion => {
            crate::node::agent::cmd_version();
            Ok(())
        }

        NodeCommands::Daemon { command } => match command {
            NodeDaemonCommands::Install {
                args,
                log_level,
                force,
            } => crate::node::daemon::install(args.config.as_deref(), log_level.as_deref(), force)
                .map_err(anyhow::Error::from),
            NodeDaemonCommands::Uninstall { args } => {
                crate::node::daemon::uninstall(args.config.as_deref()).map_err(anyhow::Error::from)
            }
            NodeDaemonCommands::Start { args } => {
                crate::node::daemon::start(args.config.as_deref()).map_err(anyhow::Error::from)
            }
            NodeDaemonCommands::Stop { args } => {
                crate::node::daemon::stop(args.config.as_deref()).map_err(anyhow::Error::from)
            }
            NodeDaemonCommands::Restart { args } => {
                crate::node::daemon::restart(args.config.as_deref()).map_err(anyhow::Error::from)
            }
            NodeDaemonCommands::Status { args } => {
                crate::node::daemon::status(args.config.as_deref()).map_err(anyhow::Error::from)
            }
            NodeDaemonCommands::Logs {
                args,
                follow,
                lines,
            } => crate::node::daemon::logs(args.config.as_deref(), follow, lines)
                .map_err(anyhow::Error::from),
        },
    }
}

/// Resolve a node identifier (ID or name) to a node ID.
async fn resolve_node_id(api: &mut ApiClient, id_or_name: &str) -> Result<String> {
    // Try as UUID first (quick check)
    if id_or_name.len() == 36 && id_or_name.contains('-') {
        return Ok(id_or_name.to_string());
    }

    // List nodes and find by name
    let nodes: Value = api.get("/nodes").await?;
    let items = nodes
        .get("nodes")
        .and_then(|v| v.as_array())
        .or_else(|| nodes.as_array());

    if let Some(arr) = items
        && let Some(node) = arr.iter().find(|n| n["name"].as_str() == Some(id_or_name))
        && let Some(nid) = node["id"].as_str().or(node["_id"].as_str())
    {
        return Ok(nid.to_string());
    }

    // Fall back to treating it as an ID (let the server decide)
    Ok(id_or_name.to_string())
}
