use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sigstore::crypto::{CosignVerificationKey, Signature};
use sigstore::trust::sigstore::SigstoreTrustRoot;
use sigstore_verification::{Attestation, AttestationClient, FetchParams};
use tokio::io::AsyncWriteExt;
use x509_cert::{Certificate, der::Decode};

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

    if release.tag_name == installed {
        println!("Status: up to date");
    } else if asset_available {
        println!("Status: update available");
    } else {
        println!("Status: prebuilt binary unavailable; update will fall back to --from-source");
    }

    Ok(())
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

    match verify_release_attestation(&archive_path, &release.tag_name).await {
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

async fn verify_release_attestation(archive_path: &Path, tag: &str) -> Result<()> {
    eprintln!("Verifying GitHub artifact attestation...");

    // Force loading the public-good Sigstore trust root. The GitHub verifier
    // below uses sigstore-rs primitives too, but this keeps trust-root failures
    // as hard failures instead of letting any downstream fallback continue.
    SigstoreTrustRoot::new(None)
        .await
        .context("Failed to load Sigstore public-good trust root")?;

    let digest = sha256_file_hex(archive_path)
        .with_context(|| format!("Failed to hash {}", archive_path.display()))?;
    let mut client_builder = AttestationClient::builder();
    if let Some(token) = github_token() {
        client_builder = client_builder.github_token(&token);
    }
    let client = client_builder
        .build()
        .context("Failed to build GitHub attestation client")?;
    let attestations = client
        .fetch_attestations(FetchParams {
            owner: GITHUB_OWNER.to_string(),
            repo: Some(format!("{GITHUB_OWNER}/{GITHUB_REPO}")),
            digest: format!("sha256:{digest}"),
            limit: 30,
            predicate_type: None,
        })
        .await
        .with_context(|| format!("Failed to fetch GitHub attestations for sha256:{digest}"))?;

    if attestations.is_empty() {
        anyhow::bail!("No GitHub artifact attestations found for sha256:{digest}");
    }

    let expected_identity = expected_workflow_identity(tag);
    let mut failures = Vec::new();

    for attestation in &attestations {
        match verify_single_release_attestation(
            attestation,
            archive_path,
            &digest,
            &expected_identity,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(err) => failures.push(format!("{err:#}")),
        }
    }

    anyhow::bail!(
        "No valid release attestation matched expected workflow identity {expected_identity}. Verification failures: {}",
        failures.join(" | ")
    );
}

async fn verify_single_release_attestation(
    attestation: &Attestation,
    archive_path: &Path,
    expected_digest: &str,
    expected_identity: &str,
) -> Result<()> {
    let parsed = sigstore_verification::bundle::parse_bundle(attestation)
        .context("Failed to parse Sigstore bundle")?;
    let certificate = parsed
        .certificate
        .as_deref()
        .context("Attestation bundle did not include a signing certificate")?;
    let envelope = parsed
        .dsse_envelope
        .as_ref()
        .context("Attestation bundle did not include a DSSE envelope")?;

    verify_attestation_identity(certificate, expected_identity)?;
    verify_payload_subject_digest(&parsed.payload, expected_digest)?;
    verify_dsse_signature(certificate, envelope, &parsed.payload)?;

    sigstore_verification::verify::verify_attestations(
        std::slice::from_ref(attestation),
        archive_path,
        Some(expected_identity),
    )
    .await
    .context("Sigstore bundle verification failed")?;

    Ok(())
}

fn verify_attestation_identity(certificate: &str, expected_identity: &str) -> Result<()> {
    let cert_info = sigstore_verification::verify::verify_certificate(certificate)
        .context("Failed to parse attestation signing certificate")?;
    let actual_identity = cert_info
        .workflow_ref
        .as_deref()
        .context("Attestation certificate did not contain a GitHub workflow identity")?;

    if actual_identity != expected_identity {
        anyhow::bail!(
            "Attestation workflow identity mismatch: expected {expected_identity}, got {actual_identity}"
        );
    }

    if !is_fulcio_issuer(&cert_info.issuer) {
        anyhow::bail!(
            "Attestation certificate issuer mismatch: expected Sigstore Fulcio, got {}",
            cert_info.issuer
        );
    }

    Ok(())
}

fn verify_payload_subject_digest(payload: &[u8], expected_digest: &str) -> Result<()> {
    let statement: Value =
        serde_json::from_slice(payload).context("Failed to parse attestation payload")?;
    let subjects = statement
        .get("subject")
        .and_then(Value::as_array)
        .context("Attestation payload did not include subject entries")?;

    let found = subjects.iter().any(|subject| {
        subject
            .get("digest")
            .and_then(|digest| digest.get("sha256"))
            .and_then(Value::as_str)
            == Some(expected_digest)
    });

    if !found {
        anyhow::bail!("Attestation subject digest mismatch: expected sha256:{expected_digest}");
    }

    Ok(())
}

fn verify_dsse_signature(
    certificate: &str,
    envelope: &sigstore_verification::api::DsseEnvelope,
    payload: &[u8],
) -> Result<()> {
    let cert_bytes = base64::engine::general_purpose::STANDARD
        .decode(certificate)
        .context("Failed to decode attestation certificate")?;
    let certificate = Certificate::from_der(&cert_bytes)
        .context("Failed to parse attestation certificate DER")?;
    let verification_key =
        CosignVerificationKey::try_from(&certificate.tbs_certificate.subject_public_key_info)
            .context("Failed to extract attestation certificate public key")?;
    let signature = envelope
        .signatures
        .first()
        .context("Attestation DSSE envelope did not include a signature")?;

    let primary_pae = dsse_pae(&envelope.payload_type, payload);
    let primary_result = verification_key.verify_signature(
        Signature::Base64Encoded(signature.sig.as_bytes()),
        &primary_pae,
    );
    if primary_result.is_ok() {
        return Ok(());
    }

    // Some older Sigstore tooling verified the JSON base64 payload string. Keep
    // this compatibility check cryptographic, but only after the DSSE-spec PAE
    // failed.
    let compatibility_pae = dsse_pae(&envelope.payload_type, envelope.payload.as_bytes());
    verification_key
        .verify_signature(
            Signature::Base64Encoded(signature.sig.as_bytes()),
            &compatibility_pae,
        )
        .with_context(|| {
            format!(
                "DSSE signature verification failed: {}; compatibility check also failed",
                primary_result.expect_err("primary result is known to be an error")
            )
        })?;

    Ok(())
}

fn dsse_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut pae = Vec::new();
    pae.extend_from_slice(b"DSSEv1");
    pae.push(b' ');
    pae.extend_from_slice(payload_type.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload_type.as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload.len().to_string().as_bytes());
    pae.push(b' ');
    pae.extend_from_slice(payload);
    pae
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

    let parts = version.split('.').collect::<Vec<_>>();
    let valid = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()));

    if !valid {
        anyhow::bail!("Invalid release version `{version}`; expected X.Y.Z");
    }

    Ok(format!("v{version}"))
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

fn is_fulcio_issuer(issuer: &str) -> bool {
    let issuer = issuer.to_lowercase();
    issuer.contains("sigstore")
        || issuer == "fulcio root ca"
        || issuer.starts_with("fulcio intermediate ")
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
        assert!(normalize_release_tag("1.2").is_err());
        assert!(normalize_release_tag("1.2.x").is_err());
        assert!(normalize_release_tag("release-1.2.3").is_err());
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
        verify_payload_subject_digest(
            payload,
            "8c5b8a213a6d3d0c74a1f3a1c9dbd9ed93094b2b2ca8c7a4d00365bd7a9a6a6b",
        )
        .unwrap();
        assert!(verify_payload_subject_digest(payload, "0000").is_err());
    }

    #[test]
    fn creates_dsse_pae() {
        assert_eq!(
            dsse_pae("application/vnd.in-toto+json", b"hello"),
            b"DSSEv1 28 application/vnd.in-toto+json 5 hello"
        );
    }
}
