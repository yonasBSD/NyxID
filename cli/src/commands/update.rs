use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::cli::UpdateArgs;
use crate::commands::repo::REPO_URL;

const GITHUB_API_URL: &str = "https://api.github.com";
const GITHUB_OWNER: &str = "ChronoAIProject";
const GITHUB_REPO: &str = "NyxID";
const RELEASE_WORKFLOW_PATH: &str = ".github/workflows/release.yml";
const DIST_PACKAGE_NAME: &str = "nyxid-cli";

pub async fn run(args: UpdateArgs) -> Result<()> {
    if args.skills_only {
        return update_skills(&args.base_url).await;
    }

    if args.check {
        return check_cli_update(&args).await;
    }

    let replaced_binary = update_cli(&args).await?;

    // Hand off the skills phase to the freshly-installed binary so it always
    // runs against the latest fetch / install logic, even when the running
    // process was launched from an older binary that predates new skill paths.
    if let Some(new_bin) = find_new_binary(replaced_binary.as_deref()) {
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

async fn check_cli_update(args: &UpdateArgs) -> Result<()> {
    let client = github_client()?;
    let release = resolve_release(&client, args.version.as_deref()).await?;
    let installed = format!("v{}", env!("CARGO_PKG_VERSION"));

    println!("Installed: {installed}");

    let Some(release) = release else {
        println!("Latest: unavailable");
        println!("Status: no NyxID GitHub releases were found yet");
        return Ok(());
    };

    let target = current_target();
    let asset_name = asset_name_for_target(target)?;
    let asset_available = release_asset(&release, &asset_name).is_some();
    let label = if args.version.is_some() {
        "Requested"
    } else {
        "Latest"
    };

    println!("{label}: {}", release.tag_name);
    println!("Target: {target}");
    println!(
        "Binary asset: {}",
        if asset_available {
            asset_name
        } else {
            format!("{asset_name} (not found)")
        }
    );

    println!(
        "Status: {}",
        cli_update_status(&installed, &release.tag_name, asset_available)
    );

    Ok(())
}

fn cli_update_status(installed: &str, release_tag: &str, asset_available: bool) -> &'static str {
    if release_tag == installed && !asset_available {
        "up to date (no prebuilt asset for this target; future updates will fall back to --from-source)"
    } else if release_tag == installed {
        "up to date"
    } else if asset_available {
        "update available"
    } else {
        "prebuilt binary unavailable; update will fall back to --from-source"
    }
}

async fn update_cli(args: &UpdateArgs) -> Result<Option<PathBuf>> {
    if args.from_source {
        update_cli_from_source().await?;
        return Ok(None);
    }

    eprintln!("Updating NyxID CLI...");

    let client = github_client()?;
    let Some(release) = resolve_release(&client, args.version.as_deref()).await? else {
        eprintln!("No NyxID GitHub releases were found yet; falling back to source install.");
        update_cli_from_source().await?;
        return Ok(None);
    };

    let target = current_target();
    let asset_name = asset_name_for_target(target)?;
    let Some(asset) = release_asset(&release, &asset_name) else {
        eprintln!(
            "No prebuilt binary asset named {asset_name} exists on {}; falling back to source install.",
            release.tag_name
        );
        update_cli_from_source().await?;
        return Ok(None);
    };

    let tmp = tempfile::tempdir().context("Failed to create temporary update directory")?;
    let archive_path = tmp.path().join(&asset.name);

    eprintln!("Downloading {}...", asset.name);
    download_asset(&client, asset, &archive_path).await?;

    match verify_release_attestation(&client, &archive_path, &release.tag_name).await {
        Ok(()) => eprintln!("Release attestation verified."),
        Err(err) if args.insecure_skip_verify => {
            eprintln!();
            eprintln!("WARNING: release attestation verification failed.");
            eprintln!("WARNING: continuing because --insecure-skip-verify was provided.");
            eprintln!("WARNING: provenance error: {err:#}");
            eprintln!();
        }
        Err(err) => {
            return Err(err).context(
                "Release attestation verification failed; refusing to install this binary",
            );
        }
    }

    let extract_dir = tmp.path().join("extract");
    fs::create_dir_all(&extract_dir).context("Failed to create extraction directory")?;
    let new_bin = extract_binary(&archive_path, &extract_dir)?;
    let replaced_path = replace_current_binary(&new_bin)?;

    eprintln!("CLI updated to {}.", release.tag_name);
    Ok(Some(replaced_path))
}

async fn update_cli_from_source() -> Result<()> {
    eprintln!("Updating NyxID CLI from source...");

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

    eprintln!("CLI updated from source.");
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

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn github_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!(
            "nyxid/{} ({GITHUB_OWNER}/{GITHUB_REPO})",
            env!("CARGO_PKG_VERSION")
        ))
        .context("Failed to build GitHub User-Agent header")?,
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        HeaderValue::from_static("2022-11-28"),
    );

    if let Some(token) = github_token() {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .context("Failed to build GitHub Authorization header")?,
        );
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("Failed to build GitHub HTTP client")
}

fn github_token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty())
}

async fn resolve_release(
    client: &reqwest::Client,
    version: Option<&str>,
) -> Result<Option<GitHubRelease>> {
    let url = if let Some(version) = version {
        let tag = normalize_release_tag(version)?;
        format!("{GITHUB_API_URL}/repos/{GITHUB_OWNER}/{GITHUB_REPO}/releases/tags/{tag}")
    } else {
        format!("{GITHUB_API_URL}/repos/{GITHUB_OWNER}/{GITHUB_REPO}/releases/latest")
    };

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to query GitHub release API: {url}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        if version.is_some() {
            anyhow::bail!("Requested NyxID release was not found: {url}");
        }
        return Ok(None);
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("GitHub release API returned an error for {url}"))?;

    response
        .json::<GitHubRelease>()
        .await
        .context("Failed to parse GitHub release response")
        .map(Some)
}

fn release_asset<'a>(release: &'a GitHubRelease, asset_name: &str) -> Option<&'a GitHubAsset> {
    release.assets.iter().find(|asset| asset.name == asset_name)
}

async fn download_asset(
    client: &reqwest::Client,
    asset: &GitHubAsset,
    destination: &Path,
) -> Result<()> {
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .with_context(|| format!("Failed to download {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("GitHub returned an error while downloading {}", asset.name))?;

    let mut file = tokio::fs::File::create(destination)
        .await
        .with_context(|| format!("Failed to create {}", destination.display()))?;

    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("Failed while reading {}", asset.name))?
    {
        file.write_all(&chunk)
            .await
            .with_context(|| format!("Failed while writing {}", destination.display()))?;
    }

    file.flush()
        .await
        .with_context(|| format!("Failed to flush {}", destination.display()))?;
    Ok(())
}

async fn verify_release_attestation(
    client: &reqwest::Client,
    archive_path: &Path,
    tag: &str,
) -> Result<()> {
    eprintln!("Verifying GitHub artifact attestation...");

    let digest = sha256_file_hex(archive_path)
        .with_context(|| format!("Failed to hash {}", archive_path.display()))?;
    let expected_identity = expected_workflow_identity(tag);
    super::update_attestation::verify_release_attestation(
        client,
        GITHUB_OWNER,
        GITHUB_REPO,
        &digest,
        &expected_identity,
    )
    .await
    .context("Sigstore bundle verification failed")
}

fn extract_binary(archive_path: &Path, extract_dir: &Path) -> Result<PathBuf> {
    let bin_name = archive_binary_name();
    self_update::Extract::from_source(archive_path)
        .extract_file(extract_dir, bin_name)
        .with_context(|| {
            format!(
                "Failed to extract {bin_name} from {}",
                archive_path.display()
            )
        })?;

    let new_bin = extract_dir.join(bin_name);
    if !new_bin.exists() {
        anyhow::bail!(
            "Release archive did not contain expected binary path {}",
            new_bin.display()
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new_bin, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to set executable mode on {}", new_bin.display()))?;
    }

    Ok(new_bin)
}

fn replace_current_binary(new_bin: &Path) -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("Failed to locate current nyxid binary")?;
    self_replace::self_replace(new_bin)
        .with_context(|| format!("Failed to replace {}", current_exe.display()))?;
    Ok(current_exe)
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn expected_workflow_identity(tag: &str) -> String {
    format!(
        "https://github.com/{GITHUB_OWNER}/{GITHUB_REPO}/{RELEASE_WORKFLOW_PATH}@refs/tags/{tag}"
    )
}

fn normalize_release_tag(version: &str) -> Result<String> {
    let version = version.trim();
    let version = version.strip_prefix('v').unwrap_or(version);

    if !is_valid_semver_tag(version) {
        anyhow::bail!(
            "Invalid release version `{version}`; expected SemVer X.Y.Z with optional pre-release/build metadata"
        );
    }

    Ok(format!("v{version}"))
}

fn is_valid_semver_tag(version: &str) -> bool {
    let Some((without_build, build_metadata)) = split_optional_once(version, '+') else {
        return false;
    };
    if let Some(build_metadata) = build_metadata
        && !valid_semver_identifiers(build_metadata, false)
    {
        return false;
    }

    let (core, pre_release) = match without_build.split_once('-') {
        Some((core, pre_release)) => (core, Some(pre_release)),
        None => (without_build, None),
    };
    valid_semver_core(core)
        && pre_release
            .map(|pre_release| valid_semver_identifiers(pre_release, true))
            .unwrap_or(true)
}

fn split_optional_once(input: &str, separator: char) -> Option<(&str, Option<&str>)> {
    match input.split_once(separator) {
        Some((_, right)) if right.contains(separator) => None,
        Some((left, right)) => Some((left, Some(right))),
        None => Some((input, None)),
    }
}

fn valid_semver_core(core: &str) -> bool {
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| valid_numeric_identifier(part, true))
}

fn valid_semver_identifiers(identifiers: &str, reject_numeric_leading_zero: bool) -> bool {
    !identifiers.is_empty()
        && identifiers.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                && (!reject_numeric_leading_zero
                    || !identifier.chars().all(|ch| ch.is_ascii_digit())
                    || valid_numeric_identifier(identifier, true))
        })
}

fn valid_numeric_identifier(identifier: &str, reject_leading_zero: bool) -> bool {
    !identifier.is_empty()
        && identifier.chars().all(|ch| ch.is_ascii_digit())
        && (!reject_leading_zero || identifier.len() == 1 || !identifier.starts_with('0'))
}

fn asset_name_for_target(target: &str) -> Result<String> {
    if target.is_empty() || target == "unknown" {
        anyhow::bail!("Current build target is unavailable");
    }

    let extension = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    Ok(format!("{DIST_PACKAGE_NAME}-{target}.{extension}"))
}

fn current_target() -> &'static str {
    env!("TARGET")
}

fn archive_binary_name() -> &'static str {
    if cfg!(windows) { "nyxid.exe" } else { "nyxid" }
}

/// Locate the freshly-installed `nyxid` binary.
///
/// For prebuilt updates this prefers the current executable path that was just
/// replaced in place. For `--from-source`, it falls back to the cargo install
/// location.
fn find_new_binary(preferred: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = preferred
        && path.exists()
    {
        return Some(path.to_path_buf());
    }

    if let Ok(path) = std::env::current_exe()
        && path.exists()
    {
        return Some(path);
    }

    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))?;

    let path = cargo_home.join("bin").join(archive_binary_name());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_dist_asset_names() {
        assert_eq!(
            asset_name_for_target("x86_64-unknown-linux-gnu").unwrap(),
            "nyxid-cli-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            asset_name_for_target("aarch64-unknown-linux-gnu").unwrap(),
            "nyxid-cli-aarch64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            asset_name_for_target("x86_64-apple-darwin").unwrap(),
            "nyxid-cli-x86_64-apple-darwin.tar.gz"
        );
        assert_eq!(
            asset_name_for_target("aarch64-apple-darwin").unwrap(),
            "nyxid-cli-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(
            asset_name_for_target("x86_64-pc-windows-msvc").unwrap(),
            "nyxid-cli-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn parses_release_version_tags() {
        assert_eq!(normalize_release_tag("0.4.0").unwrap(), "v0.4.0");
        assert_eq!(normalize_release_tag("v1.2.3").unwrap(), "v1.2.3");
        assert_eq!(
            normalize_release_tag("0.4.0-beta.1").unwrap(),
            "v0.4.0-beta.1"
        );
        assert_eq!(
            normalize_release_tag("1.2.3-rc.1+build.42").unwrap(),
            "v1.2.3-rc.1+build.42"
        );
        assert!(normalize_release_tag("1.2").is_err());
        assert!(normalize_release_tag("1.2.x").is_err());
        assert!(normalize_release_tag("1.2.3-0123").is_err());
        assert!(normalize_release_tag("1.2.3-").is_err());
        assert!(normalize_release_tag("release-1.2.3").is_err());
    }

    #[test]
    fn check_status_mentions_missing_asset_when_installed_release_has_no_binary() {
        assert_eq!(
            cli_update_status("v0.4.0", "v0.4.0", false),
            "up to date (no prebuilt asset for this target; future updates will fall back to --from-source)"
        );
        assert_eq!(cli_update_status("v0.4.0", "v0.4.0", true), "up to date");
        assert_eq!(
            cli_update_status("v0.4.0", "v0.5.0", true),
            "update available"
        );
        assert_eq!(
            cli_update_status("v0.4.0", "v0.5.0", false),
            "prebuilt binary unavailable; update will fall back to --from-source"
        );
    }

    #[test]
    fn builds_expected_workflow_identity() {
        assert_eq!(
            expected_workflow_identity("v1.2.3"),
            "https://github.com/ChronoAIProject/NyxID/.github/workflows/release.yml@refs/tags/v1.2.3"
        );
    }

    #[test]
    fn verifies_fixture_attestation_subject_digest() {
        let payload =
            include_bytes!("../../tests/fixtures/update-attestation-statement.json").as_slice();
        super::super::update_attestation::verify_payload_subject_digest(
            payload,
            "8c5b8a213a6d3d0c74a1f3a1c9dbd9ed93094b2b2ca8c7a4d00365bd7a9a6a6b",
        )
        .unwrap();
        assert!(
            super::super::update_attestation::verify_payload_subject_digest(payload, "0000")
                .is_err()
        );
    }

    #[test]
    fn creates_dsse_pae() {
        assert_eq!(
            super::super::update_attestation::dsse_pae("application/vnd.in-toto+json", b"hello"),
            b"DSSEv1 28 application/vnd.in-toto+json 5 hello"
        );
    }
}
