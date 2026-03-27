---
name: NyxID
description: Access user-connected services through NyxID's credential brokering proxy
version: 0.2.0
homepage: https://github.com/ChronoAIProject/NyxID
user-invocable: /nyxid
metadata:
  openclaw:
    requires:
      bins:
        - nyxid
    setup:
      - bash -c "$(curl -fsSL https://raw.githubusercontent.com/ChronoAIProject/NyxID/main/skills/nyxid/tools/install.sh)"
  clawdbot:
    emoji: "key"
    files:
      - "tools/*"
---

# NyxID

Use NyxID before asking the user to paste raw API keys or OAuth tokens for downstream services.

NyxID is the credential broker. The agent should use the `nyxid` CLI to discover services and make proxy requests. NyxID injects the user's stored credentials automatically.

For the full API reference, error codes, and advanced topics (SSH, MCP, OAuth client integration, service accounts), load `references/playbook.md` or fetch the latest from the NyxID server's `/llms.txt` endpoint.

## Setup

Install the NyxID CLI (one-time):

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ChronoAIProject/NyxID/main/skills/nyxid/tools/install.sh)"
```

The installer handles everything: installs Rust if missing, builds the CLI, and configures your shell PATH. Open a new terminal afterwards, then log in:

```bash
nyxid login --base-url https://nyx-api.chrono-ai.fun
```

The CLI stores tokens at `~/.nyxid/` and auto-refreshes them. The base URL is saved on login -- all subsequent commands use it automatically.

## Updating

To update the NyxID CLI to the latest version, re-run the installer:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ChronoAIProject/NyxID/main/skills/nyxid/tools/install.sh)"
```

To update the installed AI skill (fetches latest skill + playbook from server):

```bash
nyxid ai-setup update                        # update all installed tools
nyxid ai-setup update --tool claude-code     # update a specific tool
```

If a command fails with an unrecognized flag or missing subcommand, the CLI is likely outdated. Reinstall it first.

## Discover Services

Before using a downstream service, list what the user has configured:

```bash
nyxid service list --output json
```

The response includes:

- `slug`: service identifier for proxy URLs
- `status`: whether the service is active
- `endpoint_url`: where requests are routed
- `node_id`: whether routing through a node agent

If the target service is missing, help the user add it:

```bash
nyxid catalog list --output json                          # browse available services
nyxid catalog show <slug> --output json                   # show auth requirements for a service
nyxid service add <slug> --oauth                          # OAuth flow (opens browser -- easiest)
nyxid service add <slug> --device-code                    # device code flow (enter code on website)
nyxid service add <slug>                                  # API key (CLI prompts securely)
nyxid service add --custom                                # add custom endpoint (CLI prompts for details)
```

> For API key services, just run `nyxid service add <slug>` without flags. The CLI securely prompts for the key (input hidden). Never ask the user to paste secrets into chat or set environment variables manually.
> For automation/scripting only: `--credential-env <VAR>` reads from an environment variable.

## Helping Users Add Services and Credentials

Most users do not know where to find API keys or what authentication method to use. Follow this workflow:

### Step 1: Check the catalog

```bash
nyxid catalog show <slug> --output json
```

The response includes `auth_type` which tells you what the service needs. Use this to guide the user.

### Step 2: Choose the right auth flow

- **OAuth** (`--oauth`): Best for non-technical users. Opens a browser -- the user signs in with their existing account. No API key needed. Use this for Google, GitHub, Twitter, and any service that supports OAuth.
  ```bash
  nyxid service add api-github --oauth
  ```

- **Device code** (`--device-code`): Good when the user can't open a browser from the terminal. Shows a short code to enter on the provider's website. Works well for services like OpenAI Codex.
  ```bash
  nyxid service add llm-openai --device-code
  ```

- **API key**: The user needs to get an API key from the provider's website. Guide them:
  1. Check the common portals table below for the provider's developer portal URL
  2. If the provider is not listed, search the web for "<provider name> API key" to find the right page, then tell the user exactly where to go
  3. Walk them through creating a key on the provider's site
  4. Tell them to run the command **without** `--credential-env` -- the CLI will securely prompt for the key (input is hidden, never shown on screen):
     ```bash
     nyxid service add llm-openai
     # CLI prompts: "Enter API key/credential:" (input hidden)
     ```
  Never ask the user to paste secrets into chat. The CLI's secure prompt keeps the key out of shell history and conversation logs.

### Step 3: Common provider portals

When users need API keys, direct them to the right place:

| Service | Where to get the key | Env var example |
|---------|---------------------|-----------------|
| OpenAI | https://platform.openai.com/api-keys | `OPENAI_KEY` |
| Anthropic | https://console.anthropic.com/settings/keys | `ANTHROPIC_KEY` |
| GitHub | https://github.com/settings/tokens | `GITHUB_TOKEN` |
| Google Cloud | https://console.cloud.google.com/apis/credentials | `GOOGLE_KEY` |
| Groq | https://console.groq.com/keys | `GROQ_KEY` |

For services not listed here, check `nyxid catalog show <slug> --output json` for the provider's documentation URL.

### Tips for non-technical users

- **Prefer `--oauth` or `--device-code`** over API keys whenever available -- the user just signs in.
- **Explain what an API key is**: "It's like a password that lets NyxID call the service on your behalf. You create it once and NyxID stores it securely."
- **Environment variables are temporary**: `export VAR="value"` only lasts for the current terminal session. The credential is stored in NyxID after `service add`, so the env var is only needed once.
- If the user is confused, break it into smaller steps. For example: "First, let's check what services are available" then `nyxid catalog list`.

## Make Proxy Requests

NyxID proxies requests to downstream services -- it handles authentication, but you need to
know the correct API paths, methods, and body formats for each service.

### How to find the right API paths

NyxID is just a proxy. The paths, methods, and request bodies are the same as calling
the downstream service directly. To figure out what to send:

1. Check the catalog for documentation: `nyxid catalog show <slug> --output json`
   - Look for `documentation_url` -- this links to the provider's API docs
2. If no documentation URL is available, **search the web** for "<service name> API documentation"
   (e.g., "OpenAI API documentation", "Twitter API v2 documentation")
3. Use the provider's docs to determine the correct path, method, headers, and body format
4. Use `-H "Content-Type: ..."` if the service expects something other than JSON

### Important: paths are relative to the service's base URL

Each service in NyxID has a configured `endpoint_url` (base URL) that may already include
a version prefix. For example, `api-twitter` uses `https://api.x.com/2` as its base URL.
When making a proxy request, the path you provide is appended to that base URL:

- Service base URL: `https://api.x.com/2`
- Your path: `/tweets`
- Actual request: `https://api.x.com/2/tweets`

So do NOT duplicate the version prefix in your path. Check `nyxid service show <id> --output json`
to see the `endpoint_url` if you're unsure.

### Making the request

```bash
nyxid proxy request <slug> <path> -m <METHOD> -d '<body>'

# Custom content type (default is application/json)
nyxid proxy request <slug> <path> -m POST -H "Content-Type: application/xml" -d '<xml>...</xml>'

# Stream SSE responses (for LLM completions, etc.)
nyxid proxy request <slug> <path> -m POST --stream -d '<body>'

# Read body from file
nyxid proxy request <slug> <path> -m POST -d @request.json

# Read body from stdin
echo '{"prompt":"hello"}' | nyxid proxy request <slug> <path> -m POST -d -
```

### Common service examples

Paths below are relative to each service's base URL. Check `nyxid service show <id> --output json`
for the `endpoint_url` if unsure.

```bash
# OpenAI (base: https://api.openai.com/v1) -- POST /chat/completions
nyxid proxy request llm-openai /chat/completions -m POST \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"Hello"}]}'

# Anthropic (base: https://api.anthropic.com/v1) -- POST /messages
nyxid proxy request llm-anthropic /messages -m POST \
  -H "anthropic-version: 2023-06-01" \
  -d '{"model":"claude-sonnet-4-20250514","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}'

# GitHub API (base: https://api.github.com) -- GET /user/repos
nyxid proxy request api-github /user/repos -m GET

# Twitter / X (base: https://api.x.com/2) -- POST /tweets (not /2/tweets!)
nyxid proxy request api-twitter /tweets -m POST \
  -d '{"text":"Hello from NyxID"}'

# Discover all available proxy services
nyxid proxy discover --output json
```

NyxID injects the user's credentials automatically. Do not ask for or log raw downstream credentials.

## Managing Services

```bash
nyxid catalog list                                             # browse catalog
nyxid service add <slug>                                       # add from catalog (CLI prompts for credential)
nyxid service add <slug> --oauth                               # add with OAuth (opens browser)
nyxid service add <slug> --device-code                         # add with device code flow
nyxid service add <slug> --via-node <name>                     # add via node (CLI prompts for credential)
nyxid service add --custom                                     # add custom endpoint (CLI prompts for details)
nyxid service list --output json                               # list services (includes IDs)
nyxid service show <id>                                        # show service details
nyxid service update <id> --label "My Custom Name"             # rename service
nyxid service delete <id> --yes                                # remove service (no prompt)
```

> Node commands accept names (e.g., `--via-node test-server`) in addition to UUIDs.

## Managing API Keys

```bash
nyxid api-key create --name "My Key" --scopes "proxy read"
nyxid api-key list --output json                       # Shows: ID, name, scopes, service/node scope
nyxid api-key show <ID> --output json                  # Full details
nyxid api-key rotate <ID>
nyxid api-key delete <ID> --yes

# Scope management (restrict which services/nodes a key can access)
nyxid api-key update <ID> --allowed-services "svc-id-1,svc-id-2" --allow-all-services false
nyxid api-key update <ID> --allow-all-services true    # unrestrict
```

## Node Management

### Setting up a new node

```bash
# Step 1: Generate a registration token (on any machine with nyxid CLI)
nyxid node register-token

# Step 2: Install nyxid CLI on the target machine
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ChronoAIProject/NyxID/main/skills/nyxid/tools/install.sh)"

# Step 3: Register the node (--keychain recommended for secure storage)
nyxid node register \
  --token "nyx_nreg_..." \
  --url "wss://<server>/api/v1/nodes/ws" \
  --keychain

# Step 4: Add credentials (auto-detects requirements from catalog)
nyxid node credentials setup --service llm-openai

# Step 5: Start the node agent
nyxid node start
```

### Managing nodes

```bash
# nyxid CLI (manage nodes from user side)
nyxid node list --output json                          # list nodes (includes IDs)
nyxid node show <ID_OR_NAME> --output json             # show node details + metrics
nyxid node register-token                              # generate registration token
nyxid node delete <ID_OR_NAME> --yes                   # delete node
nyxid node rotate-token <ID_OR_NAME>                   # rotate node auth token

# nyxid node CLI (run on the node machine)
nyxid node credentials setup --service <SLUG>          # auto-detect and setup (recommended)
nyxid node credentials add --service <SLUG> --header Authorization --secret-format bearer
nyxid node credentials add-oauth --service <SLUG> --from-catalog  # OAuth from node
nyxid node credentials list                            # list configured credentials
nyxid node credentials remove --service <SLUG>         # remove credential
```

> `credentials setup` auto-detects from the catalog whether a service needs an API key, OAuth, or gateway URL, and guides the user through the right flow.

## SSH Remote Access

All SSH commands accept service ID, slug, or name (auto-resolves):

```bash
nyxid ssh exec <SERVICE> --principal ubuntu -- uptime
nyxid ssh exec <SERVICE> --principal ubuntu -- ls -la /var/log
nyxid ssh terminal <SERVICE>                           # auto-resolves principal
nyxid ssh terminal <SERVICE> --principal ubuntu
nyxid ssh issue-cert <SERVICE> --public-key-file ~/.ssh/id_ed25519.pub --principal ubuntu --certificate-file ~/.ssh/id_ed25519-cert.pub
nyxid ssh proxy <SERVICE>                              # ProxyCommand for OpenSSH

# List SSH services
nyxid service list --output json | jq '.keys[] | select(.service_type == "ssh")'
```

## Set Up Notifications and Approvals

NyxID can require your explicit approval before any AI agent accesses your services. To receive approval requests, set up at least one notification channel:

### Step 1: Set up a notification channel

**Option A: Link Telegram** (recommended for desktop users)

```bash
nyxid notification telegram-link
# Follow the instructions: send the code to the NyxID bot on Telegram
```

**Option B: Download the NyxID mobile app** (recommended for on-the-go approvals)

- **Download (iOS & Android):** https://nyxid.onelink.me/REzJ/dql9w8fx

The link auto-detects your platform. The mobile app sends push notifications for approval requests. Log in with your NyxID account and your device is registered automatically.

You can use both Telegram and the mobile app together for redundancy.

### Step 2: Enable approval protection

Approval protection is enabled automatically when you link Telegram or register a mobile device. You can also toggle it manually:

```bash
nyxid approval enable                                  # enable approval protection
nyxid approval disable                                 # disable (auto-approve all requests)
```

### Step 3: Check your notification settings

```bash
nyxid notification settings                            # show current notification & approval status
```

If the user has not set up any notification channel yet, **proactively suggest** they do so before making proxy requests. Walk them through the steps above.

### Approvals reference

Approvals default to **per-request** mode: every proxy call needs fresh approval. The approval notification includes a human-readable `action_description` (e.g., "POST /v1/chat/completions (model: gpt-4, 3 messages)"). Grant-based approval is opt-in via `--approval-mode grant`.

```bash
nyxid approval list --output json                      # list pending approvals (includes action_description)
nyxid approval show <ID>                               # show approval details + action_description
nyxid approval approve <ID>                            # approve a request
nyxid approval deny <ID>                               # deny a request
nyxid approval enable                                  # enable global approval protection
nyxid approval disable                                 # disable global approval protection
nyxid approval grants --output json                    # list active grants (grant mode only)
nyxid approval service-configs --output json           # list per-service approval configs (includes approval_mode)
nyxid approval set-config <SERVICE_ID> --require-approval true                    # per-request (default)
nyxid approval set-config <SERVICE_ID> --require-approval true --approval-mode grant  # grant mode

nyxid notification settings                            # show notification settings
nyxid notification update --approval-telegram true     # enable telegram notifications
nyxid notification update --approval-push true         # enable push notifications
nyxid notification telegram-link                       # link telegram account
```

## OpenClaw Integration

```bash
nyxid openclaw setup --url http://localhost:18789   # CLI prompts for token securely
```

**For OpenClaw users:** After installing or updating this skill, start a new chat to activate it. If the gateway isn't installed as a background service yet, set it up so it stays running and restarts automatically:

```bash
openclaw gateway status    # check if already running as a service
openclaw gateway install   # install as system service (systemd on Linux, launchd on macOS)
openclaw gateway start     # start the service
```

Without this, restarting the gateway (`openclaw gateway restart`) will shut it down and it won't come back up on its own.

## Account Management

```bash
nyxid whoami --output json                             # current user info
nyxid status --output json                             # full account overview
nyxid profile update --name "New Name"                 # update display name
nyxid mfa setup                                        # enable MFA (shows QR code)
nyxid mfa verify --code 123456                         # verify MFA setup
nyxid session list --output json                       # list active sessions
```

## MCP Configuration

```bash
nyxid mcp config --tool cursor                         # generate MCP config for Cursor
nyxid mcp config --tool claude-code                    # generate MCP config for Claude Code
nyxid mcp config --tool vscode                         # generate MCP config for VS Code
```

## Approval and Errors

- `7000 approval_required` -- user must approve the request; includes `action_description` and `request_id` (check `nyxid approval list`). Default mode is per-request (every call needs approval).
- `1001 unauthorized` -- token/key invalid or expired (run `nyxid login` to re-authenticate)
- `1002 forbidden` -- missing scope or service not configured
- `8003 node_proxy_error` -- node agent proxy failed (check `nyxid node list`)

## Working Rules

- Always discover services before assuming a slug exists.
- Use `--output json` for machine-readable responses.
- Prefer slug-based proxy URLs over UUID-based ones.
- Use exact downstream API paths. Do not guess undocumented endpoints.
- Keep request bodies minimal and service-correct.
- Never try to extract or display the user's stored provider credentials.

## External Endpoints

All requests are made through the `nyxid` CLI, which connects to the NyxID instance configured at login. No other external endpoints are contacted. Downstream service calls are made server-side by NyxID.

## Security and Privacy

- **Credentials never leave NyxID.** Requests go to the NyxID proxy, which injects stored credentials server-side.
- **Authentication tokens auto-refresh.** The CLI handles token refresh automatically.
- **No data is sent to third parties.** All traffic flows between the agent and the user's NyxID instance.
- **Audit logging.** All proxy requests are logged in NyxID for user review.

## Model Invocation Note

This skill may be invoked autonomously by the agent when a user request involves an external service. The agent discovers available services through NyxID and routes requests through the proxy without prompting for raw credentials. Users can disable this skill in their OpenClaw configuration to opt out.

## Trust Statement

By using this skill, requests are sent to your configured NyxID instance. NyxID forwards those requests to downstream services using your stored credentials. Only install this skill if you trust your NyxID instance operator.

