# OpenClaw + NyxID Integration Guide

This repository ships an OpenClaw integration at [`integrations/openclaw`](../integrations/openclaw).

## Included Assets

- `openclaw-plugin-nyxid`: TypeScript auth plugin for OpenClaw
- `skills/nyxid`: Skill bundle with helper shell scripts (works standalone or via ClawHub)
- `openclaw.plugin.json`: OpenClaw auth-plugin manifest with bundled skill reference

## Default Hosted Instance

The hosted NyxID base URL is:

```
https://nyx-api.chrono-ai.fun
```

The hosted NyxID dashboard is:

```
https://nyx.chrono-ai.fun
```

## Installation

### Local installation (no ClawHub required)

**Option A -- Copy to OpenClaw managed skills (recommended):**

```bash
mkdir -p ~/.openclaw/skills
cp -r integrations/openclaw/skills/nyxid ~/.openclaw/skills/nyxid
```

This makes the skill available to all OpenClaw agents on your machine.

**Option B -- Copy to a workspace (project-scoped):**

```bash
cp -r integrations/openclaw/skills/nyxid /path/to/workspace/skills/nyxid
```

Workspace skills take highest precedence in OpenClaw's skill loading order.

**Option C -- Install from Git URL:**

Paste the repository URL into OpenClaw chat:

```
https://github.com/ChronoAIProject/NyxID
```

OpenClaw will clone and install the skill automatically.

### ClawHub installation (when published)

```bash
clawhub install nyxid
```

### Plugin installation (optional, for OAuth flows)

The plugin provides OAuth login, token refresh, and RFC 8693 delegation. Install it if you need interactive authentication rather than API key access:

```bash
cd integrations/openclaw
npm install
npm run build
```

Then reference it in your OpenClaw plugin config (see Configuration below).

## Configuration

### Quickstart: API key mode (simplest)

1. Log in to NyxID dashboard (`https://nyx.chrono-ai.fun`)
2. Go to **API Keys** > **Create API Key**
3. Name: `openclaw-agent`, Scope: `proxy`
4. Copy the key (starts with `nyx_`)
5. Configure OpenClaw -- add to `~/.openclaw/openclaw.json`:

```json
{
  "skills": {
    "entries": {
      "nyxid": {
        "enabled": true,
        "env": {
          "NYXID_API_KEY": "nyx_your_key_here"
        }
      }
    }
  }
}
```

Or set shell environment variables:

```bash
export NYXID_API_KEY="nyx_your_key_here"
export NYXID_BASE_URL="https://nyx-api.chrono-ai.fun"  # optional, this is the default
```

6. Reload OpenClaw so the new skill is picked up:
   - **Start a new chat session** -- simplest option, just open a new conversation
   - **Restart the gateway** -- `openclaw gateway restart`
   - **Docker** -- `docker compose restart openclaw-gateway`
   - **Hot reload** -- set `"gateway": { "reload": { "mode": "hybrid" } }` in gateway config (this is the default; auto-restarts when new skills are detected)
   - Verify with `openclaw gateway status`
7. Ask: "What services do I have connected in NyxID?"

### Full plugin config (OAuth mode)

For interactive OAuth login with token refresh and delegation:

1. Create a developer app in NyxID (Dashboard > Developer Apps > Create)
2. Note the `client_id` and `client_secret`
3. Add to `~/.openclaw/openclaw.json`:

```json
{
  "plugins": {
    "nyxid": {
      "enabled": true,
      "baseUrl": "https://nyx-api.chrono-ai.fun",
      "clientId": "your-client-id",
      "clientSecret": "your-client-secret",
      "defaultScopes": "openid profile email",
      "delegationScopes": "proxy:*"
    }
  }
}
```

## Auth Modes

### API key mode

Set `NYXID_API_KEY` (env var or config). The skill sends requests with `X-API-Key` header. No OAuth flow, no browser redirect, no client registration needed. Best for most users.

### OAuth mode

Provide `clientId` with `baseUrl`. Add `clientSecret` for RFC 8693 token exchange. Requires registering a developer app in NyxID.

### Bearer token mode

Set `NYXID_ACCESS_TOKEN` if you already have a NyxID JWT from another source.

## Using with other AI assistants

The helper scripts work with any terminal-based AI assistant. Set the environment variables and the assistant can use the scripts directly.

### Claude Code

```bash
export NYXID_API_KEY="nyx_your_key_here"
export NYXID_BASE_URL="https://nyx-api.chrono-ai.fun"

# Then in Claude Code, the agent can run:
# ./tools/services.sh          -- list connected services
# ./tools/proxy.sh twitter POST /2/tweets '{"text":"Hello"}'
```

### Codex / other agents

Same pattern -- set the env vars, then the agent calls `services.sh` to discover services and `proxy.sh` to make requests. The scripts are self-documenting and print usage on error.

### What the agent needs to know

- `services.sh` returns a JSON list of connected services with their slugs
- `proxy.sh <service> <method> <path> [body]` calls any connected service
- NyxID injects credentials server-side -- the agent never handles raw tokens
- If a response has `error_code: 7000`, the user needs to approve the request
- If a service shows `connected: false`, the user needs to connect it in NyxID first

## Flow Summary

### API key flow

1. OpenClaw loads `NYXID_API_KEY` from env or config
2. `nyxid_list_services` calls `GET /api/v1/proxy/services` with `X-API-Key`
3. `nyxid_proxy` calls the slug-based proxy endpoint with the same key
4. NyxID injects the user's downstream credentials and forwards the request

### OAuth flow

1. OpenClaw authenticates the user via OAuth 2.0 Authorization Code + PKCE
2. The plugin stores the access and refresh tokens in the OpenClaw auth profile
3. `nyxid_list_services` calls `GET /api/v1/proxy/services` with the user access token
4. `nyxid_proxy` exchanges the access token for a delegated token via RFC 8693
5. NyxID injects the user's downstream credentials and forwards the request

## Current Backend Constraints

- RFC 8693 token exchange requires a confidential NyxID OAuth client (`clientSecret` required)
- Delegated NyxID tokens cannot call `GET /api/v1/proxy/services`; service discovery must use the base user token or API key
- Approval-gated proxy calls are blocking and end in success or `403 Forbidden`
- The bundled skill helper scripts accept either `NYXID_ACCESS_TOKEN` or `NYXID_API_KEY`

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Set NYXID_API_KEY or NYXID_ACCESS_TOKEN` | No credentials configured | Set one of the env vars |
| `1001 unauthorized` | Key is invalid or expired | Generate a new API key in NyxID |
| `1002 forbidden` | Missing scope or service not connected | Ensure key has `proxy` scope; connect service in NyxID |
| `7000 approval_required` | Approval gating enabled for this service | Approve in NyxID mobile app or Telegram |
| `8003 node_proxy_error` | Node-backed proxy failed | Check node agent is running |
| Empty services list | No services connected | Connect services in NyxID dashboard |
| Skill not loading in OpenClaw | Skill not in a recognized directory | Copy to `~/.openclaw/skills/nyxid` or add `extraDirs` |
| `curl: command not found` | curl not installed | Install curl (`brew install curl` / `apt install curl`) |
