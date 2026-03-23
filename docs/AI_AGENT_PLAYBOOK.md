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
6. [Connect User Credentials to a Service](#6-connect-user-credentials-to-a-service)
7. [Set Up MCP Proxy for AI Clients](#7-set-up-mcp-proxy-for-ai-clients)
8. [Use the Credential Proxy](#8-use-the-credential-proxy)
9. [Set Up a Provider (OAuth / API Key / Device Code)](#9-set-up-a-provider-oauth--api-key--device-code)
10. [Deploy a Node Agent (On-Premise Credentials)](#10-deploy-a-node-agent-on-premise-credentials)
11. [Add Login to a React App (OAuth Client)](#11-add-login-to-a-react-app-oauth-client)
12. [Add Login to Any Web App (Raw OAuth / OIDC)](#12-add-login-to-any-web-app-raw-oauth--oidc)
13. [Server-to-Server Authentication (Service Accounts)](#13-server-to-server-authentication-service-accounts)
14. [API Quick Reference](#14-api-quick-reference)
15. [Error Code Reference](#15-error-code-reference)
16. [Troubleshooting](#16-troubleshooting)
17. [Common Pitfalls](#17-common-pitfalls)

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

### Authenticate via API

All API calls (except public endpoints) require a Bearer token. Get one by logging in:

```bash
ACCESS_TOKEN=$(curl -s -X POST http://localhost:3001/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "USER_EMAIL", "password": "USER_PASSWORD"}' \
  | jq -r '.access_token')
```

Use it in subsequent requests:

```bash
curl http://localhost:3001/api/v1/users/me \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

Access tokens expire in 15 minutes. Refresh:

```bash
curl -X POST http://localhost:3001/api/v1/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "REFRESH_TOKEN"}'
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

## 6. Connect User Credentials to a Service

**Goal:** Store a user's credential for a service so NyxID can inject it when proxying requests.

### Via Dashboard

1. Go to http://localhost:3000/connections
2. Find the service and click "Connect"
3. Enter the credential (API key, bearer token, etc.)

### Via API

```bash
curl -X POST http://localhost:3001/api/v1/connections/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "credential": "Bearer sk-proj-your-api-key-here",
    "credential_label": "My Production Key"
  }'
```

**Update an existing credential:**

```bash
curl -X PUT http://localhost:3001/api/v1/connections/$SERVICE_ID/credential \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "credential": "Bearer sk-proj-new-key",
    "credential_label": "Rotated Key"
  }'
```

**Disconnect:**

```bash
curl -X DELETE http://localhost:3001/api/v1/connections/$SERVICE_ID \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

---

## 7. Set Up MCP Proxy for AI Clients

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

## 8. Use the Credential Proxy

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

## 9. Set Up a Provider (OAuth / API Key / Device Code)

**Goal:** Register an external provider that users can connect their accounts to.

### OAuth 2.0 Provider

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

## 10. Deploy a Node Agent (On-Premise Credentials)

**Goal:** Keep sensitive credentials on your own infrastructure. NyxID routes proxy requests through the node agent, which injects credentials locally -- they never leave your machine.

### Step 1: Generate a registration token

Via dashboard: http://localhost:3000/nodes, or via API:

```bash
curl -X POST http://localhost:3001/api/v1/nodes/register-token \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Production Node"}'
```

Returns: `{ "token": "nyx_nreg_...", "token_id": "...", "expires_at": "..." }`

### Step 2: Install and register the node agent

On the target machine:

```bash
# Download or build nyxid-node binary
# Then register:
nyxid-node register \
  --token "nyx_nreg_..." \
  --url "ws://localhost:3001/api/v1/nodes/ws"
```

For production with TLS, use `wss://` instead of `ws://`.

### Step 3: Add credentials to the node

```bash
nyxid-node credentials add \
  --service "openai" \
  --header "Authorization" \
  --value "Bearer sk-proj-your-secret-key"
```

### Step 4: Bind services to the node

In the dashboard (http://localhost:3000/nodes/{nodeId}) or via API:

```bash
curl -X POST http://localhost:3001/api/v1/nodes/$NODE_ID/bindings \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"service_id": "SERVICE_UUID"}'
```

### Step 5: Start the node agent

```bash
nyxid-node start
```

Now proxy requests to bound services will be routed through the node. Credentials are injected locally and never transit the NyxID server.

### Node management commands

```bash
nyxid-node status              # Check connection status
nyxid-node credentials list    # List configured credentials
nyxid-node credentials remove --service "openai"  # Remove a credential
```

---

## 11. Add Login to a React App (OAuth Client)

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

## 12. Add Login to Any Web App (Raw OAuth / OIDC)

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

## 13. Server-to-Server Authentication (Service Accounts)

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

## 14. API Quick Reference

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

## 15. Error Code Reference

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

## 16. Troubleshooting

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

## 17. Common Pitfalls

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
