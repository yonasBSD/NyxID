use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::{AuthArgs, LoginArgs};

const TOKEN_DIR_NAME: &str = ".nyxid";
const TOKEN_FILE_NAME: &str = "access_token";
const REFRESH_TOKEN_FILE_NAME: &str = "refresh_token";
const BASE_URL_FILE_NAME: &str = "base_url";
const CALLBACK_TIMEOUT_SECS: u64 = 120;

// ---- Token storage ----

fn token_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(TOKEN_DIR_NAME))
}

fn token_file_path() -> Result<PathBuf> {
    Ok(token_dir()?.join(TOKEN_FILE_NAME))
}

fn refresh_token_file_path() -> Result<PathBuf> {
    Ok(token_dir()?.join(REFRESH_TOKEN_FILE_NAME))
}

fn base_url_file_path() -> Result<PathBuf> {
    Ok(token_dir()?.join(BASE_URL_FILE_NAME))
}

pub fn read_saved_token() -> Option<String> {
    let path = token_file_path().ok()?;
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

pub fn read_saved_refresh_token() -> Option<String> {
    let path = refresh_token_file_path().ok()?;
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

pub fn read_saved_base_url() -> Option<String> {
    let path = base_url_file_path().ok()?;
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn save_base_url(url: &str) -> Result<()> {
    let path = base_url_file_path()?;
    let dir = path.parent().context("Invalid token directory")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, url)?;
    Ok(())
}

fn write_token_file(path: &std::path::Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(path, token)
        .with_context(|| format!("Failed to write token to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

fn save_token(token: &str) -> Result<()> {
    write_token_file(&token_file_path()?, token)
}

pub fn save_refresh_token(token: &str) -> Result<()> {
    write_token_file(&refresh_token_file_path()?, token)
}

/// Save a new access token (and optionally a new refresh token).
pub fn save_tokens(access_token: &str, refresh_token: Option<&str>) -> Result<()> {
    save_token(access_token)?;
    if let Some(rt) = refresh_token {
        save_refresh_token(rt)?;
    }
    Ok(())
}

fn clear_token() -> Result<()> {
    let path = token_file_path()?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    let refresh_path = refresh_token_file_path()?;
    if refresh_path.exists() {
        std::fs::remove_file(&refresh_path)
            .with_context(|| format!("Failed to remove {}", refresh_path.display()))?;
    }
    Ok(())
}

// ---- Token resolution (same 3-step as ssh_cli.rs) ----

pub fn resolve_access_token(auth: &AuthArgs) -> Result<String> {
    // 1. Explicit --access-token flag
    if let Some(token) = &auth.access_token {
        return Ok(token.clone());
    }

    // 2. Environment variable (NYXID_ACCESS_TOKEN by default)
    if let Ok(token) = std::env::var(&auth.access_token_env)
        && !token.is_empty()
    {
        return Ok(token);
    }

    // 3. Saved token from `nyxid login`
    if let Some(token) = read_saved_token() {
        return Ok(token);
    }

    bail!(
        "No access token found. Run `nyxid login --base-url <URL>`, \
         set {}, or pass --access-token",
        auth.access_token_env
    )
}

// ---- Login ----

pub async fn run_login(args: LoginArgs) -> Result<()> {
    if args.password {
        return run_password_login(&args.base_url, args.email.as_deref()).await;
    }
    run_browser_login(&args.base_url).await
}

// ---- Logout ----

pub async fn run_logout(base_url: &str) -> Result<()> {
    let base_url = base_url.trim_end_matches('/');

    // Best-effort server-side logout
    if let Some(token) = read_saved_token() {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let _ = client
            .post(format!("{base_url}/api/v1/auth/logout"))
            .bearer_auth(&token)
            .send()
            .await;
    }

    clear_token()?;
    eprintln!("Logged out. Token cleared.");
    Ok(())
}

// ---- Browser login (ported from login_cli.rs) ----

async fn run_browser_login(base_url: &str) -> Result<()> {
    let base_url = base_url.trim_end_matches('/');

    let frontend_url = fetch_frontend_url(base_url).await?;

    let listener =
        TcpListener::bind("127.0.0.1:0").context("Failed to bind local callback server")?;
    let port = listener.local_addr()?.port();

    let state = generate_state();
    let auth_url = format!("{frontend_url}/cli-auth?port={port}&state={state}");

    eprintln!("Opening browser to log in...");
    eprintln!();
    eprintln!("If the browser does not open, visit:");
    eprintln!("  {auth_url}");
    eprintln!();

    let _ = open::that(&auth_url);

    let callback = wait_for_callback(listener, &state).await?;
    save_tokens(&callback.access_token, callback.refresh_token.as_deref())?;
    save_base_url(base_url)?;

    eprintln!("Logged in successfully.");
    eprintln!("Token saved to {}", token_file_path()?.display());

    Ok(())
}

struct CallbackTokens {
    access_token: String,
    refresh_token: Option<String>,
}

async fn wait_for_callback(listener: TcpListener, expected_state: &str) -> Result<CallbackTokens> {
    listener
        .set_nonblocking(true)
        .context("Failed to set listener to non-blocking")?;
    let listener =
        tokio::net::TcpListener::from_std(listener).context("Failed to create async listener")?;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            accept = listener.accept() => {
                let (mut stream, _) = accept.context("Failed to accept connection")?;
                let mut buf = vec![0u8; 8192];
                let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await
                    .context("Failed to read request")?;
                let request = String::from_utf8_lossy(&buf[..n]);

                if let Some(tokens) = parse_callback_request(&request, expected_state) {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n{}",
                        callback_success_html()
                    );
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
                    return Ok(tokens);
                }

                let response = "HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
            }
            () = &mut timeout => {
                bail!("Login timed out after {CALLBACK_TIMEOUT_SECS}s. Please try again.");
            }
        }
    }
}

fn parse_callback_request(request: &str, expected_state: &str) -> Option<CallbackTokens> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    if !path.starts_with("/callback") {
        return None;
    }

    let query = path.split('?').nth(1)?;
    let params: std::collections::HashMap<&str, &str> = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            Some((parts.next()?, parts.next()?))
        })
        .collect();

    let state = *params.get("state")?;
    if state != expected_state {
        return None;
    }

    let token = *params.get("access_token")?;
    if token.is_empty() {
        return None;
    }

    let access_token = urlencoding::decode(token).ok()?.into_owned();
    let refresh_token = params
        .get("refresh_token")
        .and_then(|t| urlencoding::decode(t).ok())
        .map(|t| t.into_owned())
        .filter(|t| !t.is_empty());

    Some(CallbackTokens {
        access_token,
        refresh_token,
    })
}

fn generate_state() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn callback_success_html() -> &'static str {
    r#"<!doctype html>
<html>
<head><title>NyxID CLI</title></head>
<body style="display:flex;align-items:center;justify-content:center;min-height:100vh;font-family:system-ui;background:#0f172a;color:#e2e8f0">
<div style="text-align:center">
<h2>Login successful</h2>
<p style="color:#94a3b8">You can close this tab and return to your terminal.</p>
</div>
</body>
</html>"#
}

#[derive(Deserialize)]
struct PublicConfig {
    frontend_url: String,
}

pub async fn fetch_frontend_url(base_url: &str) -> Result<String> {
    let config_url = format!("{base_url}/api/v1/public/config");
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let config: PublicConfig = client
        .get(&config_url)
        .send()
        .await
        .context("Failed to reach NyxID server")?
        .json()
        .await
        .context("Failed to parse server config")?;

    Ok(config.frontend_url.trim_end_matches('/').to_string())
}

// ---- Password login (ported from login_cli.rs) ----

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
    refresh_token: Option<String>,
}

async fn run_password_login(base_url: &str, email: Option<&str>) -> Result<()> {
    let email = match email {
        Some(email) => email.to_string(),
        None => {
            eprint!("Email: ");
            std::io::stderr().flush()?;
            let mut email = String::new();
            std::io::stdin()
                .read_line(&mut email)
                .context("Failed to read email")?;
            email.trim().to_string()
        }
    };

    if email.is_empty() {
        bail!("Email is required");
    }

    let password = rpassword::prompt_password("Password: ").context("Failed to read password")?;
    if password.is_empty() {
        bail!("Password is required");
    }

    let base_url = base_url.trim_end_matches('/');
    let login_url = format!("{base_url}/api/v1/auth/login");

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .post(&login_url)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
            "client": "cli",
        }))
        .send()
        .await
        .context("Failed to connect to NyxID server")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Login failed (HTTP {status}): {body}");
    }

    let login: LoginResponse = response
        .json()
        .await
        .context("Failed to parse login response")?;

    save_tokens(&login.access_token, login.refresh_token.as_deref())?;
    save_base_url(base_url)?;

    eprintln!("Logged in as {email}");
    eprintln!("Token saved to {}", token_file_path()?.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        callback_success_html, parse_callback_request, refresh_token_file_path, token_file_path,
    };

    #[test]
    fn token_path_is_under_home() {
        let path = token_file_path().expect("token path");
        assert!(path.to_string_lossy().contains(".nyxid"));
        assert!(path.to_string_lossy().ends_with("access_token"));
    }

    #[test]
    fn refresh_token_path_is_under_home() {
        let path = refresh_token_file_path().expect("refresh token path");
        assert!(path.to_string_lossy().contains(".nyxid"));
        assert!(path.to_string_lossy().ends_with("refresh_token"));
    }

    #[test]
    fn parses_valid_callback_request() {
        let request =
            "GET /callback?access_token=tok_abc123&state=deadbeef HTTP/1.1\r\nHost: 127.0.0.1\r\n";
        let result = parse_callback_request(request, "deadbeef").expect("should parse");
        assert_eq!(result.access_token, "tok_abc123");
        assert!(result.refresh_token.is_none());
    }

    #[test]
    fn parses_callback_with_refresh_token() {
        let request = "GET /callback?access_token=tok_abc&refresh_token=ref_xyz&state=deadbeef HTTP/1.1\r\nHost: 127.0.0.1\r\n";
        let result = parse_callback_request(request, "deadbeef").expect("should parse");
        assert_eq!(result.access_token, "tok_abc");
        assert_eq!(result.refresh_token.as_deref(), Some("ref_xyz"));
    }

    #[test]
    fn rejects_wrong_state() {
        let request =
            "GET /callback?access_token=tok_abc123&state=wrong HTTP/1.1\r\nHost: 127.0.0.1\r\n";
        assert!(parse_callback_request(request, "deadbeef").is_none());
    }

    #[test]
    fn rejects_non_callback_path() {
        let request = "GET /other?access_token=tok_abc123&state=deadbeef HTTP/1.1\r\n";
        assert!(parse_callback_request(request, "deadbeef").is_none());
    }

    #[test]
    fn success_html_is_not_empty() {
        assert!(callback_success_html().contains("Login successful"));
    }
}
