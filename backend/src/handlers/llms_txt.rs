use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::AppState;

static PLAYBOOK: &str = include_str!("../../../docs/AI_AGENT_PLAYBOOK.md");

fn markdown_response(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        body,
    )
        .into_response()
}

/// GET /llms.txt
///
/// Concise overview of the NyxID platform for AI agents.
/// Returns deployment-specific URLs from AppConfig.
pub async fn llms_txt(State(state): State<AppState>) -> Response {
    let base = state.config.base_url.trim_end_matches('/');
    let frontend = state.config.frontend_url.trim_end_matches('/');

    let body = format!(
        r#"# NyxID

> Auth/SSO and credential management platform. Store API keys, proxy requests with automatic credential injection, expose APIs as MCP tools for AI clients, and run on-premise node agents.

## This Deployment

- API: {base}
- Dashboard: {frontend}
- MCP Endpoint: {base}/mcp
- OIDC Discovery: {base}/.well-known/openid-configuration

## What Users Typically Need Help With

1. **Register a service** -- Add an external API (e.g., OpenAI, Stripe) so credentials can be stored and requests proxied. Dashboard: {frontend}/services or `POST {base}/api/v1/services`.

2. **Connect credentials** -- Store a user's API key / bearer token for a service. Dashboard: {frontend}/connections or `POST {base}/api/v1/connections/{{service_id}}`.

3. **Set up MCP** -- Point Cursor / Claude Code / Codex at `{base}/mcp` to get all connected service endpoints as AI tools.

4. **Set up a provider** -- Register an external OAuth / API key / device-code provider users can link their accounts to. Dashboard: {frontend}/providers/manage or `POST {base}/api/v1/providers`.

5. **Deploy a node agent** -- Run an on-premise agent that keeps credentials local. Generate a registration token at {frontend}/nodes, then `nyxid-node register --token nyx_nreg_...`.

6. **Add login to an app** -- Register an OAuth client at {frontend}/developer/apps, install `@nyxids/oauth-react`, configure with `baseUrl: "{base}"`.

## Key API Endpoints

- `POST /api/v1/auth/login` -- Login (returns access_token + refresh_token)
- `POST /api/v1/services` -- Register a downstream service
- `POST /api/v1/services/{{id}}/endpoints` -- Add API endpoint to a service
- `POST /api/v1/connections/{{service_id}}` -- Store credential for a service
- `ANY  /api/v1/proxy/s/{{slug}}/{{path}}` -- Proxy request with credential injection
- `POST /api/v1/providers` -- Register an external provider
- `POST /api/v1/nodes/register-token` -- Create node registration token
- `POST /api/v1/developer/oauth-clients` -- Register an OAuth client app
- `GET  /oauth/authorize` -- OAuth authorization endpoint
- `POST /oauth/token` -- Token endpoint (auth_code, refresh, client_credentials)

## MCP Client Setup

**Cursor** -- `.cursor/mcp.json`:
```json
{{"mcpServers": {{"nyxid": {{"url": "{base}/mcp"}}}}}}
```

**Claude Code** -- `~/.claude/settings.json`:
```json
{{"mcpServers": {{"nyxid": {{"command": "npx", "args": ["-y", "@anthropic-ai/mcp-proxy", "{base}/mcp"]}}}}}}
```

**Codex** -- `~/.codex/config.toml`:
```toml
[mcp_servers.nyxid]
url = "{base}/mcp"
```

## Full Documentation

For the complete playbook with all API calls, code examples, error codes, and troubleshooting:

{base}/llms-full.txt
"#
    );

    markdown_response(body)
}

/// GET /llms-full.txt
///
/// Full AI Agent Playbook with deployment-specific URLs substituted in.
pub async fn llms_full_txt(State(state): State<AppState>) -> Response {
    let base = state.config.base_url.trim_end_matches('/');
    let frontend = state.config.frontend_url.trim_end_matches('/');

    // Derive ws/wss base for WebSocket URLs in the playbook
    let ws_base = base
        .replace("https://", "wss://")
        .replace("http://", "ws://");

    let body = PLAYBOOK
        .replace("ws://localhost:3001", &ws_base)
        .replace("http://localhost:3001", base)
        .replace("http://localhost:3000", frontend)
        .replace("http://localhost:5173", frontend);

    markdown_response(body)
}
