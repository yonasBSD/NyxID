mod cli;
mod config;
mod credential_store;
mod encryption;
mod error;
mod keychain;
mod metrics;
mod oauth;
mod proxy_executor;
mod secret_backend;
mod signing;
mod ws_client;

use std::path::Path;

use base64::Engine as _;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use std::time::Duration;

use crate::cli::{Cli, Commands, CredentialCommands, CredentialSecretFormat, OpenClawCommands};
use crate::config::NodeConfig;
use crate::credential_store::{CredentialStore, SharedCredentials, SharedCredentialsSender};
use crate::error::Result;
use crate::secret_backend::SecretBackend;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let log_level = cli.log_level.as_deref().unwrap_or("info");
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)),
        )
        .init();

    if let Err(e) = run(cli).await {
        tracing::error!(error = %e, "Fatal error");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Register {
            token,
            url,
            config: config_path,
            keychain,
        } => cmd_register(&token, url.as_deref(), config_path.as_deref(), keychain).await,
        Commands::Start {
            config: config_path,
        } => cmd_start(config_path.as_deref()).await,
        Commands::Status {
            config: config_path,
        } => cmd_status(config_path.as_deref()),
        Commands::Rekey {
            auth_token,
            signing_secret,
            config: config_path,
        } => cmd_rekey(&auth_token, &signing_secret, config_path.as_deref()),
        Commands::Credentials {
            command,
            config: config_path,
        } => cmd_credentials(command, config_path.as_deref()).await,
        Commands::Migrate {
            to,
            config: config_path,
        } => cmd_migrate(&to, config_path.as_deref()),
        Commands::Openclaw {
            command,
            config: config_path,
        } => cmd_openclaw(command, config_path.as_deref()).await,
        Commands::Version => {
            println!("nyxid-node {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

async fn cmd_register(
    token: &str,
    url: Option<&str>,
    config_path: Option<&str>,
    use_keychain: bool,
) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    std::fs::create_dir_all(&config_dir)?;
    let backend_name = if use_keychain { "keychain" } else { "file" };

    SecretBackend::preflight(backend_name, &config_dir)?;

    // M2: Use ws:// for localhost (wss:// requires TLS not available in dev)
    let ws_url = url.unwrap_or("ws://localhost:3001/api/v1/nodes/ws");

    tracing::info!(url = %ws_url, "Registering node...");

    let (node_id, auth_token, signing_secret) = ws_client::register_node(ws_url, token).await?;

    tracing::info!(node_id = %node_id, "Registration successful");

    let backend = SecretBackend::new(backend_name, &node_id, &config_dir)?;

    let mut config = NodeConfig::new(ws_url.to_string(), node_id, backend_name.to_string());
    backend.store_auth_token(&mut config, &auth_token)?;
    if let Some(secret) = signing_secret {
        backend.store_signing_secret(&mut config, &secret)?;
    }

    let config_file = config_dir.join("config.toml");
    config.save(&config_file)?;

    tracing::info!(path = %config_file.display(), "Configuration saved");
    println!("Node registered successfully.");
    println!("  Node ID:  {}", config.node.id);
    println!("  Storage:  {backend_name}");
    println!("  Config:   {}", config_file.display());
    println!();
    println!("Start the agent with:");
    println!("  nyxid-node start");

    Ok(())
}

async fn cmd_start(config_path: Option<&str>) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    let config = NodeConfig::load(&config_file)?;
    let backend = SecretBackend::from_config(&config, &config_dir)?;

    let auth_token = backend.load_auth_token(&config)?;
    let signing_secret = backend.load_signing_secret(&config)?;
    let credentials = CredentialStore::from_config_with_backend(&config, &backend)?;

    let (cred_sender, shared_creds) = SharedCredentials::new(credentials);
    let cred_sender = std::sync::Arc::new(cred_sender);

    let backend = std::sync::Arc::new(backend);

    tracing::info!(
        node_id = %config.node.id,
        server = %config.server.url,
        storage = %config.storage_backend,
        credentials = shared_creds.snapshot().count(),
        "Starting node agent"
    );

    // Spawn background task that reloads credentials when config file changes
    let reload_handle = tokio::spawn(credential_reload_loop(
        config_file.clone(),
        cred_sender.clone(),
        Duration::from_secs(5),
        std::sync::Arc::clone(&backend),
    ));

    // Spawn OAuth refresh loop (check every 60 seconds)
    let refresh_handle = tokio::spawn(oauth_refresh_loop(
        config_file.clone(),
        Duration::from_secs(60),
        std::sync::Arc::clone(&backend),
    ));

    ws_client::run_with_shutdown(
        config,
        config_file,
        auth_token,
        signing_secret,
        shared_creds,
        cred_sender,
        backend,
    )
    .await;

    reload_handle.abort();
    refresh_handle.abort();
    Ok(())
}

/// Poll the config file mtime and reload credentials when it changes.
async fn credential_reload_loop(
    config_file: std::path::PathBuf,
    sender: std::sync::Arc<SharedCredentialsSender>,
    interval: Duration,
    backend: std::sync::Arc<SecretBackend>,
) {
    let mut last_modified = std::fs::metadata(&config_file)
        .and_then(|m| m.modified())
        .ok();

    loop {
        tokio::time::sleep(interval).await;

        let current_modified = match std::fs::metadata(&config_file).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to stat config file for credential reload");
                continue;
            }
        };

        if Some(current_modified) == last_modified {
            continue;
        }

        last_modified = Some(current_modified);

        let config = match NodeConfig::load(&config_file) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to reload config, keeping existing credentials");
                continue;
            }
        };

        match CredentialStore::from_config_with_backend(&config, &backend) {
            Ok(new_store) => {
                let count = new_store.count();
                sender.update(new_store);
                tracing::info!(credentials = count, "Credentials reloaded from config");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to reload credentials, keeping existing");
            }
        }
    }
}

fn cmd_status(config_path: Option<&str>) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    let config = NodeConfig::load(&config_file)?;
    let backend = SecretBackend::from_config(&config, &config_dir)?;
    let credentials = CredentialStore::from_config_with_backend(&config, &backend)?;

    println!("Node Status");
    println!("  Node ID:     {}", config.node.id);
    println!("  Server:      {}", config.server.url);
    println!("  Storage:     {}", config.storage_backend);
    println!("  Credentials: {} configured", credentials.count());

    for slug in credentials.service_slugs() {
        println!("    - {slug}");
    }

    Ok(())
}

fn cmd_rekey(auth_token: &str, signing_secret: &str, config_path: Option<&str>) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    let mut config = NodeConfig::load(&config_file)?;
    let backend = SecretBackend::from_config(&config, &config_dir)?;

    backend.store_auth_token(&mut config, auth_token)?;
    backend.store_signing_secret(&mut config, signing_secret)?;
    config.save(&config_file)?;

    println!("Node credentials updated.");
    println!("Restart the agent to reconnect with the rotated credentials.");
    Ok(())
}

fn cmd_migrate(target_backend: &str, config_path: Option<&str>) -> Result<()> {
    if target_backend != "keychain" && target_backend != "file" {
        return Err(crate::error::Error::Validation(
            "Target must be 'keychain' or 'file'".to_string(),
        ));
    }

    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    let mut config = NodeConfig::load(&config_file)?;
    let source_backend = config.storage_backend.clone();

    if config.storage_backend == target_backend {
        println!("Already using '{target_backend}' storage. Nothing to migrate.");
        return Ok(());
    }

    let source = SecretBackend::from_config(&config, &config_dir)?;
    let target = SecretBackend::new(target_backend, &config.node.id, &config_dir)?;
    let report = migrate_config(&mut config, &source, &target, target_backend, &config_file)?;

    println!("Migrated from '{source_backend}' to '{target_backend}'.");
    println!("Restart the agent to use the new storage backend.");
    for warning in report.cleanup_warnings {
        eprintln!("Warning: {warning}");
    }
    Ok(())
}

#[derive(Debug, Default)]
struct MigrationReport {
    cleanup_warnings: Vec<String>,
}

fn migrate_config(
    config: &mut NodeConfig,
    source: &SecretBackend,
    target: &SecretBackend,
    target_backend: &str,
    config_file: &Path,
) -> Result<MigrationReport> {
    let auth_token = source.load_auth_token(config)?;
    let signing_secret = source.load_signing_secret(config)?;

    let mut credential_values = Vec::new();
    for (slug, cred_config) in &config.credentials {
        let value = source.load_credential_value(
            slug,
            cred_config
                .header_value_encrypted
                .as_deref()
                .or(cred_config.param_value_encrypted.as_deref()),
        )?;
        credential_values.push((slug.clone(), cred_config.injection_method.clone(), value));
    }

    let mut updated = config.clone();
    target.store_auth_token(&mut updated, &auth_token)?;
    if let Some(ref secret) = signing_secret {
        target.store_signing_secret(&mut updated, secret)?;
    }

    for (slug, injection_method, value) in &credential_values {
        let encrypted = target.store_credential_value(slug, value)?;
        if let Some(cred_config) = updated.credentials.get_mut(slug) {
            match injection_method.as_str() {
                "header" => cred_config.header_value_encrypted = encrypted,
                "query_param" => cred_config.param_value_encrypted = encrypted,
                _ => {}
            }
        }
    }

    updated.storage_backend = target_backend.to_string();
    if let Err(err) = updated.save(config_file) {
        rollback_target_secrets(target, &credential_values);
        return Err(err);
    }

    let cleanup_warnings = cleanup_source_secrets(source, &credential_values);
    *config = updated;
    Ok(MigrationReport { cleanup_warnings })
}

fn rollback_target_secrets(target: &SecretBackend, credential_values: &[(String, String, String)]) {
    let _ = target.delete_auth_token();
    let _ = target.delete_signing_secret();
    for (slug, _, _) in credential_values {
        let _ = target.delete_credential(slug);
    }
}

fn cleanup_source_secrets(
    source: &SecretBackend,
    credential_values: &[(String, String, String)],
) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Err(err) = source.delete_auth_token() {
        warnings.push(format!("Failed to remove old auth token: {err}"));
    }
    if let Err(err) = source.delete_signing_secret() {
        warnings.push(format!("Failed to remove old signing secret: {err}"));
    }
    for (slug, _, _) in credential_values {
        if let Err(err) = source.delete_credential(slug) {
            warnings.push(format!(
                "Failed to remove old credential '{slug}' from the previous backend: {err}"
            ));
        }
    }

    warnings
}

async fn cmd_credentials(command: CredentialCommands, config_path: Option<&str>) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    match command {
        CredentialCommands::Add {
            service,
            url,
            header,
            query_param,
            secret_format,
            value,
        } => {
            let mut config = NodeConfig::load(&config_file)?;
            let backend = SecretBackend::from_config(&config, &config_dir)?;

            if let Some(header_name) = header {
                // Support legacy inline format "Name: value" for backwards compat
                if header_name.contains(':') {
                    if value.is_some() {
                        return Err(crate::error::Error::Validation(
                            "Use either --header Name with a prompted/inline secret, or the legacy --header 'Name: value' form"
                                .to_string(),
                        ));
                    }
                    if secret_format != CredentialSecretFormat::Raw {
                        return Err(crate::error::Error::Validation(
                            "Legacy 'Name: value' input cannot be combined with --secret-format"
                                .to_string(),
                        ));
                    }
                    let (name, val) = parse_header(&header_name)?;
                    config.add_header_credential_via(
                        &service,
                        &name,
                        &val,
                        url.as_deref(),
                        &backend,
                    )?;
                } else {
                    let secret = read_secret_value(
                        value,
                        &format!("Enter value for header '{header_name}'"),
                    )?;
                    let secret = format_secret_value(secret, secret_format)?;
                    config.add_header_credential_via(
                        &service,
                        &header_name,
                        &secret,
                        url.as_deref(),
                        &backend,
                    )?;
                }
            } else if let Some(param_name) = query_param {
                // Support legacy inline format "name=value" for backwards compat
                if param_name.contains('=') {
                    if value.is_some() {
                        return Err(crate::error::Error::Validation(
                            "Use either --query-param name with a prompted/inline secret, or the legacy --query-param 'name=value' form"
                                .to_string(),
                        ));
                    }
                    if secret_format != CredentialSecretFormat::Raw {
                        return Err(crate::error::Error::Validation(
                            "Legacy 'name=value' input cannot be combined with --secret-format"
                                .to_string(),
                        ));
                    }
                    let (name, val) = parse_query_param(&param_name)?;
                    config.add_query_param_credential_via(
                        &service,
                        &name,
                        &val,
                        url.as_deref(),
                        &backend,
                    )?;
                } else {
                    if secret_format != CredentialSecretFormat::Raw {
                        return Err(crate::error::Error::Validation(
                            "--secret-format bearer/basic is only supported with --header"
                                .to_string(),
                        ));
                    }
                    let secret = read_secret_value(
                        value,
                        &format!("Enter value for query param '{param_name}'"),
                    )?;
                    config.add_query_param_credential_via(
                        &service,
                        &param_name,
                        &secret,
                        url.as_deref(),
                        &backend,
                    )?;
                }
            } else {
                // Interactive mode: prompt for all values
                let method = prompt_choice("Auth method", &["header", "query_param"], "header")?;

                if method == "header" {
                    let header_name = prompt_string("Header name", "Authorization")?;

                    let fmt_str =
                        prompt_choice("Secret format", &["raw", "bearer", "basic"], "raw")?;
                    let fmt = match fmt_str.as_str() {
                        "bearer" => CredentialSecretFormat::Bearer,
                        "basic" => CredentialSecretFormat::Basic,
                        _ => CredentialSecretFormat::Raw,
                    };

                    let secret = read_secret_value(
                        value,
                        &format!("Enter value for header '{header_name}'"),
                    )?;
                    let secret = format_secret_value(secret, fmt)?;

                    let effective_url = match url {
                        Some(u) => Some(u),
                        None => {
                            let input = prompt_string_optional(
                                "Endpoint URL (optional, press Enter to skip)",
                            )?;
                            if input.is_empty() { None } else { Some(input) }
                        }
                    };

                    config.add_header_credential_via(
                        &service,
                        &header_name,
                        &secret,
                        effective_url.as_deref(),
                        &backend,
                    )?;
                } else {
                    let param_name = prompt_string("Query param name", "api_key")?;

                    let secret = read_secret_value(
                        value,
                        &format!("Enter value for query param '{param_name}'"),
                    )?;

                    let effective_url = match url {
                        Some(u) => Some(u),
                        None => {
                            let input = prompt_string_optional(
                                "Endpoint URL (optional, press Enter to skip)",
                            )?;
                            if input.is_empty() { None } else { Some(input) }
                        }
                    };

                    config.add_query_param_credential_via(
                        &service,
                        &param_name,
                        &secret,
                        effective_url.as_deref(),
                        &backend,
                    )?;
                }
            }

            config.save(&config_file)?;
            println!("Credential added for service '{service}'.");
            Ok(())
        }
        CredentialCommands::Setup {
            service,
            api_url,
            access_token,
        } => {
            cmd_credentials_setup(
                &config_file,
                &config_dir,
                &service,
                api_url.as_deref(),
                access_token.as_deref(),
            )
            .await
        }

        CredentialCommands::List => {
            let config = NodeConfig::load(&config_file)?;
            let backend = SecretBackend::from_config(&config, &config_dir)?;
            let creds = CredentialStore::from_config_with_backend(&config, &backend)?;

            if creds.count() == 0 {
                println!("No credentials configured.");
            } else {
                println!("Configured credentials:");
                for slug in creds.service_slugs() {
                    if let Some(cred) = creds.get(&slug) {
                        println!(
                            "  {slug}: {} ({})",
                            cred.injection_method(),
                            cred.target_name()
                        );
                    }
                }
            }
            Ok(())
        }
        CredentialCommands::AddOauth {
            service,
            from_catalog,
            client_id,
            client_secret,
            authorization_url,
            token_url,
            device_code_url,
            scopes,
            url,
            api_url,
            access_token,
        } => {
            cmd_credentials_add_oauth(
                &config_file,
                &config_dir,
                &service,
                from_catalog,
                client_id,
                client_secret,
                authorization_url,
                token_url,
                device_code_url,
                scopes,
                url,
                api_url,
                access_token,
            )
            .await
        }
        CredentialCommands::Remove { service } => {
            let mut config = NodeConfig::load(&config_file)?;
            let backend = SecretBackend::from_config(&config, &config_dir)?;
            config.remove_credential_via(&service, &backend)?;
            config.save(&config_file)?;
            println!("Credential removed for service '{service}'.");
            Ok(())
        }
    }
}

fn read_secret_value(value: Option<String>, prompt: &str) -> Result<String> {
    let value = match value {
        Some(v) => v,
        None => prompt_secret(prompt)?,
    };
    if value.is_empty() {
        return Err(crate::error::Error::Validation(
            "Secret value must not be empty".to_string(),
        ));
    }
    Ok(value)
}

fn format_secret_value(secret: String, secret_format: CredentialSecretFormat) -> Result<String> {
    match secret_format {
        CredentialSecretFormat::Raw => Ok(secret),
        CredentialSecretFormat::Bearer => {
            if secret.starts_with("Bearer ") {
                Ok(secret)
            } else {
                Ok(format!("Bearer {secret}"))
            }
        }
        CredentialSecretFormat::Basic => {
            if secret.starts_with("Basic ") {
                Ok(secret)
            } else if secret.contains(':') {
                let encoded = base64::engine::general_purpose::STANDARD.encode(secret);
                Ok(format!("Basic {encoded}"))
            } else {
                Err(crate::error::Error::Validation(
                    "Basic auth secret must be in 'username:password' format".to_string(),
                ))
            }
        }
    }
}

fn prompt_secret(prompt: &str) -> Result<String> {
    let value = rpassword::prompt_password(format!("{prompt}: "))
        .map_err(|e| crate::error::Error::Validation(format!("Failed to read secret: {e}")))?;
    read_secret_value(Some(value), prompt)
}

fn parse_header(header: &str) -> Result<(String, String)> {
    let (name, value) = header.split_once(':').ok_or_else(|| {
        crate::error::Error::Validation(
            "Header must be in 'Name: value' format (e.g., 'Authorization: Bearer sk-...')"
                .to_string(),
        )
    })?;
    Ok((name.trim().to_string(), value.trim().to_string()))
}

// --- OpenClaw integration ---

const OPENCLAW_SERVICE_SLUG: &str = "llm-openclaw";
const OPENCLAW_PROVIDER_SLUG: &str = "openclaw";

async fn cmd_openclaw(command: OpenClawCommands, config_path: Option<&str>) -> Result<()> {
    let config_dir = config::resolve_config_dir(config_path);
    let config_file = config_dir.join("config.toml");

    match command {
        OpenClawCommands::Connect {
            url,
            token,
            api_url,
            access_token,
        } => {
            cmd_openclaw_connect(
                &config_file,
                &config_dir,
                &url,
                token,
                api_url,
                access_token,
            )
            .await
        }
        OpenClawCommands::Status => cmd_openclaw_status(&config_file, &config_dir),
        OpenClawCommands::Disconnect => cmd_openclaw_disconnect(&config_file, &config_dir),
    }
}

async fn cmd_openclaw_connect(
    config_file: &Path,
    config_dir: &Path,
    gateway_url: &str,
    token: Option<String>,
    api_url: Option<String>,
    access_token: Option<String>,
) -> Result<()> {
    // Validate gateway URL
    if !gateway_url.starts_with("http://") && !gateway_url.starts_with("https://") {
        return Err(crate::error::Error::Validation(
            "Gateway URL must start with http:// or https://".to_string(),
        ));
    }

    // Get bearer token (prompt if not provided)
    let bearer_token = match token {
        Some(t) => t,
        None => {
            println!("Enter your OpenClaw gateway bearer token (OPENCLAW_GATEWAY_TOKEN):");
            prompt_secret("Bearer token")?
        }
    };

    // 1. Store credential locally on the node
    let mut config = NodeConfig::load(config_file)?;
    let backend = SecretBackend::from_config(&config, config_dir)?;

    let header_value = format!("Bearer {bearer_token}");
    config.add_header_credential_via(
        OPENCLAW_SERVICE_SLUG,
        "Authorization",
        &header_value,
        Some(gateway_url),
        &backend,
    )?;
    config.save(config_file)?;
    println!("Local credential stored for '{OPENCLAW_SERVICE_SLUG}'.");

    // 2. Register provider connection with NyxID backend (if access token available)
    let nyxid_token = access_token
        .or_else(|| std::env::var("NYXID_ACCESS_TOKEN").ok())
        .filter(|s| !s.is_empty());

    let base_api_url = api_url.unwrap_or_else(|| {
        // Derive HTTP API URL from the WS URL in config
        let ws_url = &config.server.url;
        ws_url
            .replace("ws://", "http://")
            .replace("wss://", "https://")
            .replace("/api/v1/nodes/ws", "")
    });

    if let Some(ref token) = nyxid_token {
        let client = reqwest::Client::new();

        // 2a. Find the OpenClaw provider ID
        let providers_resp = client
            .get(format!("{base_api_url}/api/v1/providers"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| crate::error::Error::Config(format!("Failed to fetch providers: {e}")))?;

        if !providers_resp.status().is_success() {
            println!(
                "Warning: Could not fetch providers from NyxID ({}). Skipping remote registration.",
                providers_resp.status()
            );
        } else {
            let body: serde_json::Value = providers_resp.json().await.map_err(|e| {
                crate::error::Error::Config(format!("Failed to parse providers response: {e}"))
            })?;

            let provider_id: Option<String> =
                body["providers"]
                    .as_array()
                    .and_then(|arr: &Vec<serde_json::Value>| {
                        arr.iter().find_map(|p| {
                            if p["slug"].as_str() == Some(OPENCLAW_PROVIDER_SLUG) {
                                p["id"].as_str().map(String::from)
                            } else {
                                None
                            }
                        })
                    });

            if let Some(ref pid) = provider_id {
                // 2b. Connect API key with gateway URL
                let connect_resp = client
                    .post(format!(
                        "{base_api_url}/api/v1/providers/{pid}/connect/api-key"
                    ))
                    .bearer_auth(token)
                    .json(&serde_json::json!({
                        "api_key": bearer_token,
                        "gateway_url": gateway_url,
                        "label": "Node agent",
                    }))
                    .send()
                    .await;

                match connect_resp {
                    Ok(resp) if resp.status().is_success() => {
                        println!("Provider connection registered with NyxID.");
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Warning: Provider connect returned {status}: {body}");
                    }
                    Err(e) => {
                        println!("Warning: Could not register provider connection: {e}");
                    }
                }

                // 2c. Find service ID for llm-openclaw and create node binding
                let node_id = Some(config.node.id.clone());

                if let Some(ref nid) = node_id {
                    // Find the llm-openclaw service
                    let services_resp = client
                        .get(format!("{base_api_url}/api/v1/proxy/services"))
                        .bearer_auth(token)
                        .send()
                        .await;

                    let service_id: Option<String> = match services_resp {
                        Ok(resp) if resp.status().is_success() => {
                            let body: serde_json::Value = resp.json().await.unwrap_or_default();
                            body["services"]
                                .as_array()
                                .and_then(|arr: &Vec<serde_json::Value>| {
                                    arr.iter().find_map(|s| {
                                        if s["slug"].as_str() == Some(OPENCLAW_SERVICE_SLUG) {
                                            s["id"].as_str().map(String::from)
                                        } else {
                                            None
                                        }
                                    })
                                })
                        }
                        _ => None,
                    };

                    if let Some(ref sid) = service_id {
                        let binding_resp = client
                            .post(format!("{base_api_url}/api/v1/nodes/{nid}/bindings"))
                            .bearer_auth(token)
                            .json(&serde_json::json!({ "service_id": sid }))
                            .send()
                            .await;

                        match binding_resp {
                            Ok(resp) if resp.status().is_success() => {
                                println!("Node binding created for '{OPENCLAW_SERVICE_SLUG}'.");
                            }
                            Ok(resp) if resp.status().as_u16() == 409 => {
                                println!(
                                    "Node binding already exists for '{OPENCLAW_SERVICE_SLUG}'."
                                );
                            }
                            Ok(resp) => {
                                let status = resp.status();
                                let body = resp.text().await.unwrap_or_default();
                                println!("Warning: Create binding returned {status}: {body}");
                            }
                            Err(e) => {
                                println!("Warning: Could not create node binding: {e}");
                            }
                        }
                    } else {
                        println!(
                            "Warning: '{OPENCLAW_SERVICE_SLUG}' service not found. Binding not created."
                        );
                    }
                } else {
                    println!("Warning: Node ID not found in config. Binding not created.");
                }
            } else {
                println!(
                    "Warning: OpenClaw provider not found on NyxID. Skipping remote registration."
                );
            }
        }
    } else {
        println!("No NyxID access token provided (--access-token or NYXID_ACCESS_TOKEN env var).");
        println!(
            "Local credential stored. To complete setup, create a binding in the NyxID web UI."
        );
    }

    println!();
    println!("OpenClaw connected at {gateway_url}");
    println!("Start the node agent with: nyxid-node start");
    Ok(())
}

fn cmd_openclaw_status(config_file: &Path, config_dir: &Path) -> Result<()> {
    let config = NodeConfig::load(config_file)?;
    let backend = SecretBackend::from_config(&config, config_dir)?;
    let creds = CredentialStore::from_config_with_backend(&config, &backend)?;

    if let Some(cred) = creds.get(OPENCLAW_SERVICE_SLUG) {
        println!("OpenClaw: connected");
        println!(
            "  Injection: {} ({})",
            cred.injection_method(),
            cred.target_name()
        );
    } else {
        println!("OpenClaw: not connected");
        println!("  Run 'nyxid-node openclaw connect --url <gateway-url>' to connect.");
    }

    Ok(())
}

fn cmd_openclaw_disconnect(config_file: &Path, config_dir: &Path) -> Result<()> {
    let mut config = NodeConfig::load(config_file)?;
    let backend = SecretBackend::from_config(&config, config_dir)?;
    config.remove_credential_via(OPENCLAW_SERVICE_SLUG, &backend)?;
    config.save(config_file)?;
    println!("OpenClaw credentials removed from node.");
    println!("Note: To fully disconnect, also remove the binding in the NyxID web UI.");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_credentials_add_oauth(
    config_file: &Path,
    config_dir: &Path,
    service: &str,
    from_catalog: bool,
    client_id: Option<String>,
    client_secret: Option<String>,
    authorization_url: Option<String>,
    token_url: Option<String>,
    device_code_url: Option<String>,
    scopes: Option<String>,
    target_url: Option<String>,
    api_url: Option<String>,
    access_token: Option<String>,
) -> Result<()> {
    let mut config = NodeConfig::load(config_file)?;
    let backend = SecretBackend::from_config(&config, config_dir)?;

    // 1. Get OAuth config
    let oauth_config = if from_catalog {
        let base_api_url = api_url.unwrap_or_else(|| {
            config
                .server
                .url
                .replace("ws://", "http://")
                .replace("wss://", "https://")
                .replace("/api/v1/nodes/ws", "")
        });
        let token = access_token
            .or_else(|| std::env::var("NYXID_ACCESS_TOKEN").ok())
            .filter(|s| !s.is_empty());

        oauth::fetch_catalog_oauth_config(&base_api_url, token.as_deref(), service).await?
    } else {
        let tok_url = token_url.ok_or_else(|| {
            crate::error::Error::Validation(
                "--token-url is required when not using --from-catalog".to_string(),
            )
        })?;
        oauth::OAuthConfig {
            authorization_url,
            token_url: tok_url,
            device_code_url,
            device_verification_url: None,
            device_token_url: None,
            default_scopes: scopes
                .as_deref()
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
            supports_pkce: false,
            device_code_format: "rfc8628".to_string(),
            token_endpoint_auth_method: "client_secret_post".to_string(),
            extra_auth_params: None,
        }
    };

    // 2. Get client credentials
    let cid = match client_id {
        Some(id) => id,
        None => prompt_secret("OAuth Client ID")?,
    };
    let csecret = match client_secret {
        Some(s) => Some(s),
        None => {
            println!("Enter OAuth Client Secret (press Enter to skip for public clients):");
            match rpassword::prompt_password("Client Secret: ") {
                Ok(s) if s.is_empty() => None,
                Ok(s) => Some(s),
                Err(_) => None,
            }
        }
    };

    // 3. Determine scopes
    let final_scopes = scopes.unwrap_or_else(|| oauth_config.default_scopes.join(" "));

    // 4. Run the OAuth flow
    let token_response = if oauth_config.device_code_url.is_some() {
        oauth::run_device_code_flow(&oauth_config, &cid, csecret.as_deref(), &final_scopes).await?
    } else {
        return Err(crate::error::Error::Validation(
            "No device_code_url available. Only device code flow is currently supported."
                .to_string(),
        ));
    };

    // 5. Store tokens locally
    let header_value = format!(
        "{} {}",
        token_response.token_type.as_deref().unwrap_or("Bearer"),
        token_response.access_token
    );

    let expires_at = token_response
        .expires_in
        .map(|secs| (chrono::Utc::now() + chrono::Duration::seconds(secs as i64)).to_rfc3339());

    // Store the header credential (for immediate use by proxy_executor)
    config.add_header_credential_via(
        service,
        "Authorization",
        &header_value,
        target_url.as_deref(),
        &backend,
    )?;

    // Store OAuth metadata for refresh
    if let Some(cred) = config.credentials.get_mut(service) {
        cred.oauth_managed = true;
        cred.oauth_token_url = Some(oauth_config.token_url.clone());
        cred.oauth_access_token_encrypted = backend.store_credential_value(
            &format!("{service}:oauth_access"),
            &token_response.access_token,
        )?;
        if let Some(ref rt) = token_response.refresh_token {
            cred.oauth_refresh_token_encrypted =
                backend.store_credential_value(&format!("{service}:oauth_refresh"), rt)?;
        }
        cred.oauth_token_expires_at = expires_at;
        cred.oauth_client_id_encrypted =
            backend.store_credential_value(&format!("{service}:oauth_cid"), &cid)?;
        if let Some(ref cs) = csecret {
            cred.oauth_client_secret_encrypted =
                backend.store_credential_value(&format!("{service}:oauth_csecret"), cs)?;
        }
        cred.oauth_scopes = if final_scopes.is_empty() {
            None
        } else {
            Some(final_scopes)
        };
        cred.oauth_token_endpoint_auth_method = Some(oauth_config.token_endpoint_auth_method);
    }

    config.save(config_file)?;
    println!("OAuth credential stored for service '{service}'.");
    Ok(())
}

/// Background task that refreshes OAuth tokens before they expire.
async fn oauth_refresh_loop(
    config_file: std::path::PathBuf,
    interval: Duration,
    backend: std::sync::Arc<SecretBackend>,
) {
    loop {
        tokio::time::sleep(interval).await;

        let config = match NodeConfig::load(&config_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut config_changed = false;
        let mut updated_config = config.clone();

        for (slug, cred) in &config.credentials {
            if !cred.oauth_managed {
                continue;
            }

            // Check if token expires within 5 minutes
            let needs_refresh = match &cred.oauth_token_expires_at {
                Some(expires_str) => match chrono::DateTime::parse_from_rfc3339(expires_str) {
                    Ok(expires) => {
                        let now = chrono::Utc::now();
                        let buffer = chrono::Duration::minutes(5);
                        expires.with_timezone(&chrono::Utc) - buffer < now
                    }
                    Err(_) => false,
                },
                None => false,
            };

            if !needs_refresh {
                continue;
            }

            // Load refresh token
            let refresh_tok = match &cred.oauth_refresh_token_encrypted {
                Some(enc) => {
                    match backend
                        .load_credential_value(&format!("{slug}:oauth_refresh"), Some(enc.as_str()))
                    {
                        Ok(t) => t,
                        Err(_) => continue,
                    }
                }
                None => continue,
            };

            let client_id = match &cred.oauth_client_id_encrypted {
                Some(enc) => {
                    match backend
                        .load_credential_value(&format!("{slug}:oauth_cid"), Some(enc.as_str()))
                    {
                        Ok(t) => t,
                        Err(_) => continue,
                    }
                }
                None => continue,
            };

            let client_secret = cred.oauth_client_secret_encrypted.as_ref().and_then(|enc| {
                backend
                    .load_credential_value(&format!("{slug}:oauth_csecret"), Some(enc.as_str()))
                    .ok()
            });

            let token_url = match &cred.oauth_token_url {
                Some(url) => url.as_str(),
                None => continue,
            };

            let auth_method = cred
                .oauth_token_endpoint_auth_method
                .as_deref()
                .unwrap_or("client_secret_post");

            // Attempt refresh
            match oauth::refresh_token(
                token_url,
                &client_id,
                client_secret.as_deref(),
                &refresh_tok,
                auth_method,
            )
            .await
            {
                Ok(new_token) => {
                    tracing::info!(service = %slug, "OAuth token refreshed");

                    let header_value = format!(
                        "{} {}",
                        new_token.token_type.as_deref().unwrap_or("Bearer"),
                        new_token.access_token
                    );

                    if let Some(cred_mut) = updated_config.credentials.get_mut(slug) {
                        // Update header value
                        cred_mut.header_value_encrypted = backend
                            .store_credential_value(slug, &header_value)
                            .ok()
                            .flatten();

                        // Update OAuth tokens
                        cred_mut.oauth_access_token_encrypted = backend
                            .store_credential_value(
                                &format!("{slug}:oauth_access"),
                                &new_token.access_token,
                            )
                            .ok()
                            .flatten();

                        if let Some(ref rt) = new_token.refresh_token {
                            cred_mut.oauth_refresh_token_encrypted = backend
                                .store_credential_value(&format!("{slug}:oauth_refresh"), rt)
                                .ok()
                                .flatten();
                        }

                        cred_mut.oauth_token_expires_at = new_token.expires_in.map(|secs| {
                            (chrono::Utc::now() + chrono::Duration::seconds(secs as i64))
                                .to_rfc3339()
                        });

                        config_changed = true;
                    }
                }
                Err(e) => {
                    tracing::warn!(service = %slug, error = %e, "OAuth token refresh failed");
                }
            }
        }

        if config_changed && let Err(e) = updated_config.save(&config_file) {
            tracing::error!(error = %e, "Failed to save config after OAuth refresh");
        }
        // Config file change will be picked up by credential_reload_loop
    }
}

/// Auto-setup credentials for a service by fetching requirements from the catalog.
/// Determines if the service needs API key, OAuth, or other auth and runs the appropriate flow.
async fn cmd_credentials_setup(
    config_file: &Path,
    config_dir: &Path,
    service: &str,
    api_url: Option<&str>,
    access_token: Option<&str>,
) -> Result<()> {
    let config = NodeConfig::load(config_file)?;

    // Resolve API URL from config
    let base_api_url = api_url.map(|s| s.to_string()).unwrap_or_else(|| {
        config
            .server
            .url
            .replace("ws://", "http://")
            .replace("wss://", "https://")
            .replace("/api/v1/nodes/ws", "")
    });

    let token = access_token
        .map(|s| s.to_string())
        .or_else(|| std::env::var("NYXID_ACCESS_TOKEN").ok());

    let client = reqwest::Client::new();

    // Fetch catalog entry
    let catalog_url = format!("{base_api_url}/api/v1/catalog/{service}");
    let mut req = client.get(&catalog_url);
    if let Some(ref t) = token {
        req = req.bearer_auth(t);
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::error::Error::Validation(format!(
            "Failed to fetch catalog entry for '{service}' (HTTP {status}): {body}"
        )));
    }

    let entry: serde_json::Value = resp.json().await?;
    let provider_type = entry["provider_type"].as_str().unwrap_or("api_key");
    let credential_mode = entry["credential_mode"].as_str().unwrap_or("admin");
    let auth_method = entry["auth_method"].as_str().unwrap_or("bearer");
    let auth_key_name = entry["auth_key_name"].as_str().unwrap_or("Authorization");
    let default_url = entry["base_url"].as_str().unwrap_or("");
    let requires_gw = entry["requires_gateway_url"].as_bool().unwrap_or(false);
    let svc_name = entry["name"].as_str().unwrap_or(service);

    println!("Setting up credentials for: {svc_name} ({service})");
    println!("  Provider type:   {provider_type}");
    println!("  Credential mode: {credential_mode}");
    println!("  Auth method:     {auth_method}");
    println!();

    match provider_type {
        "oauth2" | "device_code" => {
            println!("This service requires OAuth authentication.");
            if credential_mode == "user" || credential_mode == "both" {
                println!(
                    "You need to provide your own OAuth app credentials (client_id and client_secret)."
                );
                println!();
            }
            println!("Running OAuth flow from the node...");
            println!();

            // Delegate to add-oauth with --from-catalog
            cmd_credentials_add_oauth(
                config_file,
                config_dir,
                service,
                true, // from_catalog
                None, // client_id (will be prompted if needed)
                None, // client_secret
                None,
                None,
                None,
                None, // OAuth URLs (from catalog)
                None, // target_url
                Some(base_api_url),
                token,
            )
            .await
        }
        _ => {
            // API key / bearer token
            let target_url = if requires_gw {
                println!("This service requires your instance URL.");
                eprint!("Enter your instance URL: ");
                std::io::Write::flush(&mut std::io::stderr())?;
                let mut url = String::new();
                std::io::stdin().read_line(&mut url)?;
                let url = url.trim().to_string();
                if url.is_empty() {
                    return Err(crate::error::Error::Validation(
                        "Instance URL is required for this service".to_string(),
                    ));
                }
                Some(url)
            } else if !default_url.is_empty() && !default_url.contains(".invalid") {
                Some(default_url.to_string())
            } else {
                None
            };

            println!("This service requires an API key / bearer token.");
            if let Some(ref url) = entry["api_key_url"].as_str() {
                println!("  Get your API key at: {url}");
            }
            if let Some(ref instructions) = entry["api_key_instructions"].as_str() {
                println!("  {instructions}");
            }
            println!();

            let secret = rpassword::prompt_password("Enter credential (hidden): ")?;
            if secret.is_empty() {
                return Err(crate::error::Error::Validation(
                    "Credential is required".to_string(),
                ));
            }

            let mut config = NodeConfig::load(config_file)?;
            let backend = SecretBackend::from_config(&config, config_dir)?;

            // Format credential based on auth method
            let header_value = if auth_method == "bearer" {
                format!("Bearer {secret}")
            } else {
                secret
            };

            if auth_method == "query" {
                config.add_query_param_credential_via(
                    service,
                    auth_key_name,
                    &header_value,
                    target_url.as_deref(),
                    &backend,
                )?;
            } else {
                config.add_header_credential_via(
                    service,
                    auth_key_name,
                    &header_value,
                    target_url.as_deref(),
                    &backend,
                )?;
            }

            config.save(config_file)?;
            println!("Credential added for service '{service}'.");
            if let Some(url) = target_url {
                println!("  Target URL: {url}");
            }
            println!("  Auth: {auth_method} / {auth_key_name}");
            Ok(())
        }
    }
}

fn parse_query_param(param: &str) -> Result<(String, String)> {
    let (name, value) = param.split_once('=').ok_or_else(|| {
        crate::error::Error::Validation(
            "Query param must be in 'name=value' format (e.g., 'api_key=sk-...')".to_string(),
        )
    })?;
    Ok((name.to_string(), value.to_string()))
}

/// Prompt for a string value with a default.
fn prompt_string(label: &str, default: &str) -> Result<String> {
    use std::io::Write;
    print!("{label} [{default}]: ");
    std::io::stdout()
        .flush()
        .map_err(|e| crate::error::Error::Validation(format!("Failed to flush stdout: {e}")))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| crate::error::Error::Validation(format!("Failed to read input: {e}")))?;
    let trimmed = input.trim();
    Ok(if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    })
}

/// Prompt for an optional string value (empty = None).
fn prompt_string_optional(label: &str) -> Result<String> {
    use std::io::Write;
    print!("{label}: ");
    std::io::stdout()
        .flush()
        .map_err(|e| crate::error::Error::Validation(format!("Failed to flush stdout: {e}")))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| crate::error::Error::Validation(format!("Failed to read input: {e}")))?;
    Ok(input.trim().to_string())
}

/// Prompt to choose from a set of options.
fn prompt_choice(label: &str, options: &[&str], default: &str) -> Result<String> {
    use std::io::Write;
    let options_str = options.join("/");
    print!("{label} ({options_str}) [{default}]: ");
    std::io::stdout()
        .flush()
        .map_err(|e| crate::error::Error::Validation(format!("Failed to flush stdout: {e}")))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| crate::error::Error::Validation(format!("Failed to read input: {e}")))?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default.to_string());
    }
    if options.contains(&trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(crate::error::Error::Validation(format!(
            "Invalid choice '{trimmed}', expected one of: {options_str}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::encryption::LocalEncryption;
    use crate::error::Error;

    #[test]
    fn format_secret_value_supports_raw_bearer_and_basic() {
        assert_eq!(
            format_secret_value("sk-test".to_string(), CredentialSecretFormat::Raw).unwrap(),
            "sk-test"
        );
        assert_eq!(
            format_secret_value("sk-test".to_string(), CredentialSecretFormat::Bearer).unwrap(),
            "Bearer sk-test"
        );
        assert_eq!(
            format_secret_value("user:pass".to_string(), CredentialSecretFormat::Basic).unwrap(),
            "Basic dXNlcjpwYXNz"
        );
    }

    #[test]
    fn format_secret_value_accepts_existing_bearer_and_basic_prefixes() {
        assert_eq!(
            format_secret_value("Bearer sk-test".to_string(), CredentialSecretFormat::Bearer,)
                .unwrap(),
            "Bearer sk-test"
        );
        assert_eq!(
            format_secret_value(
                "Basic dXNlcjpwYXNz".to_string(),
                CredentialSecretFormat::Basic,
            )
            .unwrap(),
            "Basic dXNlcjpwYXNz"
        );
    }

    #[test]
    fn basic_secret_requires_username_password_pair() {
        let err = format_secret_value("token-only".to_string(), CredentialSecretFormat::Basic)
            .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn missing_keychain_signing_secret_fails_closed() {
        let backend = SecretBackend::new_mock_keychain("node-1");
        let mut config = NodeConfig::new(
            "wss://example.com/api/v1/nodes/ws".to_string(),
            "node-1".to_string(),
            "keychain".to_string(),
        );

        backend
            .store_auth_token(&mut config, "nyx_nauth_test")
            .unwrap();
        config.signing.shared_secret_encrypted = Some(String::new());

        let err = backend.load_signing_secret(&config).unwrap_err();
        assert!(matches!(err, Error::Keychain(_)));
    }

    #[test]
    fn migrate_keychain_to_file_cleans_up_source_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let source = SecretBackend::new_mock_keychain("node-1");
        let target = SecretBackend::File(LocalEncryption::load_or_generate(dir.path()).unwrap());

        let mut config = NodeConfig::new(
            "wss://example.com/api/v1/nodes/ws".to_string(),
            "node-1".to_string(),
            "keychain".to_string(),
        );
        source
            .store_auth_token(&mut config, "nyx_nauth_test")
            .unwrap();
        source
            .store_signing_secret(&mut config, "00112233445566778899aabbccddeeff")
            .unwrap();
        config
            .add_header_credential_via("openai", "Authorization", "Bearer sk-test", None, &source)
            .unwrap();

        let config_file = dir.path().join("config.toml");
        let report = migrate_config(&mut config, &source, &target, "file", &config_file).unwrap();
        assert!(report.cleanup_warnings.is_empty());
        assert_eq!(config.storage_backend, "file");

        let loaded = NodeConfig::load(&config_file).unwrap();
        let file_backend = SecretBackend::from_config(&loaded, dir.path()).unwrap();
        assert_eq!(
            file_backend.load_auth_token(&loaded).unwrap(),
            "nyx_nauth_test"
        );
        assert_eq!(
            file_backend.load_signing_secret(&loaded).unwrap(),
            Some("00112233445566778899aabbccddeeff".to_string())
        );
        assert_eq!(
            file_backend
                .load_credential_value(
                    "openai",
                    loaded.credentials["openai"]
                        .header_value_encrypted
                        .as_deref(),
                )
                .unwrap(),
            "Bearer sk-test"
        );

        assert!(source.load_auth_token(&config).is_err());
        assert!(source.load_signing_secret(&config).is_err());
        assert!(
            source
                .load_credential_value(
                    "openai",
                    config.credentials["openai"]
                        .header_value_encrypted
                        .as_deref(),
                )
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn migrate_preserves_source_secrets_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let source = SecretBackend::new_mock_keychain("node-1");
        let target = SecretBackend::File(LocalEncryption::load_or_generate(dir.path()).unwrap());

        let mut config = NodeConfig::new(
            "wss://example.com/api/v1/nodes/ws".to_string(),
            "node-1".to_string(),
            "keychain".to_string(),
        );
        source
            .store_auth_token(&mut config, "nyx_nauth_test")
            .unwrap();
        source
            .store_signing_secret(&mut config, "00112233445566778899aabbccddeeff")
            .unwrap();
        config
            .add_header_credential_via("openai", "Authorization", "Bearer sk-test", None, &source)
            .unwrap();

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let config_file = dir.path().join("config.toml");
        let result = migrate_config(&mut config, &source, &target, "file", &config_file);

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert_eq!(config.storage_backend, "keychain");
        assert_eq!(source.load_auth_token(&config).unwrap(), "nyx_nauth_test");
        assert_eq!(
            source.load_signing_secret(&config).unwrap(),
            Some("00112233445566778899aabbccddeeff".to_string())
        );
        assert_eq!(
            source
                .load_credential_value(
                    "openai",
                    config.credentials["openai"]
                        .header_value_encrypted
                        .as_deref(),
                )
                .unwrap(),
            "Bearer sk-test"
        );
    }

    #[test]
    fn cleanup_source_secrets_removes_auth_signing_and_credentials() {
        let backend = SecretBackend::new_mock_keychain("node-1");
        let mut config = NodeConfig::new(
            "wss://example.com/api/v1/nodes/ws".to_string(),
            "node-1".to_string(),
            "keychain".to_string(),
        );
        backend
            .store_auth_token(&mut config, "nyx_nauth_test")
            .unwrap();
        backend
            .store_signing_secret(&mut config, "00112233445566778899aabbccddeeff")
            .unwrap();
        config
            .add_header_credential_via("openai", "Authorization", "Bearer sk-test", None, &backend)
            .unwrap();

        let warnings = cleanup_source_secrets(
            &backend,
            &[(
                "openai".to_string(),
                "header".to_string(),
                "Bearer sk-test".to_string(),
            )],
        );

        assert!(warnings.is_empty());
        // After cleanup, vault fields should be cleared
        assert!(backend.load_auth_token(&config).is_err());
        assert!(backend.load_signing_secret(&config).is_err());
        assert!(backend.load_credential_value("openai", None).is_err());
    }
}
