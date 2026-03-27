use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::cli::{AiSetupCommands, AiToolTarget};

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
    }
}

// ---------------------------------------------------------------------------
// Shared fetch: get SKILL.md from GitHub, playbook from server
// ---------------------------------------------------------------------------

struct SkillContent {
    skill_md: String,
    playbook: String,
    post_install: String,
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

    let services_sh = fetch_github(&format!("{SKILL_DIR}/tools/services.sh")).await?;
    let proxy_sh = fetch_github(&format!("{SKILL_DIR}/tools/proxy.sh")).await?;

    Ok(SkillContent {
        skill_md,
        playbook,
        post_install,
        services_sh,
        proxy_sh,
    })
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

async fn install(tool: AiToolTarget, base_url: &Option<String>) -> Result<()> {
    let base = resolve_base_url(base_url)?;
    eprintln!("Installing NyxID skill for {tool}...");

    let content = fetch_skill_content(&base).await?;

    match tool {
        AiToolTarget::ClaudeCode => install_claude_code(&content).await,
        AiToolTarget::Cursor => install_cursor(&content),
        AiToolTarget::Codex => install_codex(&content),
        AiToolTarget::Openclaw => install_openclaw(&content),
    }
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
            eprintln!(
                "Reload OpenClaw to activate: start a new chat or run `openclaw gateway restart`."
            );
            eprintln!("Verify: `openclaw skills check` (should show NyxID as ready).");
        }
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
    write_file(&dir.join("tools/services.sh"), &content.services_sh)?;
    write_file(&dir.join("tools/proxy.sh"), &content.proxy_sh)?;

    #[cfg(unix)]
    {
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
