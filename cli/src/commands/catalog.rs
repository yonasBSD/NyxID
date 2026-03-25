use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{CatalogCommands, OutputFormat};

pub async fn run(command: CatalogCommands) -> Result<()> {
    match command {
        CatalogCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let catalog: Value = api.get("/catalog").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&catalog)?);
                }
                OutputFormat::Table => {
                    eprintln!("Available Services");

                    let items = catalog
                        .get("entries")
                        .and_then(|v| v.as_array())
                        .or_else(|| catalog.as_array());
                    if let Some(items) = items {
                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["Slug", "Name", "Type", "Auth", "How to Add"]);

                        for item in items {
                            let slug = item["slug"].as_str().unwrap_or("-");
                            let name = item["name"].as_str().unwrap_or("-");
                            let svc_type = item["service_type"].as_str().unwrap_or("http");
                            let provider_type = item["provider_type"].as_str().unwrap_or("-");
                            let credential_mode =
                                item["credential_mode"].as_str().unwrap_or("admin");
                            let requires_gw =
                                item["requires_gateway_url"].as_bool().unwrap_or(false);

                            let type_label = if svc_type == "ssh" {
                                "SSH".to_string()
                            } else {
                                match provider_type {
                                    "oauth2" => "OAuth".to_string(),
                                    "device_code" => "Device Code".to_string(),
                                    "api_key" => "API Key".to_string(),
                                    _ => "HTTP".to_string(),
                                }
                            };

                            let how_to_add = if svc_type == "ssh" {
                                "nyxid service add-ssh --via-node <NODE>".to_string()
                            } else if provider_type == "oauth2" {
                                if credential_mode == "user" || credential_mode == "both" {
                                    format!(
                                        "nyxid service add {} --oauth (needs client_id/secret)",
                                        slug
                                    )
                                } else {
                                    format!("nyxid service add {} --oauth", slug)
                                }
                            } else if provider_type == "device_code" {
                                format!("nyxid service add {} --device-code", slug)
                            } else if requires_gw {
                                format!(
                                    "nyxid service add {} --endpoint-url <URL> --credential-env <VAR>",
                                    slug
                                )
                            } else {
                                format!("nyxid service add {} --credential-env <VAR>", slug)
                            };

                            table.add_row([slug, name, &type_label, provider_type, &how_to_add]);
                        }
                        eprintln!("{table}");
                    }

                    eprintln!();
                    eprintln!("Use `nyxid catalog show <slug>` for details on a specific service.");
                }
            }
            Ok(())
        }
        CatalogCommands::Show { slug, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let item: Value = api.get(&format!("/catalog/{slug}")).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&item)?);
                }
                OutputFormat::Table => {
                    let name = item["name"].as_str().unwrap_or("-");
                    let item_slug = item["slug"].as_str().unwrap_or(&slug);
                    let url = item["base_url"].as_str().unwrap_or("(none)");
                    let auth_method = item["auth_method"].as_str().unwrap_or("bearer");
                    let auth_key = item["auth_key_name"].as_str().unwrap_or("Authorization");
                    let svc_type = item["service_type"].as_str().unwrap_or("http");
                    let provider_type = item["provider_type"].as_str().unwrap_or("-");
                    let credential_mode = item["credential_mode"].as_str().unwrap_or("admin");
                    let requires_gw = item["requires_gateway_url"].as_bool().unwrap_or(false);

                    eprintln!("Service: {name}");
                    eprintln!("Slug:           {item_slug}");
                    eprintln!("Type:           {svc_type}");
                    eprintln!("Default URL:    {url}");
                    eprintln!("Auth Method:    {auth_method}");
                    eprintln!("Auth Key Name:  {auth_key}");
                    eprintln!("Provider Type:  {provider_type}");
                    eprintln!("Credential Mode: {credential_mode}");
                    if requires_gw {
                        eprintln!("Requires URL:   yes (you must provide your instance URL)");
                    }

                    if let Some(instructions) = item["api_key_instructions"].as_str() {
                        eprintln!();
                        eprintln!("How to get credentials:");
                        eprintln!("  {instructions}");
                    }
                    if let Some(api_key_url) = item["api_key_url"].as_str() {
                        eprintln!("  Get API key: {api_key_url}");
                    }
                    if let Some(docs) = item["documentation_url"].as_str() {
                        eprintln!("  Docs: {docs}");
                    }

                    eprintln!();
                    eprintln!("How to add:");

                    if svc_type == "ssh" {
                        eprintln!(
                            "  nyxid service add-ssh --label \"{name}\" --host <HOST> --via-node <NODE>"
                        );
                    } else if provider_type == "oauth2" {
                        if credential_mode == "user" || credential_mode == "both" {
                            eprintln!("  # Step 1: Set your OAuth app credentials");
                            eprintln!("  nyxid service credentials {item_slug} \\");
                            eprintln!(
                                "    --client-id-env MY_CLIENT_ID --client-secret-env MY_CLIENT_SECRET"
                            );
                            eprintln!();
                            eprintln!("  # Step 2: Connect via OAuth (opens browser)");
                            eprintln!("  nyxid service add {item_slug} --oauth");
                        } else {
                            eprintln!("  nyxid service add {item_slug} --oauth");
                        }
                    } else if provider_type == "device_code" {
                        eprintln!("  nyxid service add {item_slug} --device-code");
                    } else if requires_gw {
                        eprintln!("  nyxid service add {item_slug} \\");
                        eprintln!("    --endpoint-url <YOUR_INSTANCE_URL> \\");
                        eprintln!("    --credential-env <TOKEN_VAR>");
                    } else {
                        eprintln!("  nyxid service add {item_slug} --credential-env <API_KEY_VAR>");
                    }

                    eprintln!();
                    eprintln!("  # Or route through a node (credentials stay local):");
                    eprintln!("  nyxid service add {item_slug} --via-node <NODE_NAME>");
                }
            }
            Ok(())
        }
    }
}
