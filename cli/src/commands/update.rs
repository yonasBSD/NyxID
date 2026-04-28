use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::UpdateArgs;
use crate::commands::repo::REPO_URL;

pub async fn run(args: UpdateArgs) -> Result<()> {
    if args.skills_only {
        return update_skills(&args.base_url).await;
    }

    update_cli().await?;

    // Hand off the skills phase to the freshly-installed binary so it always
    // runs against the latest fetch / install logic, even when the running
    // process was launched from an older binary that predates new skill paths.
    if let Some(new_bin) = find_new_binary() {
        eprintln!("Handing off to {} for skill update...", new_bin.display());
        return exec_skills_update(&new_bin, &args.base_url);
    }

    eprintln!(
        "Warning: could not locate the freshly-installed nyxid binary; \
         continuing with the in-process skill update."
    );
    update_skills(&args.base_url).await?;

    eprintln!();
    eprintln!("All up to date.");
    Ok(())
}

async fn update_cli() -> Result<()> {
    eprintln!("Updating NyxID CLI...");

    let status = tokio::process::Command::new("cargo")
        .args([
            "install",
            "--git",
            REPO_URL,
            "nyxid-cli",
            "--force",
            "--locked",
        ])
        .status()
        .await
        .context("Failed to run cargo install. Is cargo available?")?;

    if !status.success() {
        anyhow::bail!("cargo install failed with exit code {}", status);
    }

    eprintln!("CLI updated.");
    Ok(())
}

async fn update_skills(base_url: &Option<String>) -> Result<()> {
    // Reuse the ai-setup update logic (updates all installed tools)
    super::ai_setup::run(crate::cli::AiSetupCommands::Update {
        tool: None,
        base_url: base_url.clone(),
    })
    .await
}

/// Locate the freshly-installed `nyxid` binary that `cargo install` just wrote.
/// Always points at `${CARGO_HOME:-~/.cargo}/bin/nyxid`, which is the same
/// physical file the `~/.local/bin/nyxid` symlink (set up by `install.sh`) ends
/// up resolving to.
fn find_new_binary() -> Option<PathBuf> {
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))?;

    let exe = if cfg!(windows) { "nyxid.exe" } else { "nyxid" };
    let path = cargo_home.join("bin").join(exe);
    path.exists().then_some(path)
}

/// Replace the current process with `<new_bin> update --skills-only [--base-url X]`.
/// On Unix this `exec`s in place; on Windows we spawn + wait + propagate the
/// exit code since `exec` semantics aren't available.
fn exec_skills_update(new_bin: &PathBuf, base_url: &Option<String>) -> Result<()> {
    let mut cmd = std::process::Command::new(new_bin);
    cmd.arg("update").arg("--skills-only");
    if let Some(url) = base_url {
        cmd.arg("--base-url").arg(url);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // On success, exec replaces the process and never returns.
        let err = cmd.exec();
        Err(anyhow::anyhow!(
            "Failed to exec {}: {err}",
            new_bin.display()
        ))
    }
    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .with_context(|| format!("Failed to spawn {}", new_bin.display()))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
