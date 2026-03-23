# NyxID OpenClaw Integration

`openclaw-plugin-nyxid` lets OpenClaw agents discover and call user-connected services through NyxID's credential brokering proxy.

## Quickstart (API Key -- Simplest Path)

### Step 1: Create a NyxID API key

1. Log in to your NyxID dashboard (hosted: `https://nyx.chrono-ai.fun`)
2. Go to **API Keys** in the sidebar
3. Click **Create API Key**
4. Name it something like `openclaw-agent`
5. Select the `proxy` scope (required for proxy access)
6. Copy the generated key (starts with `nyx_`)

### Step 2: Install the skill locally

Choose one of these methods:

**Option A -- Copy to OpenClaw managed skills (recommended):**

```bash
cp -r integrations/openclaw/skills/nyxid ~/.openclaw/skills/nyxid
```

**Option B -- Copy to a workspace:**

```bash
cp -r integrations/openclaw/skills/nyxid /path/to/your/workspace/skills/nyxid
```

**Option C -- Point OpenClaw to this repo's skill directory:**

Add to `~/.openclaw/openclaw.json`:

```json
{
  "skills": {
    "load": {
      "extraDirs": ["/absolute/path/to/NyxID/integrations/openclaw/skills"]
    }
  }
}
```

**Option D -- Install from ClawHub (when published):**

```bash
clawhub install nyxid
```

### Step 3: Configure credentials

Set the environment variable in your shell profile or in OpenClaw config.

**Shell profile (`~/.zshrc` or `~/.bashrc`):**

```bash
export NYXID_API_KEY="nyx_your_key_here"
export NYXID_BASE_URL="https://nyx-api.chrono-ai.fun"  # optional, this is the default
```

**Or in OpenClaw config (`~/.openclaw/openclaw.json`):**

```json
{
  "skills": {
    "entries": {
      "nyxid": {
        "enabled": true,
        "env": {
          "NYXID_API_KEY": "nyx_your_key_here",
          "NYXID_BASE_URL": "https://nyx-api.chrono-ai.fun"
        }
      }
    }
  }
}
```

### Step 4: Test it

Start a new OpenClaw session and ask: "What services do I have connected in NyxID?"

The agent will run `./tools/services.sh` and list your connected services. Then you can say things like "Post a tweet saying hello" and the agent will proxy through NyxID.

## Using the skill with other AI assistants

The skill's helper scripts (`tools/services.sh` and `tools/proxy.sh`) work standalone with `curl`. Any AI assistant (Claude Code, Codex, etc.) can use them if the environment variables are set.

### Claude Code / Codex / any terminal-based agent

```bash
# Set these in your shell before starting the agent
export NYXID_API_KEY="nyx_your_key_here"
export NYXID_BASE_URL="https://nyx-api.chrono-ai.fun"

# List available services
./tools/services.sh

# Call a service
./tools/proxy.sh twitter POST /2/tweets '{"text":"Hello from my agent"}'
./tools/proxy.sh github GET /user/repos
./tools/proxy.sh slack POST /chat.postMessage '{"channel":"#general","text":"Hello"}'
```

The agent only needs to know:
- `services.sh` lists what the user has connected
- `proxy.sh <service> <method> <path> [body]` calls a service through NyxID
- NyxID handles credential injection -- the agent never sees raw tokens

## Full plugin setup (OAuth mode -- advanced)

If you want interactive OAuth login instead of API key auth, configure the plugin with an OAuth client:

1. Create a developer app in NyxID (Dashboard > Developer Apps > Create)
2. Note the `client_id` and `client_secret`
3. Configure in `~/.openclaw/openclaw.json`:

```json
{
  "plugins": {
    "nyxid": {
      "enabled": true,
      "baseUrl": "https://nyx-api.chrono-ai.fun",
      "clientId": "your-client-id",
      "clientSecret": "your-client-secret",
      "delegationScopes": "proxy:*"
    }
  }
}
```

This enables OAuth 2.0 + PKCE login, automatic token refresh, and RFC 8693 delegated token exchange for proxy calls.

## What it supports

- OAuth 2.0 + PKCE login against NyxID
- Refresh token handling
- RFC 8693 token exchange when a confidential client is configured
- Direct proxy access with a NyxID API key (simplest setup)
- Direct proxy access with an existing NyxID access token
- A ClawHub-ready `nyxid` skill with helper scripts

## Auth modes

### API key mode (recommended for most users)

Set `NYXID_API_KEY` in your environment or OpenClaw config. The skill sends requests with `X-API-Key` header. No OAuth flow, no browser redirect, no client registration needed.

### OAuth mode

Provide `baseUrl` and `clientId`. Add `clientSecret` if you want RFC 8693 token exchange for delegated proxy calls. Requires registering a developer app in NyxID.

### Bearer token mode

Set `NYXID_ACCESS_TOKEN` if you already have a NyxID JWT from another source.

## Skill helpers

- `skills/nyxid/tools/services.sh` -- list available proxy services
- `skills/nyxid/tools/proxy.sh` -- send a proxied request through NyxID

Both scripts accept either `NYXID_ACCESS_TOKEN` or `NYXID_API_KEY`.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Set NYXID_API_KEY or NYXID_ACCESS_TOKEN` | No credentials configured | Set one of the env vars (see Step 3) |
| `1001 unauthorized` | API key is invalid or expired | Generate a new API key in NyxID dashboard |
| `1002 forbidden` | Missing scope or service not connected | Ensure the API key has `proxy` scope; connect the service in NyxID |
| `7000 approval_required` | Approval gating is enabled | Approve the request in your NyxID mobile app or Telegram |
| `8003 node_proxy_error` | Node-backed service proxy failed | Check the node agent is running and connected |
| Services list is empty | No services connected in NyxID | Go to NyxID dashboard and connect services (GitHub, Twitter, etc.) |
