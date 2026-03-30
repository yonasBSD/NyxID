use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::api::{ApiClient, CLI_USER_AGENT};
use crate::cli::{AgentCommands, AiSetupCommands, AiToolTarget, OutputFormat};

/// GitHub raw base URL for the NyxID repository.
const GITHUB_RAW: &str = "https://raw.githubusercontent.com/ChronoAIProject/NyxID/main";

/// Path within the repo to the canonical skill files.
const SKILL_DIR: &str = "skills/nyxid";

/// The default hosted NyxID URL used in the repo's SKILL.md.
/// Replaced with the user's actual server URL at install time.
const DEFAULT_HOSTED_URL: &str = "https://nyx-api.chrono-ai.fun";

/// The default hosted dashboard URL.
const DEFAULT_HOSTED_DASHBOARD: &str = "https://nyx.chrono-ai.fun";

pub async fn run(command: AiSetupCommands) -> Result<()> {
    match command {
        AiSetupCommands::Install { tool, base_url } => install(tool, &base_url).await,
        AiSetupCommands::Update { tool, base_url } => update(tool, &base_url).await,
        AiSetupCommands::Status => status(),
        AiSetupCommands::Agent { command } => run_agent_command(command).await,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_base_url(base_url: &Option<String>) -> Result<String> {
    let url = if let Some(url) = base_url {
        url.trim_end_matches('/').to_string()
    } else if let Some(url) = crate::auth::read_saved_base_url() {
        url.trim_end_matches('/').to_string()
    } else {
        bail!(
            "No base URL configured. Run `nyxid login --base-url <URL>` first, \
             or pass --base-url, or set NYXID_URL"
        )
    };

    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("Base URL must use http:// or https:// scheme, got: {url}");
    }

    Ok(url)
}

/// Max response size for fetched files (2 MiB).
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(CLI_USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")
}

/// Fetch a URL, validating status code and response size.
async fn fetch_url(url: &str) -> Result<String> {
    let resp = http_client()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch {url}"))?;

    if !resp.status().is_success() {
        bail!("HTTP {} from {url}", resp.status());
    }
    if resp
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES)
    {
        bail!("Response too large (max {MAX_RESPONSE_BYTES} bytes) from {url}");
    }

    resp.text()
        .await
        .with_context(|| format!("Failed to read body from {url}"))
}

/// Fetch a file from the GitHub repository.
async fn fetch_github(path: &str) -> Result<String> {
    fetch_url(&format!("{GITHUB_RAW}/{path}")).await
}

/// Fetch the playbook from the user's NyxID server (already has URL substitution).
async fn fetch_playbook(base_url: &str) -> Result<String> {
    fetch_url(&format!("{base_url}/llms.txt")).await
}

/// Replace default hosted URLs in content with the user's actual server URL.
fn substitute_urls(content: &str, base_url: &str, dashboard_url: &str) -> String {
    content
        .replace(DEFAULT_HOSTED_URL, base_url)
        .replace(DEFAULT_HOSTED_DASHBOARD, dashboard_url)
        .replace("http://localhost:3001", base_url)
        .replace("http://localhost:3000", dashboard_url)
}

async fn resolve_dashboard_url(base_url: &str) -> String {
    crate::auth::fetch_frontend_url(base_url)
        .await
        .unwrap_or_else(|_| base_url.to_string())
}

fn write_file(path: &PathBuf, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    eprintln!("  Wrote {}", path.display());
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Could not determine home directory")
}

fn cargo_home_dir(home: &Path) -> PathBuf {
    std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".cargo"))
}

fn current_shell_name() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    PathBuf::from(shell)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "sh".into())
}

fn shell_rc_path(home: &Path, shell_name: &str) -> PathBuf {
    match shell_name {
        "zsh" => home.join(".zshrc"),
        "bash" => {
            if cfg!(target_os = "macos") {
                home.join(".bash_profile")
            } else {
                home.join(".bashrc")
            }
        }
        "fish" => std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"))
            .join("fish/config.fish"),
        _ => home.join(".profile"),
    }
}

fn shell_escape_double_quoted(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn cargo_path_is_configured(contents: &str, cargo_bin: &Path, cargo_env: &Path) -> bool {
    let cargo_bin = cargo_bin.to_string_lossy();
    let cargo_env = cargo_env.to_string_lossy();

    [
        cargo_bin.as_ref(),
        cargo_env.as_ref(),
        "$HOME/.cargo/bin",
        "$HOME/.cargo/env",
        "${HOME}/.cargo/bin",
        "${HOME}/.cargo/env",
        ".cargo/bin",
        ".cargo/env",
        "fish_add_path",
    ]
    .iter()
    .any(|needle| contents.contains(needle))
}

fn cargo_setup_command(shell_name: &str, cargo_bin: &Path, cargo_env: &Path) -> String {
    if shell_name == "fish" {
        format!(
            "fish_add_path \"{}\"",
            shell_escape_double_quoted(cargo_bin)
        )
    } else if cargo_env.exists() {
        format!(". \"{}\"", shell_escape_double_quoted(cargo_env))
    } else {
        format!(
            "export PATH=\"{}:$PATH\"",
            shell_escape_double_quoted(cargo_bin)
        )
    }
}

fn cargo_setup_rc_snippet(shell_name: &str, cargo_bin: &Path, cargo_env: &Path) -> String {
    format!(
        "\n# Cargo (Rust package manager) -- added by NyxID installer\n{}\n",
        cargo_setup_command(shell_name, cargo_bin, cargo_env)
    )
}

fn skill_paths(tool: AiToolTarget) -> Result<Vec<(String, PathBuf)>> {
    let home = home_dir()?;
    match tool {
        AiToolTarget::ClaudeCode => Ok(vec![(
            "skill".into(),
            home.join(".claude/skills/nyxid/SKILL.md"),
        )]),
        AiToolTarget::Cursor => Ok(vec![(
            "rule".into(),
            PathBuf::from(".cursor/rules/nyxid.mdc"),
        )]),
        AiToolTarget::Codex => Ok(vec![(
            "skill".into(),
            home.join(".codex/skills/nyxid/SKILL.md"),
        )]),
        AiToolTarget::Openclaw => Ok(vec![(
            "skill".into(),
            home.join(".openclaw/skills/nyxid/SKILL.md"),
        )]),
        AiToolTarget::Generic => Ok(vec![]),
    }
}

// ---------------------------------------------------------------------------
// Shared fetch: get SKILL.md from GitHub, playbook from server
// ---------------------------------------------------------------------------

struct SkillContent {
    skill_md: String,
    playbook: String,
    post_install: String,
    install_sh: String,
    services_sh: String,
    proxy_sh: String,
}

async fn fetch_skill_content(base_url: &str) -> Result<SkillContent> {
    let dashboard = resolve_dashboard_url(base_url).await;

    eprintln!("  Fetching skill from GitHub...");
    let skill_md_raw = fetch_github(&format!("{SKILL_DIR}/SKILL.md")).await?;
    let skill_md = substitute_urls(&skill_md_raw, base_url, &dashboard);

    let post_install = fetch_github(&format!("{SKILL_DIR}/POST_INSTALL.md"))
        .await
        .unwrap_or_default();

    eprintln!("  Fetching playbook from {base_url}/llms.txt...");
    let playbook = fetch_playbook(base_url).await?;

    let install_sh = fetch_github(&format!("{SKILL_DIR}/tools/install.sh")).await?;
    let services_sh = fetch_github(&format!("{SKILL_DIR}/tools/services.sh")).await?;
    let proxy_sh = fetch_github(&format!("{SKILL_DIR}/tools/proxy.sh")).await?;

    Ok(SkillContent {
        skill_md,
        playbook,
        post_install,
        install_sh,
        services_sh,
        proxy_sh,
    })
}

// ---------------------------------------------------------------------------
// PATH setup
// ---------------------------------------------------------------------------

/// Detect the user's shell RC file and ensure the active cargo bin directory is
/// in PATH. This is critical for non-technical users whose shell does not
/// source cargo env by default (e.g. after a fresh `cargo install`).
fn ensure_cargo_in_path() {
    let home = match home_dir() {
        Ok(h) => h,
        Err(_) => return,
    };

    let cargo_home = cargo_home_dir(&home);
    let cargo_bin = cargo_home.join("bin");
    let cargo_env = cargo_home.join("env");

    if let Some(path_var) = std::env::var_os("PATH")
        && std::env::split_paths(&path_var).any(|path| path == cargo_bin)
    {
        return;
    }

    // If the binary doesn't exist at all, skip PATH setup.
    if !cargo_bin.join("nyxid").exists() {
        return;
    }

    let shell_name = current_shell_name();
    let rc_file = shell_rc_path(&home, &shell_name);

    if let Ok(contents) = std::fs::read_to_string(&rc_file)
        && cargo_path_is_configured(&contents, &cargo_bin, &cargo_env)
    {
        return;
    }

    let line = cargo_setup_rc_snippet(&shell_name, &cargo_bin, &cargo_env);

    if let Some(parent) = rc_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_file)
    {
        Ok(mut f) => {
            use std::io::Write;
            if f.write_all(line.as_bytes()).is_ok() {
                eprintln!(
                    "  Added {} to PATH in {}",
                    cargo_bin.display(),
                    rc_file.display()
                );
                eprintln!("  Open a new terminal or run: source {}", rc_file.display());
            }
        }
        Err(e) => {
            eprintln!("  [warn] Could not update {}: {e}", rc_file.display());
            eprintln!(
                "  Add this to your shell config manually: {}",
                cargo_setup_command(&shell_name, &cargo_bin, &cargo_env)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

async fn install(tool: AiToolTarget, base_url: &Option<String>) -> Result<()> {
    let base = resolve_base_url(base_url)?;
    eprintln!("Installing NyxID skill for {tool}...");

    let content = fetch_skill_content(&base).await?;

    let result = match tool {
        AiToolTarget::ClaudeCode => install_claude_code(&content).await,
        AiToolTarget::Cursor => install_cursor(&content),
        AiToolTarget::Codex => install_codex(&content),
        AiToolTarget::Openclaw => install_openclaw(&content),
        AiToolTarget::Generic => {
            bail!(
                "Generic tool has no skill files. Use `nyxid ai-setup agent create` to create an agent identity instead."
            )
        }
    };

    if result.is_ok() {
        // After installing the skill, ensure the active cargo bin directory is
        // in PATH so that `skills check` can find the nyxid binary.
        ensure_cargo_in_path();
    }

    result
}

/// Print the post-install summary plus tool-specific notes.
fn print_post_install(tool: AiToolTarget, content: &SkillContent) {
    // Tool-specific restart / activation notes
    match tool {
        AiToolTarget::ClaudeCode => {
            eprintln!(
                "Use /nyxid in Claude Code, or just ask about NyxID and it activates automatically."
            );
        }
        AiToolTarget::Cursor => {
            eprintln!(
                "Cursor rules are project-level. Run this command in each project you want NyxID in."
            );
        }
        AiToolTarget::Codex => {
            eprintln!("Start a new Codex session to load the skill.");
        }
        AiToolTarget::Openclaw => {
            eprintln!("To activate, start a new chat in OpenClaw.");
            eprintln!();

            // Check if the gateway is installed as a service
            let gateway_installed = std::process::Command::new("openclaw")
                .args(["gateway", "status"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());

            if !gateway_installed {
                eprintln!("Tip: Install the OpenClaw gateway as a background service so it");
                eprintln!("stays running and restarts automatically:");
                eprintln!();
                eprintln!("  openclaw gateway install");
                eprintln!("  openclaw gateway start");
                eprintln!();
                eprintln!("You can verify it's running with: openclaw gateway status");
                eprintln!();
            }

            eprintln!("Verify skill: `openclaw skills check` (should show NyxID as ready).");
        }
        AiToolTarget::Generic => {}
    }

    eprintln!();
    eprintln!("To update the skill later: nyxid ai-setup update");
    eprintln!(
        "To update the CLI itself: cargo install --git https://github.com/ChronoAIProject/NyxID nyxid-cli"
    );

    if !content.post_install.is_empty() {
        eprintln!();
        for line in content.post_install.lines() {
            eprintln!("{line}");
        }
    }
}

async fn install_claude_code(content: &SkillContent) -> Result<()> {
    let dir = home_dir()?.join(".claude/skills/nyxid");

    write_file(&dir.join("SKILL.md"), &content.skill_md)?;
    write_file(&dir.join("references/playbook.md"), &content.playbook)?;

    eprintln!();
    print_post_install(AiToolTarget::ClaudeCode, content);
    Ok(())
}

fn install_cursor(content: &SkillContent) -> Result<()> {
    let mdc = format!(
        "---\ndescription: NyxID auth/SSO platform -- credential brokering, services, API keys, nodes, SSH, MCP\nglobs:\nalwaysApply: true\n---\n\n{}\n",
        content.skill_md
    );
    write_file(&PathBuf::from(".cursor/rules/nyxid.mdc"), &mdc)?;

    eprintln!();
    print_post_install(AiToolTarget::Cursor, content);
    Ok(())
}

fn install_codex(content: &SkillContent) -> Result<()> {
    let dir = home_dir()?.join(".codex/skills/nyxid");

    write_file(&dir.join("SKILL.md"), &content.skill_md)?;
    write_file(&dir.join("references/playbook.md"), &content.playbook)?;

    eprintln!();
    print_post_install(AiToolTarget::Codex, content);
    Ok(())
}

fn install_openclaw(content: &SkillContent) -> Result<()> {
    let dir = home_dir()?.join(".openclaw/skills/nyxid");

    write_file(&dir.join("SKILL.md"), &content.skill_md)?;
    write_file(&dir.join("references/playbook.md"), &content.playbook)?;
    write_file(&dir.join("tools/install.sh"), &content.install_sh)?;
    write_file(&dir.join("tools/services.sh"), &content.services_sh)?;
    write_file(&dir.join("tools/proxy.sh"), &content.proxy_sh)?;

    #[cfg(unix)]
    {
        make_executable(&dir.join("tools/install.sh"))?;
        make_executable(&dir.join("tools/services.sh"))?;
        make_executable(&dir.join("tools/proxy.sh"))?;
    }

    eprintln!();
    print_post_install(AiToolTarget::Openclaw, content);
    Ok(())
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

async fn update(tool: Option<AiToolTarget>, base_url: &Option<String>) -> Result<()> {
    let all_tools = [
        AiToolTarget::ClaudeCode,
        AiToolTarget::Cursor,
        AiToolTarget::Codex,
        AiToolTarget::Openclaw,
    ];

    let tools: Vec<AiToolTarget> = match tool {
        Some(AiToolTarget::Generic) => {
            bail!("Generic tool has no skill files to update.")
        }
        Some(t) => vec![t],
        None => all_tools.to_vec(),
    };

    let base = resolve_base_url(base_url)?;
    let mut installed_tools = Vec::new();

    // Check which tools are installed before fetching
    for &t in &tools {
        let paths = skill_paths(t)?;
        let installed = paths.iter().any(|(_, p)| p.exists());
        if installed {
            installed_tools.push(t);
        } else if tool.is_some() {
            bail!(
                "No NyxID skill installed for {t}. Run `nyxid ai-setup install --tool {t}` first."
            );
        }
    }

    if installed_tools.is_empty() {
        eprintln!("No installed skills found. Run `nyxid ai-setup install --tool <tool>` first.");
        return Ok(());
    }

    // Fetch once, install to all
    let content = fetch_skill_content(&base).await?;

    for t in installed_tools {
        eprintln!("Updating {t}...");
        match t {
            AiToolTarget::ClaudeCode => install_claude_code(&content).await?,
            AiToolTarget::Cursor => install_cursor(&content)?,
            AiToolTarget::Codex => install_codex(&content)?,
            AiToolTarget::Openclaw => install_openclaw(&content)?,
            AiToolTarget::Generic => unreachable!(),
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

fn status() -> Result<()> {
    use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

    let all_tools = [
        AiToolTarget::ClaudeCode,
        AiToolTarget::Cursor,
        AiToolTarget::Codex,
        AiToolTarget::Openclaw,
    ];

    let home = home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Agent", "Type", "Path", "Status"]);

    for tool in all_tools {
        let paths = skill_paths(tool)?;

        for (label, path) in &paths {
            let status_str = if path.exists() {
                let date = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| {
                        let dt: chrono::DateTime<Utc> = t.into();
                        dt.format("%Y-%m-%d").to_string()
                    })
                    .unwrap_or_else(|| "unknown".into());
                format!("installed ({date})")
            } else {
                "not installed".into()
            };

            let display_path = path.to_string_lossy().replace(&*home_str, "~");

            table.add_row([
                &tool.to_string(),
                label.as_str(),
                &display_path,
                &status_str,
            ]);
        }
    }

    eprintln!("{table}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Agent identity commands
// ---------------------------------------------------------------------------

async fn run_agent_command(command: AgentCommands) -> Result<()> {
    match command {
        AgentCommands::Create {
            name,
            platform,
            services,
            scopes,
            auth,
        } => agent_create(&auth, &name, platform, services.as_deref(), &scopes).await,
        AgentCommands::List { auth } => agent_list(&auth).await,
        AgentCommands::Show { name, auth } => agent_show(&auth, &name).await,
        AgentCommands::Bind {
            name,
            service,
            credential,
            auth,
        } => agent_bind(&auth, &name, &service, &credential).await,
        AgentCommands::Rotate { name, auth } => agent_rotate(&auth, &name).await,
        AgentCommands::Delete { name, yes, auth } => agent_delete(&auth, &name, yes).await,
    }
}

fn array_from_response<'a>(
    value: &'a serde_json::Value,
    field_names: &[&str],
) -> Option<&'a [serde_json::Value]> {
    field_names
        .iter()
        .find_map(|field| value.get(*field).and_then(|entry| entry.as_array()))
        .map(Vec::as_slice)
        .or_else(|| value.as_array().map(Vec::as_slice))
}

async fn agent_create(
    auth: &crate::cli::AuthArgs,
    name: &str,
    platform: AiToolTarget,
    services: Option<&str>,
    scopes: &str,
) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;

    // Parse service slugs, resolve to UserService IDs
    let service_slugs: Vec<&str> = services
        .map(|s| s.split(',').map(str::trim).collect())
        .unwrap_or_default();

    let allowed_service_ids = if !service_slugs.is_empty() {
        let user_services: serde_json::Value = api.get("/user-services").await?;
        let arr = array_from_response(&user_services, &["services"]).unwrap_or(&[]);

        let ids: Vec<String> = arr
            .iter()
            .filter(|s| service_slugs.contains(&s["slug"].as_str().unwrap_or("")))
            .filter_map(|s| s["id"].as_str().map(String::from))
            .collect();

        if ids.len() != service_slugs.len() {
            bail!("Some services not found. Run `nyxid service list` to see available services.");
        }
        ids
    } else {
        vec![]
    };

    let body = serde_json::json!({
        "name": name,
        "scopes": scopes,
        "description": format!("Agent identity for {} ({})", name, platform),
        "allowed_service_ids": allowed_service_ids,
        "allow_all_services": service_slugs.is_empty(),
        "allow_all_nodes": true,
        "platform": platform.to_string(),
    });

    let resp: serde_json::Value = api.post("/api-keys", &body).await?;

    let api_key = resp["full_key"]
        .as_str()
        .unwrap_or("(error: key not in response)");
    let key_id = resp["id"].as_str().unwrap_or("");

    match auth.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "api_key_id": key_id,
                    "api_key": api_key,
                    "name": name,
                    "platform": platform.to_string(),
                    "setup_instructions": platform_instructions(platform, api_key),
                }))?
            );
        }
        OutputFormat::Table => {
            eprintln!("Agent '{name}' created successfully.");
            eprintln!();
            eprintln!("API Key (save this -- shown only once):");
            eprintln!("  {api_key}");
            eprintln!();
            eprintln!("{}", platform_instructions(platform, api_key));
        }
    }

    Ok(())
}

fn platform_instructions(platform: AiToolTarget, api_key: &str) -> String {
    match platform {
        AiToolTarget::ClaudeCode => format!(
            "Add to .claude/settings.json:\n\n\
             {{\n  \"mcpServers\": {{\n    \"nyxid\": {{\n      \
             \"command\": \"nyxid\",\n      \"args\": [\"mcp\", \"serve\"],\n      \
             \"env\": {{\n        \"NYXID_ACCESS_TOKEN\": \"{api_key}\"\n      }}\n    }}\n  }}\n}}"
        ),
        AiToolTarget::Codex => format!(
            "Add to your shell profile or Codex project config:\n\n\
             export NYXID_ACCESS_TOKEN=\"{api_key}\""
        ),
        AiToolTarget::Openclaw => format!(
            "Add to your OpenClaw workspace config:\n\n\
             skills:\n  nyxid:\n    env:\n      NYXID_ACCESS_TOKEN: \"{api_key}\""
        ),
        AiToolTarget::Cursor | AiToolTarget::Generic => format!(
            "Set NYXID_ACCESS_TOKEN={api_key} in your agent's environment.\n\
             All nyxid CLI commands will use this token automatically.\n\
             Proxy: nyxid proxy request <slug> <path> -m POST -d '...'\n\
             Direct API: curl -H \"X-API-Key: {api_key}\" <base_url>/api/v1/proxy/s/<slug>/<path>"
        ),
    }
}

async fn agent_list(auth: &crate::cli::AuthArgs) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;
    let keys: serde_json::Value = api.get("/api-keys").await?;

    let items = array_from_response(&keys, &["keys", "api_keys"]);

    match auth.output {
        OutputFormat::Json => {
            let agents: Vec<&serde_json::Value> = items
                .map(|arr| {
                    arr.iter()
                        .filter(|k| k.get("platform").and_then(|p| p.as_str()).is_some())
                        .collect()
                })
                .unwrap_or_default();
            println!("{}", serde_json::to_string_pretty(&agents)?);
        }
        OutputFormat::Table => {
            use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};

            let agents: Vec<&serde_json::Value> = items
                .map(|arr| {
                    arr.iter()
                        .filter(|k| k.get("platform").and_then(|p| p.as_str()).is_some())
                        .collect()
                })
                .unwrap_or_default();

            if agents.is_empty() {
                eprintln!("No agent identities found.");
                eprintln!(
                    "Create one with: nyxid ai-setup agent create --name <name> --platform <platform>"
                );
                return Ok(());
            }

            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(["Name", "Platform", "Scopes", "Created"]);

            for key in &agents {
                let name = key["name"].as_str().unwrap_or("-");
                let platform = key["platform"].as_str().unwrap_or("-");
                let scopes = key["scopes"].as_str().unwrap_or("-");
                let created = key["created_at"].as_str().unwrap_or("-");
                table.add_row([name, platform, scopes, created]);
            }
            eprintln!("{table}");
        }
    }

    Ok(())
}

/// Find an API key by name, returning the full key object.
async fn find_key_by_name(api: &mut ApiClient, name: &str) -> Result<serde_json::Value> {
    let keys: serde_json::Value = api.get("/api-keys").await?;
    let items = array_from_response(&keys, &["keys", "api_keys"]);

    let found = items.and_then(|arr| arr.iter().find(|k| k["name"].as_str() == Some(name)));

    match found {
        Some(k) => Ok(k.clone()),
        None => bail!("Agent '{name}' not found. Run `nyxid ai-setup agent list` to see agents."),
    }
}

async fn agent_show(auth: &crate::cli::AuthArgs, name: &str) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;
    let key = find_key_by_name(&mut api, name).await?;
    let key_id = key["id"]
        .as_str()
        .or(key["_id"].as_str())
        .context("Key has no ID")?;

    // Fetch bindings
    let bindings: serde_json::Value = api
        .get(&format!("/api-keys/{key_id}/bindings"))
        .await
        .unwrap_or_else(|_| serde_json::json!({ "bindings": [] }));

    match auth.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "agent": key,
                    "bindings": bindings,
                }))?
            );
        }
        OutputFormat::Table => {
            let name = key["name"].as_str().unwrap_or("-");
            let platform = key["platform"].as_str().unwrap_or("-");
            let scopes = key["scopes"].as_str().unwrap_or("-");
            let created = key["created_at"].as_str().unwrap_or("-");
            let active = key["is_active"].as_bool().unwrap_or(false);

            eprintln!("Name:     {name}");
            eprintln!("ID:       {key_id}");
            eprintln!("Platform: {platform}");
            eprintln!("Scopes:   {scopes}");
            eprintln!("Active:   {active}");
            eprintln!("Created:  {created}");

            let binding_arr = array_from_response(&bindings, &["bindings"]).unwrap_or(&[]);
            if !binding_arr.is_empty() {
                eprintln!();
                eprintln!("Bindings:");

                use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
                let mut table = Table::new();
                table.load_preset(UTF8_FULL_CONDENSED);
                table.set_header(["Binding ID", "Service", "Credential"]);

                for b in binding_arr {
                    let bid = b["id"].as_str().unwrap_or("-");
                    let service = b["service_label"]
                        .as_str()
                        .or(b["service_slug"].as_str())
                        .or(b["user_service_id"].as_str())
                        .unwrap_or("-");
                    let credential = b["credential_label"]
                        .as_str()
                        .or(b["user_api_key_id"].as_str())
                        .unwrap_or("-");
                    table.add_row([bid, service, credential]);
                }
                eprintln!("{table}");
            } else {
                eprintln!();
                eprintln!("No credential bindings.");
                eprintln!(
                    "Bind one with: nyxid ai-setup agent bind {name} --service <slug> --credential <label>"
                );
            }
        }
    }

    Ok(())
}

async fn agent_bind(
    auth: &crate::cli::AuthArgs,
    name: &str,
    service_slug: &str,
    credential_label: &str,
) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;

    // Resolve agent key
    let key = find_key_by_name(&mut api, name).await?;
    let key_id = key["id"]
        .as_str()
        .or(key["_id"].as_str())
        .context("Key has no ID")?;

    // Resolve service slug to UserService ID
    let user_services: serde_json::Value = api.get("/user-services").await?;
    let service_arr = array_from_response(&user_services, &["services"]).unwrap_or(&[]);
    let service = service_arr
        .iter()
        .find(|s| s["slug"].as_str() == Some(service_slug))
        .ok_or_else(|| anyhow::anyhow!("Service '{service_slug}' not found"))?;
    let user_service_id = service["id"].as_str().context("Service has no ID")?;

    // Resolve credential label to UserApiKey ID
    let ext_keys: serde_json::Value = api.get("/api-keys/external").await?;
    let ext_arr = array_from_response(&ext_keys, &["api_keys"]).unwrap_or(&[]);
    let cred = ext_arr
        .iter()
        .find(|k| {
            k["label"].as_str() == Some(credential_label)
                || k["name"].as_str() == Some(credential_label)
        })
        .ok_or_else(|| anyhow::anyhow!("Credential '{credential_label}' not found"))?;
    let user_api_key_id = cred["id"].as_str().context("Credential has no ID")?;

    // Create binding
    let body = serde_json::json!({
        "user_service_id": user_service_id,
        "user_api_key_id": user_api_key_id,
    });

    let resp: serde_json::Value = api
        .post(&format!("/api-keys/{key_id}/bindings"), &body)
        .await?;

    match auth.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        OutputFormat::Table => {
            let binding_id = resp["id"].as_str().unwrap_or("-");
            eprintln!("Binding created: {binding_id}");
            eprintln!("  Agent:      {name}");
            eprintln!("  Service:    {service_slug}");
            eprintln!("  Credential: {credential_label}");
        }
    }

    Ok(())
}

async fn agent_rotate(auth: &crate::cli::AuthArgs, name: &str) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;
    let key = find_key_by_name(&mut api, name).await?;
    let key_id = key["id"]
        .as_str()
        .or(key["_id"].as_str())
        .context("Key has no ID")?;

    let resp: serde_json::Value = api
        .post(
            &format!("/api-keys/{key_id}/rotate"),
            &serde_json::json!({}),
        )
        .await?;

    match auth.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        OutputFormat::Table => {
            let new_key = resp["full_key"].as_str().unwrap_or("(not in response)");
            eprintln!("Agent '{name}' key rotated.");
            eprintln!();
            eprintln!("New API Key (save this -- shown only once):");
            eprintln!("  {new_key}");

            if let Some(platform_str) = key["platform"].as_str() {
                let platform = match platform_str {
                    "claude-code" => Some(AiToolTarget::ClaudeCode),
                    "cursor" => Some(AiToolTarget::Cursor),
                    "codex" => Some(AiToolTarget::Codex),
                    "openclaw" => Some(AiToolTarget::Openclaw),
                    "generic" => Some(AiToolTarget::Generic),
                    _ => None,
                };
                if let Some(p) = platform {
                    eprintln!();
                    eprintln!("{}", platform_instructions(p, new_key));
                }
            }
        }
    }

    Ok(())
}

async fn agent_delete(auth: &crate::cli::AuthArgs, name: &str, yes: bool) -> Result<()> {
    let mut api = ApiClient::from_auth(auth)?;
    let key = find_key_by_name(&mut api, name).await?;
    let key_id = key["id"]
        .as_str()
        .or(key["_id"].as_str())
        .context("Key has no ID")?;

    if !yes {
        use std::io::Write;
        eprint!("Delete agent '{name}'? [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            eprintln!("Cancelled.");
            return Ok(());
        }
    }

    api.delete_empty(&format!("/api-keys/{key_id}")).await?;
    eprintln!("Agent '{name}' deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        cargo_path_is_configured, cargo_setup_command, shell_escape_double_quoted, shell_rc_path,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn cargo_path_detection_matches_default_home_entries() {
        let cargo_bin = Path::new("/tmp/example/.cargo/bin");
        let cargo_env = Path::new("/tmp/example/.cargo/env");

        assert!(cargo_path_is_configured(
            ". \"$HOME/.cargo/env\"\n",
            cargo_bin,
            cargo_env
        ));
        assert!(cargo_path_is_configured(
            "export PATH=\"$HOME/.cargo/bin:$PATH\"\n",
            cargo_bin,
            cargo_env
        ));
    }

    #[test]
    fn cargo_path_detection_matches_custom_cargo_home_entries() {
        let cargo_bin = Path::new("/opt/nyx cargo/bin");
        let cargo_env = Path::new("/opt/nyx cargo/env");

        assert!(cargo_path_is_configured(
            "export PATH=\"/opt/nyx cargo/bin:$PATH\"\n",
            cargo_bin,
            cargo_env
        ));
        assert!(cargo_path_is_configured(
            ". \"/opt/nyx cargo/env\"\n",
            cargo_bin,
            cargo_env
        ));
    }

    #[test]
    fn bash_rc_path_matches_platform_convention() {
        let home = Path::new("/tmp/home");
        let expected = if cfg!(target_os = "macos") {
            home.join(".bash_profile")
        } else {
            home.join(".bashrc")
        };

        assert_eq!(shell_rc_path(home, "bash"), expected);
    }

    #[test]
    fn cargo_setup_command_uses_absolute_custom_cargo_home() {
        let test_root = temp_test_dir();
        let cargo_home = test_root.join("custom cargo home");
        let cargo_bin = cargo_home.join("bin");
        let cargo_env = cargo_home.join("env");

        fs::create_dir_all(&cargo_bin).unwrap();
        fs::write(&cargo_env, "").unwrap();

        let command = cargo_setup_command("bash", &cargo_bin, &cargo_env);
        let expected_env = shell_escape_double_quoted(&cargo_env);

        assert_eq!(command, format!(". \"{expected_env}\""));

        fs::remove_dir_all(test_root).unwrap();
    }

    fn temp_test_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "nyxid-ai-setup-tests-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn platform_instructions_claude_code_contains_mcp_config() {
        let instructions =
            super::platform_instructions(super::AiToolTarget::ClaudeCode, "test-key-123");
        assert!(instructions.contains("mcpServers"));
        assert!(instructions.contains("test-key-123"));
        assert!(instructions.contains("NYXID_ACCESS_TOKEN"));
    }

    #[test]
    fn platform_instructions_codex_contains_export() {
        let instructions = super::platform_instructions(super::AiToolTarget::Codex, "test-key-456");
        assert!(instructions.contains("export NYXID_ACCESS_TOKEN"));
        assert!(instructions.contains("test-key-456"));
    }

    #[test]
    fn platform_instructions_generic_contains_env_var() {
        let instructions =
            super::platform_instructions(super::AiToolTarget::Generic, "test-key-789");
        assert!(instructions.contains("NYXID_ACCESS_TOKEN=test-key-789"));
        assert!(instructions.contains("X-API-Key"));
    }

    #[test]
    fn ai_tool_target_display_generic() {
        assert_eq!(super::AiToolTarget::Generic.to_string(), "generic");
    }

    #[test]
    fn skill_paths_generic_returns_empty() {
        let paths = super::skill_paths(super::AiToolTarget::Generic).unwrap();
        assert!(paths.is_empty());
    }
}
