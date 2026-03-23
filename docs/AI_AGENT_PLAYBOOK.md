# NyxID AI Agent Playbook

This document is a reference for AI agents (Claude, Codex, ChatGPT, Gemini, etc.) to help users configure services, credentials, providers, nodes, and integrations on a running NyxID deployment. It uses placeholder URLs that the server replaces with real values when served via `/llms-full.txt`.

**Audience:** AI coding assistants helping developers use NyxID.

**Server URLs (replaced dynamically when served via /llms-full.txt):**
- Backend API: `http://localhost:3001`
- Frontend Dashboard: `http://localhost:3000`

---

## Table of Contents

1. [What is NyxID](#1-what-is-nyxid)
2. [Key Concepts](#2-key-concepts)
3. [Getting Started](#3-getting-started)
4. [Register a Service (External API)](#4-register-a-service-external-api)
5. [Register a Service (Internal / Shared Credential)](#5-register-a-service-internal--shared-credential)
6. [Register a Service (OIDC / SSO Provider)](#6-register-a-service-oidc--sso-provider)
7. [Connect User Credentials to a Service](#7-connect-user-credentials-to-a-service)
8. [Set Up MCP Proxy for AI Clients](#8-set-up-mcp-proxy-for-ai-clients)
9. [Use the Credential Proxy](#9-use-the-credential-proxy)
10. [Set Up a Provider (OAuth / API Key / Device Code)](#10-set-up-a-provider-oauth--api-key--device-code)
11. [Deploy a Node Agent (On-Premise Credentials)](#11-deploy-a-node-agent-on-premise-credentials)
12. [Add Login to a React App (OAuth Client)](#12-add-login-to-a-react-app-oauth-client)
13. [Add Login to Any Web App (Raw OAuth / OIDC)](#13-add-login-to-any-web-app-raw-oauth--oidc)
14. [Server-to-Server Authentication (Service Accounts)](#14-server-to-server-authentication-service-accounts)
15. [Approval Workflow](#15-approval-workflow)
16. [SSH Services](#16-ssh-services)
17. [API Keys (Programmatic Access)](#17-api-keys-programmatic-access)
18. [LLM Gateway](#18-llm-gateway)
19. [API Quick Reference](#19-api-quick-reference)
20. [Error Code Reference](#20-error-code-reference)
21. [Troubleshooting](#21-troubleshooting)
22. [Common Pitfalls](#22-common-pitfalls)

---

## 1. What is NyxID

NyxID is an Auth/SSO and credential management platform. Users interact with it primarily to:

- **Store and proxy credentials** -- Register external APIs as services, store API keys/tokens, and let NyxID inject them into proxied requests automatically.
- **Expose APIs as MCP tools** -- AI clients (Cursor, Claude Code, Codex) connect to a single MCP endpoint and get tools from all connected services.
- **Run on-premise node agents** -- Keep sensitive credentials on your own infrastructure; NyxID routes requests through nodes without the credentials ever leaving your machines.
- **Act as an OAuth 2.0 / OIDC identity provider** -- Add "Sign in with NyxID" to your apps.
- **Manage providers** -- Connect external OAuth services (Google, GitHub, OpenAI, etc.) so users can link their accounts.

The dashboard is at http://localhost:3000. The API is at http://localhost:3001.

---

## 2. Key Concepts

| Concept | Description |
|---------|-------------|
| **Service** | A downstream app or API registered in NyxID. Has a base URL, auth method, and optional API endpoints. |
| **Provider** | An external OAuth/API service users can connect to (e.g., Google, GitHub, OpenAI). |
| **Connection** | A link between a user and a service, storing the user's credential for that service. |
| **OAuth Client** | An app registered to use NyxID as its identity provider (gets a client_id). |
| **Service Account** | A non-human identity for server-to-server auth (client_credentials grant). |
| **Node** | An on-premise agent that holds credentials locally and proxies requests through NyxID. |
| **MCP Proxy** | Exposes connected service endpoints as MCP tools at `/mcp`. |

### Service Categories

- **External** (`connection`) -- Users bring their own credentials (API key, bearer token, basic auth).
- **Internal** (`internal`) -- Admin configures a shared credential. Users just enable access.
- **Provider** (`provider`) -- OIDC identity provider. Used for federation.

### Auth Methods for Credential Injection

- `header` -- Inject credential as HTTP header (e.g., `Authorization: Bearer ...`)
- `query` -- Inject credential as query parameter
- `body` -- Inject credential into request body
- `oidc` -- Full OIDC client with client_id/client_secret
- `none` -- No credential injection

---

## 3. Getting Started

### What the user needs before starting

- A NyxID account (register at http://localhost:3000/register or via API)
- For admin operations: an admin account
- An API key for AI agent access (see below)

### Set up an API key for AI agent access (recommended)

Create an API key so AI agents can make API calls on the user's behalf. The key should be stored as an environment variable -- **never paste it into AI chat**.

**Step 1:** Create an API key via the dashboard at http://localhost:3000/api-keys, or via API:

```bash
# Login first to get an access token
ACCESS_TOKEN=$(curl -s -X POST http://localhost:3001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "USER_EMAIL", "password": "USER_PASSWORD"}' \
  | jq -r '.access_token')

# Create an API key with read + write scopes
curl -X POST http://localhost:3001/api/v1/api-keys \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "AI Agent Key", "scopes": "read write"}'
# Response includes full_key -- save it immediately (shown only once)
```

**Step 2:** Store the key as an environment variable:

```bash
export NYXID_API_KEY="nyxid_..."
```

Add it to your shell profile (`~/.zshrc`, `~/.bashrc`) to persist across sessions.

**Step 3:** Use the key in API calls (via `X-API-Key` header or `Authorization: Bearer`):

```bash
curl http://localhost:3001/api/v1/users/me \
  -H "X-API-Key: $NYXID_API_KEY"

# Or as Bearer token:
curl http://localhost:3001/api/v1/users/me \
  -H "Authorization: Bearer $NYXID_API_KEY"
```

**IMPORTANT for AI agents:** The user's API key is in `$NYXID_API_KEY`. Always use the environment variable in commands. Never ask the user to paste the key into chat. Never output the key value.

### Alternative: Login with email/password

For short-lived sessions without an API key:

```bash
ACCESS_TOKEN=$(curl -s -X POST http://localhost:3001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "USER_EMAIL", "password": "USER_PASSWORD"}' \
  | jq -r '.access_token')
```

Access tokens expire in 15 minutes. Refresh:

```bash
curl -X POST http://localhost:3001/api/v1/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "REFRESH_TOKEN"}'
```

### A note on `$ACCESS_TOKEN` in examples

The API examples throughout this document use `$ACCESS_TOKEN`. If the user has set up an API key (recommended), replace with:

```bash
-H "X-API-Key: $NYXID_API_KEY"
# instead of:
-H "Authorization: Bearer $ACCESS_TOKEN"
```

### Or use the dashboard

Most operations can also be done in the NyxID dashboard at http://localhost:3000. The dashboard is the easiest way to get started -- the API examples below are for when the user wants to automate or script things.

---

## 4. Register a Service (External API)

**Goal:** Register an external API so users can store their own credentials and proxy requests through NyxID.

### Via Dashboard

1. Go to http://localhost:3000/services
2. Click "New Service"
3. Fill in: name, base URL, auth type (e.g., API Key / Bearer / Basic), auth key name (e.g., `Authorization`)
4. Set category to "External" (users bring their own credentials)
5. Save

### Via API

```bash
curl -X POST http://localhost:3001/api/v1/services \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "OpenAI API",
    "slug": "openai",
    "base_url": "https://api.openai.com",
    "auth_method": "header",
    "auth_key_name": "Authorization",
    "service_category": "connection",
    "visibility": "public"
  }'
```

Save the returned `id` -- this is the service ID.

### Add API Endpoints (for MCP tools)

Endpoints define which API paths are exposed as MCP tools. You can auto-discover them from an OpenAPI spec or add them manually.

**Auto-discover from OpenAPI:**

```bash
# First set the spec URL on the service
curl -X PUT http://localhost:3001/api/v1/services/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"openapi_spec_url": "https://api.example.com/openapi.json"}'

# Then trigger discovery (no request body)
curl -X POST http://localhost:3001/api/v1/services/$SERVICE_ID/discover-endpoints \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**Add manually:**

```bash
curl -X POST http://localhost:3001/api/v1/services/$SERVICE_ID/endpoints \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "create_chat_completion",
    "method": "POST",
    "path": "/v1/chat/completions",
    "description": "Create a chat completion"
  }'
```

---

## 5. Register a Service (Internal / Shared Credential)

**Goal:** Register a service where the admin provides a shared credential. Users just enable access without managing keys.

### Via API

```bash
curl -X POST http://localhost:3001/api/v1/services \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Internal Analytics API",
    "slug": "analytics",
    "base_url": "https://analytics.internal.example.com",
    "auth_method": "header",
    "auth_key_name": "X-API-Key",
    "service_category": "internal",
    "credential": "the-shared-api-key-here",
    "visibility": "public"
  }'
```

Users connect with an empty body (no credential needed):

```bash
curl -X POST http://localhost:3001/api/v1/connections/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

---

## 6. Register a Service (OIDC / SSO Provider)

**Goal:** Register a service where NyxID acts as the OIDC identity provider. Users sign in via NyxID's OAuth flow and the downstream app gets a client_id/client_secret pair.

### Via Dashboard

1. Go to http://localhost:3000/services
2. Click "New Service"
3. Set auth type to **OIDC**
4. NyxID auto-generates a client_id and client_secret for the service
5. The service category is automatically set to "provider"

### Via API

```bash
curl -X POST http://localhost:3001/api/v1/services \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Customer Portal",
    "slug": "customer-portal",
    "base_url": "https://portal.example.com",
    "auth_method": "oidc",
    "visibility": "public"
  }'
```

When `auth_method` is `oidc`, NyxID automatically:
- Creates an OAuth client with a generated `client_id` and `client_secret`
- Sets the default redirect URI to `{base_url}/callback`
- Sets `service_category` to `provider`

### Retrieve OIDC credentials

```bash
curl http://localhost:3001/api/v1/services/$SERVICE_ID/oidc-credentials \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

Response:

```json
{
  "client_id": "generated-uuid",
  "client_secret": "generated-secret",
  "redirect_uris": ["https://portal.example.com/callback"],
  "allowed_scopes": "openid profile email",
  "delegation_scopes": "",
  "issuer": "http://localhost:3001",
  "authorization_endpoint": "http://localhost:3001/oauth/authorize",
  "token_endpoint": "http://localhost:3001/oauth/token",
  "userinfo_endpoint": "http://localhost:3001/oauth/userinfo",
  "jwks_uri": "http://localhost:3001/.well-known/jwks.json"
}
```

Give these values to the downstream app to configure its OIDC client.

### Update redirect URIs

```bash
curl -X PUT http://localhost:3001/api/v1/services/$SERVICE_ID/redirect-uris \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "redirect_uris": [
      "https://portal.example.com/callback",
      "https://portal.example.com/auth/callback"
    ]
  }'
```

### Regenerate client secret

```bash
curl -X POST http://localhost:3001/api/v1/services/$SERVICE_ID/regenerate-secret \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

Returns the new `client_secret` (one-time display).

---

## 7. Connect to a Service

There are two ways to connect to a service depending on how it's set up:

1. **Direct credential** -- Enter an API key, bearer token, or basic auth credential directly.
2. **Via a provider** -- Connect through an OAuth flow, device code flow, or API key provider that's been registered in NyxID (see section 10).

### Option A: Direct credential

For services where you have an API key or token.

**Via Dashboard:**

1. Go to http://localhost:3000/connections
2. Find the service and click "Connect"
3. Enter your credential (API key, bearer token, etc.)

**Via API:**

```bash
curl -X POST http://localhost:3001/api/v1/connections/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "credential": "Bearer sk-proj-your-api-key-here",
    "credential_label": "My Production Key"
  }'
```

### Option B: Via a provider (OAuth / Device Code)

For services that use a provider for authentication (e.g., OpenAI via Codex device code flow, GitHub via OAuth).

**Via Dashboard:**

1. Go to http://localhost:3000/providers
2. Find the provider and click "Connect"
3. Follow the OAuth flow, enter a device code, or paste an API key depending on the provider type

**Via API:**

```bash
# OAuth provider -- initiates browser redirect
GET /api/v1/providers/{provider_id}/connect/oauth
# Returns: { "authorization_url": "https://..." }

# Device code provider (e.g., Codex) -- no browser needed
POST /api/v1/providers/{provider_id}/connect/device-code/initiate
# Returns: { "user_code": "ABCD-1234", "verification_uri": "https://...", "state": "..." }
# Show user_code to the user, then poll:
POST /api/v1/providers/{provider_id}/connect/device-code/poll
{"state": "STATE_FROM_INITIATE"}

# API key provider
POST /api/v1/providers/{provider_id}/connect/api-key
{"api_key": "sk-...", "label": "My Key"}
```

### Update or disconnect

```bash
# Update a direct credential
curl -X PUT http://localhost:3001/api/v1/connections/$SERVICE_ID/credential \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"credential": "Bearer sk-proj-new-key", "credential_label": "Rotated Key"}'

# Disconnect from a service
curl -X DELETE http://localhost:3001/api/v1/connections/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN"

# Disconnect from a provider
curl -X DELETE http://localhost:3001/api/v1/providers/$PROVIDER_ID/disconnect \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

---

## 8. Set Up MCP Proxy for AI Clients

**Goal:** Let Cursor, Claude Code, or Codex call APIs through NyxID's MCP proxy with automatic credential injection.

### Prerequisites

1. At least one service with endpoints registered (see section 4)
2. User has connected their credential to that service (see section 6)

### Cursor

Create or edit `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "nyxid": {
      "url": "http://localhost:3001/mcp"
    }
  }
}
```

Restart Cursor. Authenticate via browser when prompted.

### Claude Code

Edit `~/.claude/settings.json` or project-level `.claude/settings.json`:

```json
{
  "mcpServers": {
    "nyxid": {
      "command": "npx",
      "args": ["-y", "@anthropic-ai/mcp-proxy", "http://localhost:3001/mcp"],
      "description": "NyxID MCP Proxy"
    }
  }
}
```

Restart Claude Code.

### Codex

Edit `~/.codex/config.toml`:

```toml
[mcp_servers.nyxid]
url = "http://localhost:3001/mcp"
```

Restart Codex.

### How it works

1. MCP client connects and authenticates via OAuth in the browser
2. NyxID aggregates API endpoints from all connected services into MCP tools
3. Tools are named `{service-slug}__{endpoint-name}`
4. When a tool is called, NyxID injects the user's stored credential
5. Request is proxied to the downstream service, response returned to the MCP client

Built-in meta-tools let you search for tools and discover/connect to new services from within the MCP client.

---

## 9. Use the Credential Proxy

**Goal:** Proxy API requests through NyxID with automatic credential injection (without MCP).

### Proxy by slug (recommended)

```bash
curl http://localhost:3001/api/v1/proxy/s/openai/v1/chat/completions \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]}'
```

### Proxy by service ID

```bash
curl http://localhost:3001/api/v1/proxy/$SERVICE_ID/v1/chat/completions \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]}'
```

NyxID automatically injects the user's stored credential (e.g., `Authorization: Bearer sk-proj-...`) into the outgoing request.

### List proxyable services

```bash
curl http://localhost:3001/api/v1/proxy/services \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

### Identity Propagation (optional)

Services can be configured to forward the NyxID user's identity to the downstream API:

- `X-User-ID` -- NyxID user ID
- `X-User-Email` -- User's email
- `X-User-Name` -- User's display name
- `X-NyxID-Authenticated` -- Always `true`

---

## 10. Set Up a Provider (OAuth / API Key / Device Code)

**Goal:** Register an external provider that users can connect their accounts to.

### OAuth 2.0 Provider (admin credentials)

Admin provides a shared OAuth app. Users just authorize via the OAuth flow.

```bash
curl -X POST http://localhost:3001/api/v1/providers \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "GitHub",
    "slug": "github",
    "provider_type": "oauth2",
    "credential_mode": "admin",
    "authorization_url": "https://github.com/login/oauth/authorize",
    "token_url": "https://github.com/login/oauth/access_token",
    "default_scopes": ["repo", "read:user"],
    "client_id": "YOUR_GITHUB_CLIENT_ID",
    "client_secret": "YOUR_GITHUB_CLIENT_SECRET",
    "supports_pkce": false
  }'
```

Users connect via: `GET /api/v1/providers/{provider_id}/connect/oauth`

### OAuth 2.0 Provider (user credentials)

Each user provides their own developer OAuth app credentials (e.g., X/Twitter where each developer creates their own app, or Codex CLI auth).

```bash
curl -X POST http://localhost:3001/api/v1/providers \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "X (Twitter)",
    "slug": "x-twitter",
    "provider_type": "oauth2",
    "credential_mode": "user",
    "authorization_url": "https://twitter.com/i/oauth2/authorize",
    "token_url": "https://api.twitter.com/2/oauth2/token",
    "default_scopes": ["tweet.read", "users.read"],
    "supports_pkce": true
  }'
```

Users first set their own OAuth credentials, then authorize:

```bash
# Step 1: User provides their own developer app client_id/client_secret
curl -X PUT http://localhost:3001/api/v1/providers/$PROVIDER_ID/credentials \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "client_id": "users-own-client-id",
    "client_secret": "users-own-client-secret",
    "label": "My X Developer App"
  }'

# Step 2: User initiates OAuth flow (uses their own credentials)
# GET /api/v1/providers/{provider_id}/connect/oauth
# Returns: { "authorization_url": "https://twitter.com/i/oauth2/authorize?..." }
```

### OAuth 2.0 Provider (both modes)

Admin provides default credentials but users can override with their own:

```bash
curl -X POST http://localhost:3001/api/v1/providers \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Codex",
    "slug": "codex",
    "provider_type": "oauth2",
    "credential_mode": "both",
    "authorization_url": "https://auth.openai.com/authorize",
    "token_url": "https://auth.openai.com/oauth/token",
    "default_scopes": ["openid", "profile"],
    "client_id": "default-client-id",
    "client_secret": "default-client-secret",
    "supports_pkce": true
  }'
```

Users who want the default just call `GET /api/v1/providers/{id}/connect/oauth`. Users who want their own credentials set them first via `PUT /api/v1/providers/{id}/credentials`.

### User credential management

```bash
# Get user's credentials for a provider
GET /api/v1/providers/{provider_id}/credentials

# Set user's own OAuth credentials
PUT /api/v1/providers/{provider_id}/credentials
{"client_id": "...", "client_secret": "...", "label": "My App"}

# Delete user's credentials (fall back to admin credentials if mode is "both")
DELETE /api/v1/providers/{provider_id}/credentials
```

### API Key Provider

```bash
curl -X POST http://localhost:3001/api/v1/providers \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Anthropic",
    "slug": "anthropic",
    "provider_type": "api_key",
    "credential_mode": "user",
    "api_key_instructions": "Get your API key from https://console.anthropic.com/settings/keys",
    "api_key_url": "https://console.anthropic.com/settings/keys"
  }'
```

Users connect via:

```bash
POST /api/v1/providers/{provider_id}/connect/api-key
{"api_key": "sk-ant-api03-...", "label": "My Anthropic Key"}
```

### Device Code Provider (for CLI tools)

```bash
curl -X POST http://localhost:3001/api/v1/providers \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "OpenAI Codex",
    "slug": "openai-codex",
    "provider_type": "device_code",
    "device_code_url": "https://auth.openai.com/api/accounts/deviceauth/usercode",
    "device_token_url": "https://auth.openai.com/api/accounts/deviceauth/token",
    "authorization_url": "https://auth.openai.com/authorize",
    "token_url": "https://auth.openai.com/oauth/token"
  }'
```

Users connect via:

```bash
# Initiate
POST /api/v1/providers/{provider_id}/connect/device-code/initiate
# Returns: { user_code, verification_uri, state, expires_in, interval }

# Poll (repeat until status != "pending")
POST /api/v1/providers/{provider_id}/connect/device-code/poll
{"state": "STATE_FROM_INITIATE"}
# Returns: { status: "pending" | "success" | "expired" | "denied" }
```

### Credential Modes

- `admin` -- Admin provides client credentials. Users just authorize via OAuth.
- `user` -- Users bring their own client_id/client_secret.
- `both` -- Admin provides defaults; users can override with their own.

### Via Dashboard

Go to http://localhost:3000/providers/manage to create and manage providers.

---

## 11. Deploy a Node Agent (On-Premise Credentials)

**Goal:** Keep sensitive credentials on your own infrastructure. NyxID routes proxy requests through the node agent, which injects credentials locally -- they never leave your machine.

### How it works

1. The node agent runs on your infrastructure and connects to NyxID via WebSocket
2. You store API credentials locally on the node (encrypted at rest)
3. When a user makes a proxy request for a node-bound service, NyxID routes it through the node
4. The node injects the credential and forwards the request to the downstream API
5. The credential never transits the NyxID server

### Step 1: Install the node agent

The node agent is a single Rust binary. Build from source:

```bash
# Requires Rust toolchain (https://rustup.rs)
# Clone the NyxID repo, then:
cargo build --release -p nyxid-node

# Binary is at: target/release/nyxid-node
# Copy it to the target machine or add to PATH:
cp target/release/nyxid-node /usr/local/bin/
```

Or install directly:

```bash
cargo install --path node-agent
```

Verify:

```bash
nyxid-node version
```

### Step 2: Generate a registration token

Via dashboard: http://localhost:3000/nodes, or via API:

```bash
curl -X POST http://localhost:3001/api/v1/nodes/register-token \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Production Node"}'
```

Returns: `{ "token": "nyx_nreg_...", "token_id": "...", "expires_at": "..." }`

Registration tokens expire after 1 hour by default.

### Step 3: Register the node

On the target machine where the agent will run:

```bash
nyxid-node register \
  --token "nyx_nreg_..." \
  --url "wss://localhost:3001/api/v1/nodes/ws"
```

This creates a config file at `~/.nyxid-node/config.toml` and an encryption keyfile at `~/.nyxid-node/.keyfile`.

**Options:**
- `--config <PATH>` -- Custom config directory (default: `~/.nyxid-node`)
- `--keychain` -- Use OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service) instead of file-based encryption

### Step 4: Add credentials

Add credentials for each service the node will handle. The secret value is prompted securely (not visible in shell history):

```bash
# Header injection (most common -- e.g., Authorization header)
nyxid-node credentials add --service "openai" --header "Authorization"
# Prompts for: Enter secret value: (hidden input)

# With automatic Bearer prefix
nyxid-node credentials add --service "openai" --header "Authorization" --secret-format Bearer
# Prompts for the API key, then stores as "Bearer <key>"

# Query parameter injection
nyxid-node credentials add --service "weather-api" --query-param "api_key"
# Prompts for the key value
```

**Secret formats:**
- `Raw` (default) -- Store the value exactly as entered
- `Bearer` -- Automatically prepend `Bearer ` to the value
- `Basic` -- Base64-encode as `username:password` and prepend `Basic `

**Manage credentials:**

```bash
nyxid-node credentials list                      # List all configured credentials
nyxid-node credentials remove --service "openai"  # Remove a credential
```

### Step 5: Bind services to the node

In the dashboard (http://localhost:3000/nodes/{nodeId}), or via API:

```bash
# Get the node ID (shown during registration, or from the dashboard)
curl -X POST http://localhost:3001/api/v1/nodes/$NODE_ID/bindings \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"service_id": "SERVICE_UUID"}'
```

Only bound services will be routed through this node. Unbound services use NyxID's direct proxy.

### Step 6: Start the agent

```bash
nyxid-node start
```

The agent connects via WebSocket and automatically reconnects with exponential backoff (100ms to 60s) if the connection drops. Run it as a systemd service or supervisor process for production.

**Example systemd unit:**

```ini
[Unit]
Description=NyxID Node Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/nyxid-node start
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Configuration file reference

The config is stored at `~/.nyxid-node/config.toml` (created during registration):

```toml
storage_backend = "file"  # "file" (AES-256-GCM) or "keychain" (OS keychain)

[server]
url = "wss://localhost:3001/api/v1/nodes/ws"

[node]
id = "node-uuid-here"
auth_token_encrypted = "..."  # Encrypted auth token (file backend only)

[signing]
shared_secret_encrypted = "..."  # HMAC signing secret (file backend only)

[ssh]
max_tunnels = 10           # Max concurrent SSH tunnels (default: 10)
io_timeout_secs = 3600     # Idle timeout per tunnel in seconds (default: 3600)
# Restrict which SSH hosts the node can connect to:
# allowed_targets = [
#   { host = "internal.example.com", port = 22 },
#   { host = "db.example.com", port = 22 },
# ]

# Credentials are stored per-service:
[credentials.openai]
injection_method = "header"
header_name = "Authorization"
header_value_encrypted = "..."  # AES-256-GCM encrypted (file backend)

[credentials.weather-api]
injection_method = "query_param"
param_name = "api_key"
param_value_encrypted = "..."
```

### Secret storage backends

**File backend (default):**
- Secrets encrypted with AES-256-GCM
- Encryption key stored at `~/.nyxid-node/.keyfile` (mode `0600`)
- Works on all platforms, no daemon required

**Keychain backend:**
- macOS: Keychain
- Windows: Credential Manager
- Linux: Secret Service D-Bus (GNOME Keyring / KDE Wallet)
- Register with `--keychain` flag, or migrate later

**Migrate between backends:**

```bash
nyxid-node migrate --to keychain   # File -> OS keychain
nyxid-node migrate --to file       # OS keychain -> file
```

### All CLI commands

```bash
nyxid-node register   --token <TOKEN> [--url <WS_URL>] [--config <PATH>] [--keychain]
nyxid-node start      [--config <PATH>] [--log-level <LEVEL>]
nyxid-node status     [--config <PATH>]
nyxid-node rekey      --auth-token <TOKEN> --signing-secret <HEX> [--config <PATH>]
nyxid-node credentials add    --service <SLUG> [--header <NAME> | --query-param <NAME>] [--secret-format Raw|Bearer|Basic]
nyxid-node credentials list   [--config <PATH>]
nyxid-node credentials remove --service <SLUG> [--config <PATH>]
nyxid-node migrate    --to <file|keychain> [--config <PATH>]
nyxid-node version
```

Global option: `--log-level <trace|debug|info|warn|error>` (default: `info`)

---

## 12. Add Login to a React App (OAuth Client)

**Goal:** Add "Sign in with NyxID" to a React app using the official SDK.

### Step 1: Register an OAuth client

Via dashboard at http://localhost:3000/developer/apps, or via API:

```bash
curl -X POST http://localhost:3001/api/v1/developer/oauth-clients \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My React App",
    "redirect_uris": ["https://myapp.example.com/auth/callback"],
    "client_type": "public",
    "allowed_scopes": ["openid", "profile", "email"]
  }'
```

**Save the returned `id`** -- this is your `client_id`.

### Step 2: Install the SDK

```bash
npm install @nyxids/oauth-core @nyxids/oauth-react
```

### Step 3: Configure the provider

```tsx
// src/main.tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { NyxIDProvider, createNyxClient } from "@nyxids/oauth-react";
import App from "./App";

const nyxClient = createNyxClient({
  baseUrl: "http://localhost:3001",
  clientId: "YOUR_CLIENT_ID",
  redirectUri: `${window.location.origin}/auth/callback`,
  scope: "openid profile email",
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <NyxIDProvider client={nyxClient}>
      <App />
    </NyxIDProvider>
  </StrictMode>,
);
```

### Step 4: Add login button and callback handler

```tsx
// src/App.tsx
import { useEffect, useState } from "react";
import { useNyxID } from "@nyxids/oauth-react";
import type { OAuthUserInfo } from "@nyxids/oauth-core";

export default function App() {
  const {
    isAuthenticated,
    tokens,
    loginWithRedirect,
    handleRedirectCallback,
    getUserInfo,
    clearSession,
  } = useNyxID();

  const [user, setUser] = useState<OAuthUserInfo | null>(null);

  useEffect(() => {
    if (window.location.pathname === "/auth/callback") {
      handleRedirectCallback()
        .then(() => window.history.replaceState({}, "", "/"))
        .catch(() => { /* handle error */ });
    }
  }, [handleRedirectCallback]);

  useEffect(() => {
    if (isAuthenticated && tokens?.accessToken) {
      getUserInfo(tokens.accessToken)
        .then(setUser)
        .catch(() => { /* handle error */ });
    }
  }, [isAuthenticated, tokens, getUserInfo]);

  if (!isAuthenticated) {
    return <button onClick={() => loginWithRedirect()}>Sign in with NyxID</button>;
  }

  return (
    <div>
      <h1>Welcome, {user?.name ?? user?.email ?? "User"}</h1>
      <button onClick={clearSession}>Sign out</button>
    </div>
  );
}
```

### SDK API Reference

```typescript
interface NyxIDClientConfig {
  baseUrl: string;           // NyxID API URL (e.g., "http://localhost:3001")
  clientId: string;          // OAuth client ID from step 1
  redirectUri: string;       // Must match a registered redirect_uri
  scope?: string;            // Default: "openid profile email"
  storage?: StorageLike;     // Default: localStorage
  fetchFn?: typeof fetch;    // Custom fetch (for testing/SSR)
}

// useNyxID() hook returns:
interface NyxIDContextValue {
  client: NyxIDClient;
  tokens: NyxIDTokenSet | null;
  isAuthenticated: boolean;
  loginWithRedirect(options?: LoginRedirectOptions): Promise<void>;
  handleRedirectCallback(url?: string): Promise<NyxIDTokenSet>;
  clearSession(): void;
  getUserInfo(accessToken?: string): Promise<OAuthUserInfo>;
}

interface NyxIDTokenSet {
  accessToken: string;       // Bearer token (15 min TTL)
  tokenType: string;         // "Bearer"
  expiresIn: number;         // Seconds until expiration
  refreshToken?: string;
  idToken?: string;
  scope?: string;
}

interface OAuthUserInfo {
  sub: string;
  email?: string;
  email_verified?: boolean;
  name?: string;
  picture?: string;
  roles?: string[];
  groups?: string[];
  permissions?: string[];
}
```

---

## 13. Add Login to Any Web App (Raw OAuth / OIDC)

**Goal:** Integrate NyxID login without the SDK. Works with any language/framework.

### Using OIDC Discovery (recommended)

Point your OIDC library at:

```
http://localhost:3001/.well-known/openid-configuration
```

#### NextAuth.js

```typescript
import NextAuth from "next-auth";

export const { handlers, auth } = NextAuth({
  providers: [{
    id: "nyxid",
    name: "NyxID",
    type: "oidc",
    issuer: "http://localhost:3001",
    clientId: "YOUR_CLIENT_ID",
    clientSecret: "YOUR_CLIENT_SECRET",
  }],
});
```

#### Passport.js

```javascript
const { Issuer, Strategy } = require("openid-client");
const issuer = await Issuer.discover("http://localhost:3001");
const client = new issuer.Client({
  client_id: "YOUR_CLIENT_ID",
  redirect_uris: ["https://myapp.example.com/callback"],
  response_types: ["code"],
});
passport.use("nyxid", new Strategy({ client }, (tokenSet, userinfo, done) => {
  done(null, userinfo);
}));
```

### Manual OAuth 2.0 + PKCE

#### Step 1: Generate PKCE challenge

```python
import hashlib, base64, os

code_verifier = base64.urlsafe_b64encode(os.urandom(48)).rstrip(b"=").decode()
code_challenge = base64.urlsafe_b64encode(
    hashlib.sha256(code_verifier.encode()).digest()
).rstrip(b"=").decode()
state = base64.urlsafe_b64encode(os.urandom(24)).rstrip(b"=").decode()
```

#### Step 2: Redirect user to authorize

```
GET http://localhost:3001/oauth/authorize?
  response_type=code&
  client_id=YOUR_CLIENT_ID&
  redirect_uri=https://myapp.example.com/callback&
  scope=openid profile email&
  code_challenge=<code_challenge>&
  code_challenge_method=S256&
  state=<state>
```

#### Step 3: Exchange code for tokens

```bash
curl -X POST http://localhost:3001/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code" \
  -d "code=AUTHORIZATION_CODE" \
  -d "redirect_uri=https://myapp.example.com/callback" \
  -d "client_id=YOUR_CLIENT_ID" \
  -d "code_verifier=CODE_VERIFIER"
```

#### Step 4: Get user info

```bash
curl http://localhost:3001/oauth/userinfo \
  -H "Authorization: Bearer ACCESS_TOKEN"
```

#### Step 5: Refresh tokens

```bash
curl -X POST http://localhost:3001/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=refresh_token" \
  -d "refresh_token=REFRESH_TOKEN" \
  -d "client_id=YOUR_CLIENT_ID"
```

#### Step 6: Verify JWTs (optional)

```bash
curl http://localhost:3001/.well-known/jwks.json
```

Use any JWT library to verify tokens with the RS256 algorithm and these keys.

---

## 14. Server-to-Server Authentication (Service Accounts)

**Goal:** Authenticate a backend service (no browser) using client credentials.

### Step 1: Create a service account (admin only)

```bash
curl -X POST http://localhost:3001/api/v1/admin/service-accounts \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "My Backend Service", "description": "Automated data pipeline"}'
```

Response includes a one-time `secret` -- **save it immediately**.

### Step 2: Get an access token

```bash
curl -X POST http://localhost:3001/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=SERVICE_ACCOUNT_ID" \
  -d "client_secret=SERVICE_ACCOUNT_SECRET"
```

### Step 3: Use the token

```bash
curl http://localhost:3001/api/v1/some-endpoint \
  -H "Authorization: Bearer ACCESS_TOKEN"
```

Token TTL defaults to 1 hour.

---

## 15. Approval Workflow

**Goal:** Require approval before users can access sensitive services. Approvals can be delivered via Telegram or mobile push notifications.

### Configure a service to require approval

```bash
curl -X PUT http://localhost:3001/api/v1/approvals/service-configs/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"enabled": true}'
```

When a user tries to proxy a request to this service, they get a 403 with `error_code: 7000` and a `request_id`.

### Poll for approval status

```bash
curl http://localhost:3001/api/v1/approvals/requests/$REQUEST_ID/status \
  -H "Authorization: Bearer $ACCESS_TOKEN"
# Returns: { "status": "pending" | "approved" | "denied", "expires_at": "..." }
```

### Approve or deny (admin / approver)

```bash
curl -X POST http://localhost:3001/api/v1/approvals/requests/$REQUEST_ID/decide \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"approved": true}'
```

### View approval history and grants

```bash
# List all approval requests
GET /api/v1/approvals/requests

# List active grants
GET /api/v1/approvals/grants

# Revoke a grant
DELETE /api/v1/approvals/grants/{grant_id}
```

### Set up Telegram notifications

```bash
# Link Telegram account (returns a link code to send to the NyxID bot)
POST /api/v1/notifications/telegram/link

# Configure notification preferences
PUT /api/v1/notifications/settings
{"approval_requests": true, "approval_grants": true}

# Disconnect Telegram
DELETE /api/v1/notifications/telegram
```

### Via Dashboard

- Approval history: http://localhost:3000/approvals/history
- Active grants: http://localhost:3000/approvals/grants
- Notification settings: http://localhost:3000/approvals/settings

---

## 16. SSH Services

**Goal:** Register an SSH service for certificate-based authentication, remote command execution, or interactive terminal sessions.

### Register an SSH service

```bash
curl -X POST http://localhost:3001/api/v1/services \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Production Server",
    "slug": "prod-server",
    "base_url": "ssh://10.0.0.5",
    "service_type": "ssh",
    "visibility": "private",
    "ssh_config": {
      "host": "10.0.0.5",
      "port": 22,
      "certificate_auth_enabled": true,
      "certificate_ttl_minutes": 30,
      "allowed_principals": ["ubuntu", "deploy"]
    }
  }'
```

SSH services allow private IPs and localhost (they're admin-configured infrastructure, not user-supplied URLs).

### Issue an SSH certificate

```bash
curl -X POST http://localhost:3001/api/v1/ssh/$SERVICE_ID/certificate \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"public_key": "ssh-ed25519 AAAA..."}'
# Returns: { "certificate": "ssh-ed25519-cert-v01@openssh.com ...", "validity_period": "30m" }
```

### Execute a remote command

```bash
curl -X POST http://localhost:3001/api/v1/ssh/$SERVICE_ID/exec \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"command": "uptime"}'
# Returns: { "stdout": "...", "stderr": "...", "exit_code": 0 }
```

### Interactive terminal (WebSocket)

```
GET /api/v1/ssh/{service_id}/terminal  (WebSocket upgrade)
```

### SSH tunnel (WebSocket)

```
GET /api/v1/ssh/{service_id}  (WebSocket upgrade, bidirectional SSH protocol)
```

---

## 17. API Keys (Programmatic Access)

**Goal:** Create API keys for CLI or programmatic access without going through the OAuth flow.

```bash
# Create an API key
curl -X POST http://localhost:3001/api/v1/api-keys \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "CI Pipeline Key"}'
# Returns the key value (one-time display)

# List API keys
GET /api/v1/api-keys

# Rotate a key
POST /api/v1/api-keys/{key_id}/rotate

# Delete a key
DELETE /api/v1/api-keys/{key_id}
```

Use API keys as Bearer tokens: `Authorization: Bearer nyxid_key_...`

---

## 18. LLM Gateway

**Goal:** Proxy LLM API calls through NyxID with automatic credential injection. Provides an OpenAI-compatible API interface.

### Check available LLM providers

```bash
curl http://localhost:3001/api/v1/llm/status \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

### Make LLM requests (OpenAI-compatible)

```bash
# Route through the unified gateway
curl http://localhost:3001/api/v1/llm/gateway/v1/chat/completions \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]}'

# Or route to a specific provider by slug
curl http://localhost:3001/api/v1/openai/v1/chat/completions \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]}'
```

NyxID injects the user's stored provider token automatically.

---

## 19. API Quick Reference

Base URL: `http://localhost:3001`

### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/auth/register` | Register new user |
| POST | `/api/v1/auth/login` | Login (returns access_token + refresh_token) |
| POST | `/api/v1/auth/logout` | Logout |
| POST | `/api/v1/auth/refresh` | Refresh access token |
| POST | `/api/v1/auth/forgot-password` | Request password reset email |
| POST | `/api/v1/auth/reset-password` | Complete password reset |

### User

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/users/me` | Get current user profile |
| PUT | `/api/v1/users/me` | Update profile |
| GET | `/api/v1/sessions` | List active sessions |
| GET | `/api/v1/api-keys` | List API keys |
| POST | `/api/v1/api-keys` | Create API key |

### Services

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/services` | List services |
| POST | `/api/v1/services` | Create service |
| GET | `/api/v1/services/{id}` | Get service details |
| PUT | `/api/v1/services/{id}` | Update service |
| DELETE | `/api/v1/services/{id}` | Delete service |
| POST | `/api/v1/services/{id}/endpoints` | Add API endpoint |
| POST | `/api/v1/services/{id}/discover-endpoints` | Auto-discover from OpenAPI |
| GET | `/api/v1/services/{id}/oidc-credentials` | Get OIDC client credentials |
| PUT | `/api/v1/services/{id}/redirect-uris` | Update OIDC redirect URIs |
| POST | `/api/v1/services/{id}/regenerate-secret` | Regenerate OIDC client secret |

### Connections

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/connections` | List user's connections |
| POST | `/api/v1/connections/{service_id}` | Connect to service |
| DELETE | `/api/v1/connections/{service_id}` | Disconnect from service |
| PUT | `/api/v1/connections/{service_id}/credential` | Update credential |

### Proxy

| Method | Endpoint | Description |
|--------|----------|-------------|
| ANY | `/api/v1/proxy/{service_id}/{path}` | Proxy by UUID |
| ANY | `/api/v1/proxy/s/{slug}/{path}` | Proxy by slug |
| GET | `/api/v1/proxy/services` | List proxyable services |

### Providers

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/providers` | List providers |
| POST | `/api/v1/providers` | Create provider |
| GET | `/api/v1/providers/{id}` | Get provider |
| PUT | `/api/v1/providers/{id}` | Update provider |
| DELETE | `/api/v1/providers/{id}` | Delete provider |
| GET | `/api/v1/providers/{id}/connect/oauth` | Initiate OAuth connect |
| POST | `/api/v1/providers/{id}/connect/api-key` | Connect with API key |
| POST | `/api/v1/providers/{id}/connect/device-code/initiate` | Start device code flow |
| POST | `/api/v1/providers/{id}/connect/device-code/poll` | Poll device code status |
| POST | `/api/v1/providers/{id}/refresh` | Refresh provider token |
| DELETE | `/api/v1/providers/{id}/disconnect` | Disconnect provider |
| GET | `/api/v1/providers/{id}/credentials` | Get user's own OAuth credentials |
| PUT | `/api/v1/providers/{id}/credentials` | Set user's own OAuth credentials |
| DELETE | `/api/v1/providers/{id}/credentials` | Delete user's own credentials |

### Nodes

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/nodes/register-token` | Create registration token |
| GET | `/api/v1/nodes` | List user's nodes |
| GET | `/api/v1/nodes/{id}` | Get node details |
| DELETE | `/api/v1/nodes/{id}` | Delete node |
| POST | `/api/v1/nodes/{id}/rotate-token` | Rotate node auth token |
| POST | `/api/v1/nodes/{id}/bindings` | Bind service to node |
| DELETE | `/api/v1/nodes/{id}/bindings/{binding_id}` | Remove binding |

### Developer Apps (OAuth Clients)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/developer/oauth-clients` | List your OAuth clients |
| POST | `/api/v1/developer/oauth-clients` | Create OAuth client |
| GET | `/api/v1/developer/oauth-clients/{id}` | Get client details |
| PATCH | `/api/v1/developer/oauth-clients/{id}` | Update client |
| DELETE | `/api/v1/developer/oauth-clients/{id}` | Delete client |
| POST | `/api/v1/developer/oauth-clients/{id}/rotate-secret` | Rotate secret |

### OAuth / OIDC

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/.well-known/openid-configuration` | OIDC discovery |
| GET | `/.well-known/jwks.json` | Public signing keys |
| GET | `/oauth/authorize` | Authorization endpoint |
| POST | `/oauth/token` | Token endpoint |
| GET/POST | `/oauth/userinfo` | Get authenticated user info |
| POST | `/oauth/introspect` | Token introspection |
| POST | `/oauth/revoke` | Token revocation |

### Admin

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/admin/users` | List all users |
| POST | `/api/v1/admin/service-accounts` | Create service account |
| GET | `/api/v1/admin/audit-log` | View audit log |

### MFA

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/auth/mfa/setup` | Start MFA setup (TOTP) |
| POST | `/api/v1/auth/mfa/confirm` | Confirm MFA setup |
| POST | `/api/v1/auth/mfa/verify` | Verify MFA code |
| POST | `/api/v1/auth/mfa/disable` | Disable MFA |

### Approvals

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/approvals/requests` | List approval requests |
| GET | `/api/v1/approvals/requests/{id}` | Get request details |
| POST | `/api/v1/approvals/requests/{id}/decide` | Approve or deny |
| GET | `/api/v1/approvals/requests/{id}/status` | Poll approval status |
| GET | `/api/v1/approvals/grants` | List active grants |
| DELETE | `/api/v1/approvals/grants/{id}` | Revoke grant |
| GET | `/api/v1/approvals/service-configs` | List service approval configs |
| PUT | `/api/v1/approvals/service-configs/{service_id}` | Configure approval requirement |
| DELETE | `/api/v1/approvals/service-configs/{service_id}` | Remove approval requirement |

### Notifications

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/notifications/settings` | Get notification preferences |
| PUT | `/api/v1/notifications/settings` | Update notification preferences |
| POST | `/api/v1/notifications/telegram/link` | Link Telegram account |
| DELETE | `/api/v1/notifications/telegram` | Disconnect Telegram |
| POST | `/api/v1/notifications/devices` | Register push notification device |
| GET | `/api/v1/notifications/devices` | List registered devices |
| DELETE | `/api/v1/notifications/devices/{id}` | Remove device |

### SSH

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/v1/ssh/{service_id}/certificate` | Issue SSH user certificate |
| POST | `/api/v1/ssh/{service_id}/exec` | Execute remote command |
| GET | `/api/v1/ssh/{service_id}/terminal` | Interactive terminal (WebSocket) |
| GET | `/api/v1/ssh/{service_id}` | SSH tunnel (WebSocket) |

### LLM Gateway

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/llm/status` | List available LLM providers |
| ANY | `/api/v1/llm/gateway/v1/{path}` | Unified LLM proxy (OpenAI-compatible) |
| ANY | `/api/v1/{provider_slug}/v1/{path}` | Provider-specific LLM proxy |

### MCP

| Method | Endpoint | Description |
|--------|----------|-------------|
| * | `/mcp` | MCP proxy endpoint (Streamable HTTP) |

### Public Endpoints (no auth)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| GET | `/llms.txt` | Short AI agent context |
| GET | `/llms-full.txt` | Full AI agent playbook |
| GET | `/.well-known/openid-configuration` | OIDC discovery |
| GET | `/.well-known/jwks.json` | JWT public keys |

---

## 20. Error Code Reference

All errors return JSON: `{ "error": "...", "error_code": N, "message": "..." }`

| Code | Error Key | HTTP | Description |
|------|-----------|------|-------------|
| 1000 | bad_request | 400 | Invalid request body or parameters |
| 1001 | unauthorized | 401 | Missing or invalid auth token |
| 1002 | forbidden | 403 | Insufficient permissions |
| 1003 | not_found | 404 | Resource not found |
| 1004 | conflict | 409 | Resource already exists (e.g., duplicate email) |
| 1005 | rate_limited | 429 | Too many requests |
| 1006 | internal_error | 500 | Internal server error |
| 1007 | database_error | 500 | Database operation failed |
| 1008 | validation_error | 422 | Input validation failed |
| 2000 | authentication_failed | 401 | Wrong email or password |
| 2001 | token_expired | 401 | Access or refresh token has expired |
| 2002 | mfa_required | 403 | MFA verification needed (includes `session_token`) |
| 3000 | pkce_verification_failed | 400 | PKCE code verifier mismatch |
| 3001 | invalid_redirect_uri | 400 | Redirect URI not registered |
| 3002 | invalid_scope | 400 | Requested scope not allowed |
| 3003 | consent_required | 403 | User consent needed (includes `consent_url`) |
| 3004 | unsupported_grant_type | 400 | Grant type not supported |
| 4006 | duplicate_slug | 409 | Slug already in use |
| 5000 | service_account_not_found | 404 | Service account does not exist |
| 5001 | service_account_inactive | 403 | Service account is disabled |
| 6000 | social_auth_failed | 400 | Social login provider error |
| 6001 | social_auth_conflict | 409 | Social account already linked to another user |
| 6004 | external_token_invalid | 401 | External provider token is invalid or expired |
| 7000 | approval_required | 403 | Service requires approval (includes `request_id`) |
| 8000 | node_not_found | 404 | Node does not exist |
| 8001 | node_offline | 502 | Node is not connected |
| 8002 | node_proxy_timeout | 504 | Node did not respond in time |
| 8003 | node_registration_failed | 400 | Node registration failed |

---

## 21. Troubleshooting

### "unauthorized" on all requests

- Check that the `Authorization: Bearer <token>` header is set
- Verify the token has not expired (default 15 min). Refresh with `/api/v1/auth/refresh`

### "mfa_required" error on login

Login returns `error_code: 2002` with a `session_token`. Complete the two-step flow:

```bash
# Step 1: Login attempt returns MFA challenge
curl -X POST http://localhost:3001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "password": "password"}'
# Response: { "error": "mfa_required", "error_code": 2002, "session_token": "..." }

# Step 2: Submit TOTP code with the session token
curl -X POST http://localhost:3001/api/v1/auth/mfa/verify \
  -H "Content-Type: application/json" \
  -d '{"session_token": "SESSION_TOKEN_FROM_STEP_1", "code": "123456"}'
# Response: { "access_token": "...", "refresh_token": "...", "expires_in": 900 }
```

### Token expired (error_code 2001)

Access tokens expire after 15 minutes. Refresh:

```bash
curl -X POST http://localhost:3001/api/v1/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "REFRESH_TOKEN"}'
```

For OAuth clients:

```bash
curl -X POST http://localhost:3001/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=refresh_token" \
  -d "refresh_token=REFRESH_TOKEN" \
  -d "client_id=YOUR_CLIENT_ID"
```

### OAuth callback fails with "invalid_grant"

- PKCE code verifier must match the one used to generate the code challenge
- Authorization code can only be used once
- `redirect_uri` must exactly match between authorize request and token exchange
- Code expires after ~60 seconds

### Service proxy returns 403 with "approval_required"

- The service requires approval before access
- Use the `request_id` from the error to poll: `GET /api/v1/approvals/requests/{request_id}/status`
- Admin can approve via: `POST /api/v1/approvals/requests/{request_id}/decide {"approved": true}`

### MCP client can't find tools

1. Verify at least one service has endpoints defined
2. Verify the user has connected to the service (`GET /api/v1/connections`)
3. Restart the MCP client after configuration changes

### Node agent connection issues

- Check the node is running: `nyxid-node status`
- Verify the WebSocket URL is correct and reachable
- For production, use `wss://` (not `ws://`)
- Check node heartbeat timeout (default 90s) -- node marked offline if no heartbeat

---

## 22. Common Pitfalls

1. **`allowed_scopes` is an array, not a string.** When creating OAuth clients, pass `["openid", "profile", "email"]`, not `"openid profile email"`.

2. **`discover-endpoints` takes no request body.** Set `openapi_spec_url` on the service first (via PUT), then call `POST .../discover-endpoints` with no body.

3. **Endpoint creation requires a `name` field.** E.g., `"name": "list_users"`.

4. **Redirect URIs must match exactly.** Trailing slashes matter. The `redirect_uri` in the token exchange must be byte-identical to what was registered.

5. **Authorization codes are single-use.** A second attempt returns `invalid_grant`.

6. **SDK packages use `@nyxids` scope.** `@nyxids/oauth-core` and `@nyxids/oauth-react`.

7. **PKCE is required for public clients.** `code_challenge` + `code_challenge_method=S256` are mandatory.

8. **Internal services don't need user credentials.** Pass an empty body `{}` when connecting.

9. **MFA error code is 2002, not 2001.** 2001 is `token_expired`.

10. **Node WebSocket URL must use `wss://` in production.** Use `ws://` only for local development.
