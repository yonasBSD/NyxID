use anyhow::Result;

use crate::api::ApiClient;
use crate::cli::{McpCommands, OutputFormat};

pub async fn run(command: McpCommands) -> Result<()> {
    match command {
        McpCommands::Config { tool, auth } => {
            let api = ApiClient::from_auth(&auth)?;
            let base = api.base_url_root();
            let mcp_url = format!("{base}/mcp");

            match auth.output {
                OutputFormat::Json => {
                    let config = serde_json::json!({
                        "mcp_url": mcp_url,
                        "tool": tool,
                        "base_url": base,
                    });
                    println!("{}", serde_json::to_string_pretty(&config)?);
                }
                OutputFormat::Table => match tool.as_str() {
                    "cursor" => {
                        println!("{{");
                        println!("  \"mcpServers\": {{");
                        println!("    \"nyxid\": {{");
                        println!("      \"url\": \"{mcp_url}\",");
                        println!(
                            "      \"headers\": {{ \"Authorization\": \"Bearer ${{NYXID_API_KEY}}\" }}"
                        );
                        println!("    }}");
                        println!("  }}");
                        println!("}}");
                    }
                    "claude-code" => {
                        println!("claude mcp add nyxid --transport streamable-http \\");
                        println!("  --url \"{mcp_url}\" \\");
                        println!("  --header \"Authorization: Bearer ${{NYXID_API_KEY}}\"");
                    }
                    "vscode" => {
                        println!("{{");
                        println!("  \"mcp\": {{");
                        println!("    \"servers\": {{");
                        println!("      \"nyxid\": {{");
                        println!("        \"type\": \"http\",");
                        println!("        \"url\": \"{mcp_url}\",");
                        println!("        \"headers\": {{");
                        println!("          \"Authorization\": \"Bearer ${{NYXID_API_KEY}}\"");
                        println!("        }}");
                        println!("      }}");
                        println!("    }}");
                        println!("  }}");
                        println!("}}");
                    }
                    _ => {
                        println!("MCP Server URL: {mcp_url}");
                        println!("Authorization:  Bearer <your-api-key>");
                        println!();
                        println!("Create an API key:");
                        println!(
                            "  nyxid api-key create --name \"MCP Key\" --scopes \"read write proxy\" --base-url {}",
                            auth.resolved_base_url()?
                        );
                    }
                },
            }
            Ok(())
        }
    }
}
