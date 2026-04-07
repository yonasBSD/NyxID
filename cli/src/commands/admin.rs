use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use serde_json::{Value, json};

use crate::api::ApiClient;
use crate::cli::{AdminCommands, InviteCodeCommands, OutputFormat};

pub async fn run(command: AdminCommands) -> Result<()> {
    match command {
        AdminCommands::InviteCode { command } => run_invite_code(command).await,
    }
}

async fn run_invite_code(command: InviteCodeCommands) -> Result<()> {
    match command {
        InviteCodeCommands::Create {
            max_uses,
            note,
            auth,
        } => {
            let mut api = ApiClient::from_auth(&auth)?;

            let mut body = json!({});
            if let Some(n) = max_uses {
                body["max_uses"] = json!(n);
            }
            if let Some(ref n) = note {
                body["note"] = json!(n);
            }

            let result: Value = api.post("/admin/invite-codes", &body).await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let code = result["code"].as_str().unwrap_or("-");
                    let id = result["id"].as_str().unwrap_or("-");
                    let max = result["max_uses"].as_i64().unwrap_or(0);
                    let used = result["used_count"].as_i64().unwrap_or(0);
                    let note_display = result["note"].as_str().unwrap_or("-");

                    eprintln!("Invite code created.");
                    eprintln!();
                    eprintln!("Code:     {code}");
                    eprintln!("ID:       {id}");
                    eprintln!("Uses:     {used}/{max}");
                    eprintln!("Note:     {note_display}");
                    eprintln!();
                    eprintln!("Share the code with the user who should register.");
                }
            }
            Ok(())
        }

        InviteCodeCommands::List { auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            let result: Value = api.get("/admin/invite-codes").await?;

            match auth.output {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Table => {
                    let items = result
                        .get("invite_codes")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    if items.is_empty() {
                        eprintln!("No invite codes.");
                        return Ok(());
                    }

                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL_CONDENSED);
                    table.set_header(["ID", "Code", "Uses", "Active", "Note", "Created"]);

                    for ic in items {
                        let id = ic["id"].as_str().unwrap_or("-");
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
                        let code = ic["code"].as_str().unwrap_or("-");
                        let used = ic["used_count"].as_i64().unwrap_or(0);
                        let max = ic["max_uses"].as_i64().unwrap_or(0);
                        let uses = format!("{used}/{max}");
                        let active = if ic["is_active"].as_bool().unwrap_or(false) {
                            "yes"
                        } else {
                            "no"
                        };
                        let note = ic["note"].as_str().unwrap_or("-");
                        let created = ic["created_at"].as_str().unwrap_or("-");
                        let short_created = created.get(..10).unwrap_or(created);
                        table.add_row([short_id, code, uses.as_str(), active, note, short_created]);
                    }
                    eprintln!("{table}");
                }
            }
            Ok(())
        }

        InviteCodeCommands::Deactivate { id, auth } => {
            let mut api = ApiClient::from_auth(&auth)?;
            api.delete_empty(&format!("/admin/invite-codes/{id}"))
                .await?;
            eprintln!("Invite code {id} deactivated.");
            Ok(())
        }
    }
}
