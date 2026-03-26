use std::io::Write;

use anyhow::{Context, Result, bail};
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{OutputFormat, ServiceCommands};

pub async fn run(command: ServiceCommands) -> Result<()> {
    match command {
        ServiceCommands::Add {
            slug,
            custom,
            oauth,
            device_code,
            via_node,
            endpoint_url,
            label,
            auth_method,
            auth_key_name,
            credential,
            credential_env,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            // OAuth flow
            if oauth {
                return run_oauth_add(&mut api, slug, via_node.as_deref(), &auth).await;
            }

            // Device code flow
            if device_code {
                return run_device_code_add(&mut api, slug, via_node.as_deref(), &auth).await;
            }

            let mut body = serde_json::Map::new();

            if custom {
                let label_val = match label {
                    Some(l) => l,
                    None => prompt_line("Label: ")?,
                };
                let endpoint = match endpoint_url {
                    Some(u) => u,
                    None => prompt_line("Endpoint URL: ")?,
                };
                let method = match auth_method {
                    Some(m) => m,
                    None => prompt_line_default(
                        "Auth method [bearer/header/query/path/basic]: ",
                        "bearer",
                    )?,
                };
                let key_name = match auth_key_name {
                    Some(k) => k,
                    None => prompt_line_default("Auth key name: ", "Authorization")?,
                };

                body.insert("label".into(), Value::String(label_val));
                body.insert("endpoint_url".into(), Value::String(endpoint));
                body.insert("auth_method".into(), Value::String(method));
                body.insert("auth_key_name".into(), Value::String(key_name));
            } else {
                let slug = slug.ok_or_else(|| {
                    anyhow::anyhow!("Provide a catalog slug or use --custom for a custom endpoint")
                })?;
                body.insert("service_slug".into(), Value::String(slug.clone()));

                // Fetch catalog entry to validate slug and get default label
                let catalog_entry = api.get_value(&format!("/catalog/{slug}")).await;
                let catalog_name = match &catalog_entry {
                    Ok(entry) => entry["name"].as_str().map(|s| s.to_string()),
                    Err(_) => None,
                };

                if catalog_name.is_none() {
                    eprintln!("Service '{slug}' not found in catalog.");
                    eprintln!();
                    eprintln!("Available options:");
                    eprintln!("  nyxid catalog list              # browse available services");
                    eprintln!("  nyxid service add --custom      # add a custom endpoint");
                    eprintln!("  nyxid service add-ssh            # add an SSH service");
                    bail!(
                        "Catalog service '{slug}' not found. Use --custom for a custom endpoint."
                    );
                }

                let lbl = label.unwrap_or_else(|| catalog_name.unwrap_or_else(|| slug.clone()));
                body.insert("label".into(), Value::String(lbl));

                if let Some(url) = endpoint_url {
                    body.insert("endpoint_url".into(), Value::String(url));
                }
                if let Some(method) = auth_method {
                    body.insert("auth_method".into(), Value::String(method));
                }
                if let Some(key_name) = auth_key_name {
                    body.insert("auth_key_name".into(), Value::String(key_name));
                }
            }

            if let Some(ref node) = via_node {
                body.insert("node_id".into(), Value::String(node.clone()));
            }

            // Resolve credential: --credential flag > --credential-env > interactive prompt
            // Skip if routing through a node (credentials live on the node)
            if !body.contains_key("node_id") {
                let cred_value = if let Some(c) = credential {
                    c
                } else if let Some(env_var) = &credential_env {
                    std::env::var(env_var)
                        .with_context(|| format!("Environment variable {env_var} not set"))?
                } else {
                    rpassword::prompt_password("Enter API key/credential: ")?
                };
                if cred_value.is_empty() {
                    bail!(
                        "Credential is required. Pass --credential, --credential-env, or enter interactively."
                    );
                }
                body.insert("credential".into(), Value::String(cred_value));
            }

            let result: Value = api.post("/keys", &body).await?;
            print_add_result(&api, &result, auth.output)?;
            if let Some(node_id) = via_node.as_deref() {
                let result_slug = result["slug"]
                    .as_str()
                    .or(result["service_slug"].as_str())
                    .unwrap_or("-");
                eprintln!();
                eprintln!("Next step: configure the credential on node {node_id}.");
                if custom {
                    eprintln!(
                        "  Run `nyxid node credentials add ... --service {result_slug}` on that node."
                    );
                } else {
                    eprintln!(
                        "  Run `nyxid node credentials setup --service {result_slug}` on that node."
                    );
                }
            }
            Ok(())
        }

        ServiceCommands::AddSsh {
            label,
            host,
            port,
            cert_auth,
            principals,
            ttl,
            via_node,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let principals_str = principals.as_deref().unwrap_or("");
            let principal_list: Vec<&str> = principals_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let body = serde_json::json!({
                "label": label,
                "ssh_host": host,
                "ssh_port": port,
                "ssh_certificate_auth": cert_auth,
                "ssh_principals": principals_str,
                "ssh_certificate_ttl_minutes": ttl,
                "node_id": via_node,
            });

            let result: Value = api.post("/keys", &body).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let slug = result["slug"]
                        .as_str()
                        .or(result["service_slug"].as_str())
                        .unwrap_or("-");
                    let svc_id = result["id"]
                        .as_str()
                        .or(result["_id"].as_str())
                        .unwrap_or("-");

                    eprintln!("SSH service added!");
                    eprintln!();
                    eprintln!("Slug:       {slug}");
                    eprintln!("ID:         {svc_id}");
                    eprintln!("Host:       {host}:{port}");
                    eprintln!("Node:       {via_node}");
                    if cert_auth {
                        eprintln!();
                        eprintln!("SSH Setup Instructions:");
                        eprintln!("  1. Download the CA public key:");
                        eprintln!("     nyxid ssh issue-cert --service-id {svc_id} \\");
                        eprintln!("       --public-key-file ~/.ssh/id_ed25519.pub \\");
                        eprintln!(
                            "       --principal {} \\",
                            principal_list.first().unwrap_or(&"ubuntu")
                        );
                        eprintln!("       --certificate-file ~/.ssh/id_ed25519-cert.pub \\");
                        eprintln!("       --ca-public-key-file /tmp/nyxid_ca.pub");
                        eprintln!();
                        eprintln!("  2. On the target server, add to /etc/ssh/sshd_config:");
                        eprintln!("     TrustedUserCAKeys /etc/ssh/nyxid_ca.pub");
                        eprintln!();
                        eprintln!("  3. Copy the CA public key to the server:");
                        eprintln!("     scp /tmp/nyxid_ca.pub {host}:/etc/ssh/nyxid_ca.pub");
                        eprintln!();
                        eprintln!("  4. Restart sshd on the target server");
                    }
                }
            }
            Ok(())
        }

        ServiceCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let resp: Value = api.get("/keys").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
                OutputFormat::Table => {
                    let items = resp.get("keys").and_then(|v| v.as_array());
                    if let Some(items) = items {
                        if items.is_empty() {
                            eprintln!("No services configured.");
                            eprintln!(
                                "Use `nyxid service add <slug>` or `nyxid catalog list` to get started."
                            );
                            return Ok(());
                        }

                        let mut table = Table::new();
                        table.load_preset(UTF8_FULL_CONDENSED);
                        table.set_header(["ID", "Slug", "Label", "Endpoint", "Status", "Node"]);

                        for svc in items {
                            let id = svc["id"].as_str().or(svc["_id"].as_str()).unwrap_or("-");
                            let slug = svc["slug"]
                                .as_str()
                                .or(svc["service_slug"].as_str())
                                .unwrap_or("-");
                            let label = svc["label"].as_str().unwrap_or("-");
                            let endpoint = svc["endpoint_url"].as_str().unwrap_or("-");
                            let status = svc["status"].as_str().unwrap_or("active");
                            let node = svc["node_id"].as_str().unwrap_or("--");
                            table.add_row([id, slug, label, endpoint, status, node]);
                        }
                        eprintln!("{table}");
                    }
                }
            }
            Ok(())
        }

        ServiceCommands::Show { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let svc: Value = api.get(&format!("/keys/{id}")).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&svc)?);
                }
                OutputFormat::Table => {
                    let name = svc["label"]
                        .as_str()
                        .or(svc["name"].as_str())
                        .unwrap_or("-");
                    let slug = svc["slug"]
                        .as_str()
                        .or(svc["service_slug"].as_str())
                        .unwrap_or("-");
                    let svc_id = svc["id"].as_str().or(svc["_id"].as_str()).unwrap_or(&id);
                    let status = svc["status"].as_str().unwrap_or("active");
                    let endpoint = svc["endpoint_url"].as_str().unwrap_or("-");
                    let auth_method = svc["auth_method"].as_str().unwrap_or("-");
                    let auth_key = svc["auth_key_name"].as_str().unwrap_or("-");
                    let node = svc["node_id"].as_str().unwrap_or("-- (direct)");
                    let svc_type = svc["service_type"].as_str().unwrap_or("http");
                    let is_active = svc["is_active"].as_bool().unwrap_or(true);

                    eprintln!("Service: {name} ({slug})");
                    eprintln!("ID:         {svc_id}");
                    eprintln!("Type:       {svc_type}");
                    eprintln!("Status:     {status}");
                    eprintln!("Active:     {is_active}");
                    eprintln!();
                    eprintln!("Endpoint:   {endpoint}");
                    eprintln!("Auth:       {auth_method} / {auth_key}");
                    eprintln!("Node:       {node}");

                    // SSH-specific fields
                    if svc_type == "ssh" {
                        let ssh_host = svc["ssh_host"].as_str().unwrap_or("-");
                        let ssh_port = svc["ssh_port"].as_u64().unwrap_or(22);
                        let cert_auth = svc["ssh_ca_public_key"].is_string();
                        let ttl = svc["ssh_certificate_ttl_minutes"].as_u64().unwrap_or(30);
                        let principals = svc["ssh_allowed_principals"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "-".to_string());

                        eprintln!();
                        eprintln!("SSH Host:       {ssh_host}:{ssh_port}");
                        eprintln!("Cert Auth:      {cert_auth}");
                        eprintln!("Principals:     {principals}");
                        eprintln!("Cert TTL:       {ttl} min");
                    }

                    eprintln!();
                    eprintln!("Proxy URL:  {}/api/v1/proxy/s/{slug}/", api.base_url_root());
                }
            }
            Ok(())
        }

        ServiceCommands::Delete { id, yes, auth } => {
            if !yes {
                eprint!("Delete service {id}? [y/N] ");
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Cancelled.");
                    return Ok(());
                }
            }

            let mut api = ApiClient::from_auth(&auth)?;
            api.delete_empty(&format!("/keys/{id}")).await?;
            eprintln!("Service deleted.");
            Ok(())
        }

        ServiceCommands::Update {
            id,
            label,
            endpoint_url,
            node_id,
            no_node,
            active,
            inactive,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let mut body = serde_json::Map::new();

            if let Some(l) = label {
                body.insert("label".into(), Value::String(l));
            }
            if let Some(url) = endpoint_url {
                body.insert("endpoint_url".into(), Value::String(url));
            }
            if no_node {
                body.insert("node_id".into(), Value::String(String::new()));
            } else if let Some(nid) = node_id {
                body.insert("node_id".into(), Value::String(nid));
            }
            if active {
                body.insert("is_active".into(), Value::Bool(true));
            } else if inactive {
                body.insert("is_active".into(), Value::Bool(false));
            }

            let _: Value = api
                .put(&format!("/keys/{id}"), &Value::Object(body))
                .await?;

            eprintln!("Service updated.");
            Ok(())
        }

        ServiceCommands::RotateCredential {
            id,
            credential_env,
            credential,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            // Fetch service to get the external api_key_id
            let svc: Value = api.get(&format!("/keys/{id}")).await?;
            let api_key_id = svc["api_key_id"]
                .as_str()
                .or(svc["external_key_id"].as_str())
                .ok_or_else(|| anyhow::anyhow!("No external credential found for this service"))?;
            if svc["credential_type"].as_str() == Some("node_managed") {
                bail!("This service is node-managed. Update the credential on the node instead.");
            }

            let credential = if let Some(c) = credential {
                c
            } else if let Some(env_var) = &credential_env {
                std::env::var(env_var)
                    .with_context(|| format!("Environment variable {env_var} not set"))?
            } else {
                rpassword::prompt_password("New credential: ")
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            };
            if credential.is_empty() {
                bail!("Credential is required");
            }

            let body = serde_json::json!({ "credential": credential });
            let _: Value = api
                .put(&format!("/api-keys/external/{api_key_id}"), &body)
                .await?;

            eprintln!("Credential rotated for service {id}.");
            Ok(())
        }

        ServiceCommands::Route {
            id,
            node,
            direct,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let svc: Value = api.get(&format!("/keys/{id}")).await?;
            let service_id = svc["user_service_id"]
                .as_str()
                .or(svc["id"].as_str())
                .or(svc["_id"].as_str())
                .ok_or_else(|| anyhow::anyhow!("Could not resolve user_service_id"))?;

            let node_value = if direct {
                Value::Null
            } else if let Some(node_id) = node {
                Value::String(node_id)
            } else {
                bail!("Specify --node <NODE_ID> or --direct");
            };

            let body = serde_json::json!({ "node_id": node_value });
            let _: Value = api
                .put(&format!("/user-services/{service_id}"), &body)
                .await?;

            if direct {
                eprintln!("Service {id} set to direct routing.");
            } else {
                eprintln!("Service {id} routed through node.");
            }
            Ok(())
        }

        ServiceCommands::Credentials {
            slug,
            client_id_env,
            client_id,
            client_secret_env,
            client_secret,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            // Fetch catalog entry to get provider_id
            let catalog: Value = api.get(&format!("/catalog/{slug}")).await?;
            let provider_id = catalog["provider_config_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Could not find provider_id for slug: {slug}"))?;

            let client_id = if let Some(c) = client_id {
                c
            } else if let Some(env_var) = &client_id_env {
                std::env::var(env_var)
                    .with_context(|| format!("Environment variable {env_var} not set"))?
            } else {
                let mut input = String::new();
                eprint!("OAuth client ID: ");
                std::io::stderr().flush()?;
                std::io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            };
            if client_id.is_empty() {
                bail!("Client ID is required");
            }

            let client_secret = if let Some(c) = client_secret {
                c
            } else if let Some(env_var) = &client_secret_env {
                std::env::var(env_var)
                    .with_context(|| format!("Environment variable {env_var} not set"))?
            } else {
                rpassword::prompt_password("OAuth client secret: ")?
            };
            if client_secret.is_empty() {
                bail!("Client secret is required");
            }

            let body = serde_json::json!({
                "client_id": client_id,
                "client_secret": client_secret,
            });

            let result: Value = api
                .put(&format!("/providers/{provider_id}/credentials"), &body)
                .await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    eprintln!("OAuth credentials set for {slug}.");
                }
            }
            Ok(())
        }
    }
}

// ---- OAuth add flow (I21) ----

async fn run_oauth_add(
    api: &mut ApiClient,
    slug: Option<String>,
    via_node: Option<&str>,
    auth: &crate::cli::AuthArgs,
) -> Result<()> {
    let slug = slug.ok_or_else(|| anyhow::anyhow!("Catalog slug is required for --oauth"))?;

    // Fetch catalog to get provider info and create a placeholder unified key first.
    let catalog: Value = api.get(&format!("/catalog/{slug}")).await?;
    let label = catalog["name"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| slug.clone());

    let key_body = if let Some(node_id) = via_node {
        serde_json::json!({
            "service_slug": slug,
            "label": label,
            "node_id": node_id,
        })
    } else {
        serde_json::json!({
            "service_slug": slug,
            "label": label,
        })
    };
    let key_result: Value = api.post("/keys", &key_body).await?;
    let key_id = key_result["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Created key response did not include an id"))?;

    if via_node.is_some() {
        print_add_result(api, &key_result, auth.output)?;
        eprintln!();
        eprintln!("Next step: run this on the node that owns the credential:");
        eprintln!("  nyxid node credentials setup --service {slug}");
        return Ok(());
    }

    let provider_id = catalog["provider_config_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No provider found for slug: {slug}"))?;
    let redirect_path = format!("/keys/{key_id}");
    let initiate: Value = api
        .get(&format!(
            "/providers/{provider_id}/connect/oauth?redirect_path={}",
            urlencoding::encode(&redirect_path)
        ))
        .await?;
    let authorization_url = initiate["authorization_url"].as_str().ok_or_else(|| {
        anyhow::anyhow!("OAuth initiate response did not include authorization_url")
    })?;

    eprintln!("Opening browser for OAuth authorization...");
    eprintln!();
    eprintln!("If the browser does not open, visit:");
    eprintln!("  {authorization_url}");
    eprintln!();

    let _ = open::that(authorization_url);

    let final_key = wait_for_authorized_key(api, key_id).await?;
    print_add_result(api, &final_key, auth.output)?;

    Ok(())
}

// ---- Device code add flow (I22) ----

async fn run_device_code_add(
    api: &mut ApiClient,
    slug: Option<String>,
    via_node: Option<&str>,
    auth: &crate::cli::AuthArgs,
) -> Result<()> {
    let slug = slug.ok_or_else(|| anyhow::anyhow!("Catalog slug is required for --device-code"))?;

    // Fetch catalog to get provider info and create a placeholder unified key first.
    let catalog: Value = api.get(&format!("/catalog/{slug}")).await?;
    let label = catalog["name"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| slug.clone());
    let key_body = if let Some(node_id) = via_node {
        serde_json::json!({
            "service_slug": slug,
            "label": label,
            "node_id": node_id,
        })
    } else {
        serde_json::json!({
            "service_slug": slug,
            "label": label,
        })
    };
    let key_result: Value = api.post("/keys", &key_body).await?;
    let key_id = key_result["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Created key response did not include an id"))?;

    if via_node.is_some() {
        print_add_result(api, &key_result, auth.output)?;
        eprintln!();
        eprintln!("Next step: run this on the node that owns the credential:");
        eprintln!("  nyxid node credentials setup --service {slug}");
        return Ok(());
    }

    let provider_id = catalog["provider_config_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No provider found for slug: {slug}"))?;

    // Initiate device code flow
    let initiate: Value = api
        .post(
            &format!("/providers/{provider_id}/connect/device-code/initiate"),
            &serde_json::json!({}),
        )
        .await?;

    let user_code = initiate["user_code"].as_str().unwrap_or("-");
    let verification_uri = initiate["verification_uri"]
        .as_str()
        .or(initiate["verification_url"].as_str())
        .unwrap_or("-");
    let state = initiate["state"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Device code initiate response did not include state"))?;
    let mut interval = initiate["interval"]
        .as_u64()
        .or_else(|| initiate["interval"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(5);

    eprintln!("Device Code Authorization");
    eprintln!();
    eprintln!("  Code: {user_code}");
    eprintln!("  URL:  {verification_uri}");
    eprintln!();
    eprintln!("Enter the code at the URL above, then wait for authorization...");

    let _ = open::that(verification_uri);

    // Poll for completion
    let poll_body = serde_json::json!({ "state": state });

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        let poll_path = format!("/providers/{provider_id}/connect/device-code/poll");
        match api.post::<Value, _>(&poll_path, &poll_body).await {
            Ok(result) => {
                let status = result["status"].as_str().unwrap_or("");
                if status == "complete"
                    || status == "authorized"
                    || result["access_token"].is_string()
                {
                    eprintln!("Authorization successful!");
                    eprintln!();
                    let key_result: Value = api.get(&format!("/keys/{key_id}")).await?;

                    match auth.output {
                        crate::cli::OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&key_result)?);
                        }
                        crate::cli::OutputFormat::Table => {
                            print_add_result(api, &key_result, auth.output)?;
                        }
                    }
                    return Ok(());
                }
                if status == "expired" {
                    bail!("Device code authentication expired before completion");
                }
                if status == "denied" {
                    bail!("Device code authentication was denied");
                }
                if status == "slow_down" {
                    interval = result["interval"]
                        .as_u64()
                        .or_else(|| result["interval"].as_str().and_then(|s| s.parse().ok()))
                        .unwrap_or(interval + 5);
                }
                // Still pending, continue polling
                eprint!(".");
                std::io::stderr().flush()?;
            }
            Err(_) => {
                // Treat errors during polling as "still pending"
                eprint!(".");
                std::io::stderr().flush()?;
            }
        }
    }
}

async fn wait_for_authorized_key(api: &mut ApiClient, key_id: &str) -> Result<Value> {
    const MAX_ATTEMPTS: usize = 150;
    const POLL_INTERVAL_SECS: u64 = 2;

    eprintln!("Waiting for authorization to complete...");

    for _ in 0..MAX_ATTEMPTS {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        let key: Value = api.get(&format!("/keys/{key_id}")).await?;
        let status = key["status"].as_str().unwrap_or("");
        match status {
            "pending_auth" => {
                eprint!(".");
                std::io::stderr().flush()?;
            }
            "active" => {
                eprintln!();
                return Ok(key);
            }
            other => {
                eprintln!();
                bail!("OAuth flow finished with unexpected key status: {other}");
            }
        }
    }

    eprintln!();
    bail!("Timed out waiting for OAuth authorization to complete")
}

fn print_add_result(api: &ApiClient, result: &Value, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(result)?);
        }
        OutputFormat::Table => {
            let slug = result["slug"]
                .as_str()
                .or(result["service_slug"].as_str())
                .unwrap_or("-");
            let endpoint = result["endpoint_url"].as_str().unwrap_or("-");
            let status = result["status"].as_str().unwrap_or("active");

            eprintln!("Service added successfully!");
            eprintln!();
            eprintln!("Slug:      {slug}");
            eprintln!("Endpoint:  {endpoint}");
            eprintln!("Status:    {status}");
            eprintln!();
            eprintln!("Proxy URL: {}/api/v1/proxy/s/{slug}/", api.base_url_root());
        }
    }
    Ok(())
}

fn prompt_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        bail!("Input is required");
    }
    Ok(trimmed)
}

fn prompt_line_default(prompt: &str, default: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}
