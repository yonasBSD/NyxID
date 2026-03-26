use anyhow::Result;
use serde_json::Value;

use crate::api::ApiClient;
use crate::cli::{MfaCommands, OutputFormat};

pub async fn run(command: MfaCommands) -> Result<()> {
    match command {
        MfaCommands::Setup { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let result: Value = api.post("/mfa/setup", &serde_json::json!({})).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let secret = result["secret"].as_str().unwrap_or("-");
                    let qr_url = result["qr_code_url"]
                        .as_str()
                        .or(result["otpauth_url"].as_str())
                        .unwrap_or("-");

                    eprintln!("MFA Setup");
                    eprintln!();
                    eprintln!("Secret:  {secret}");
                    eprintln!("QR URL:  {qr_url}");
                    eprintln!();
                    eprintln!("1. Add this secret to your authenticator app (or scan the QR URL)");
                    eprintln!("2. Verify with: nyxid mfa verify --code <TOTP_CODE>");
                }
            }
            Ok(())
        }

        MfaCommands::Verify { code, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let body = serde_json::json!({ "code": code });
            let result: Value = api.post("/mfa/verify-setup", &body).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    eprintln!("MFA enabled successfully.");
                    if let Some(codes) = result["recovery_codes"].as_array()
                        && !codes.is_empty()
                    {
                        eprintln!();
                        eprintln!("Recovery codes (save these securely):");
                        for code in codes {
                            if let Some(c) = code.as_str() {
                                eprintln!("  {c}");
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        MfaCommands::Status { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let user: Value = api.get("/users/me").await?;

            match auth.output {
                OutputFormat::Json => {
                    let status = serde_json::json!({
                        "mfa_enabled": user["mfa_enabled"],
                    });
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                OutputFormat::Table => {
                    let enabled = user["mfa_enabled"].as_bool().unwrap_or(false);
                    let status = if enabled { "enabled" } else { "disabled" };
                    eprintln!("MFA: {status}");
                    if !enabled {
                        eprintln!();
                        eprintln!("Enable with: nyxid mfa setup");
                    }
                }
            }
            Ok(())
        }
    }
}
