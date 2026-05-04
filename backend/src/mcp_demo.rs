//! MCP stdio server with a curated, hardcoded tool surface.
//!
//! Used by directory listings (Glama) to introspect a fixed `tools/list`
//! response for scoring without provisioning MongoDB, OAuth, or a real
//! user. The tools mirror NyxID's gateway API but are not wired to any
//! backing service — production clients use the authenticated
//! Streamable HTTP transport at `/mcp` on a real NyxID deployment.
//!
//! The transport is JSON-RPC 2.0 over newline-delimited stdin/stdout,
//! per the MCP stdio spec. Stderr is reserved for diagnostics so the
//! protocol stream stays clean.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: &str = "2025-03-26";

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

pub async fn run() -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(err) => {
                eprintln!("mcp-demo: failed to parse request: {err}");
                continue;
            }
        };

        // Notifications (no `id`) get no response per JSON-RPC 2.0.
        let Some(id) = req.id else {
            continue;
        };

        let outcome = match req.method.as_str() {
            "initialize" => Ok(handle_initialize()),
            "tools/list" => Ok(handle_tools_list()),
            "tools/call" => Ok(handle_tools_call(&req.params)),
            "ping" => Ok(json!({})),
            other => Err(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {other}"),
            }),
        };

        let resp = match outcome {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(error),
            },
        };

        let serialized = serde_json::to_string(&resp)?;
        stdout.write_all(serialized.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "nyxid",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn handle_tools_list() -> Value {
    json!({ "tools": tool_definitions() })
}

fn handle_tools_call(params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)");
    json!({
        "content": [{
            "type": "text",
            "text": format!(
                "This is the NyxID demo image. The '{name}' tool is exposed for \
                 directory introspection only and is not wired to a backing \
                 service. To use NyxID's full tool surface, run an authenticated \
                 NyxID instance and connect over the Streamable HTTP transport at \
                 /mcp. See https://github.com/ChronoAIProject/NyxID."
            ),
        }],
    })
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "nyx_proxy_request",
            "description": "Forward an HTTP request through NyxID to any downstream service the agent has been granted access to. NyxID injects the appropriate credential at proxy time so the agent never holds raw API keys. Supports cloud APIs (OpenAI, GitHub, Slack, Lark, Telegram, etc.), internal REST endpoints, and localhost services reached over a NAT-pierced credential node. Every call is rate-limited and audit-logged per agent identity.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "service_slug": {
                        "type": "string",
                        "description": "Slug of the connected service (e.g. \"llm-openai\", \"api-github\"). Use nyx_list_services to enumerate the slugs visible to this agent."
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"],
                        "description": "HTTP method for the downstream request."
                    },
                    "path": {
                        "type": "string",
                        "description": "Path on the downstream service, beginning with '/'. NyxID prepends the service's base URL automatically."
                    },
                    "body": {
                        "type": ["object", "string", "null"],
                        "description": "Request body for POST/PUT/PATCH. Object values are JSON-encoded; string values are sent verbatim. Omit for GET/DELETE."
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Additional headers to forward. Authorization / API-key headers are injected by NyxID and MUST NOT be set here."
                    }
                },
                "required": ["service_slug", "method", "path"]
            }
        },
        {
            "name": "nyx_list_services",
            "description": "Enumerate the downstream services this agent can call through NyxID. Returns each service's slug, display name, base URL, auth method, configured rate limits, and (when an OpenAPI spec is available) its callable endpoints. Use this for tool discovery before issuing nyx_proxy_request — agent-key scope determines which services are visible.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_endpoints": {
                        "type": "boolean",
                        "description": "When true, include the parsed OpenAPI endpoint list per service. Adds latency on services with large specs.",
                        "default": false
                    },
                    "category": {
                        "type": "string",
                        "enum": ["llm", "api", "ssh", "node", "internal"],
                        "description": "Optional filter by service category. Omit to list all categories."
                    }
                },
                "required": []
            }
        },
        {
            "name": "nyx_request_approval",
            "description": "Request human approval before performing a sensitive action. NyxID delivers the request to the user via push notification (Telegram or mobile app) and blocks the agent until the user approves, denies, or the request times out. Use this for destructive operations, financial transactions, or any action where unattended automation is inappropriate. Approvals can be granted ad-hoc per call or via pre-configured grant rules.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action_summary": {
                        "type": "string",
                        "description": "One-line summary of what the agent intends to do, written for a human reader (e.g. \"Send $500 wire to vendor X\", \"Delete production database backup older than 30 days\")."
                    },
                    "details": {
                        "type": "string",
                        "description": "Optional longer description with the full context the user needs to decide. Keep under 1000 characters; rendered as plain text in the approval UI."
                    },
                    "service_slug": {
                        "type": "string",
                        "description": "Slug of the service the agent will call after approval. Used to scope approval grants and audit logs."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "How long to wait for a human decision before failing. Bounded by the user's configured maximum.",
                        "minimum": 30,
                        "maximum": 3600,
                        "default": 300
                    }
                },
                "required": ["action_summary", "service_slug"]
            }
        },
        {
            "name": "nyx_exchange_identity",
            "description": "Exchange the agent's NyxID identity for a delegated access token bound to a downstream OIDC service (RFC 8693 token exchange). Lets the agent call APIs on behalf of a specific user without holding that user's long-lived credentials. The returned token carries the original user's identity claims while remaining auditable as an agent action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "audience": {
                        "type": "string",
                        "description": "Target downstream service identifier (resource URI or audience claim) the exchanged token should be valid for."
                    },
                    "subject_user": {
                        "type": "string",
                        "description": "User ID or email of the principal whose identity the agent is acting on behalf of. Must already exist in NyxID and have granted the agent delegation rights."
                    },
                    "scope": {
                        "type": "string",
                        "description": "Space-separated OAuth scopes to request on the exchanged token. Must be a subset of what the subject_user has approved for this agent."
                    },
                    "ttl_secs": {
                        "type": "integer",
                        "description": "Requested token lifetime in seconds. Bounded by the audience's configured maximum (typically 900-3600 seconds).",
                        "minimum": 60,
                        "maximum": 7200,
                        "default": 900
                    }
                },
                "required": ["audience", "subject_user"]
            }
        }
    ])
}
