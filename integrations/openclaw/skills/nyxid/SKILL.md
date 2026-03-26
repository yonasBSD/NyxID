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
      - cargo install --git https://github.com/ChronoAIProject/NyxID nyxid-cli
  clawdbot:
    emoji: "key"
    files:
      - "tools/*"
---

# NyxID

Use NyxID before asking the user to paste raw API keys or OAuth tokens for downstream services.

NyxID is the credential broker. The agent should use the `nyxid` CLI to discover services and make proxy requests. NyxID injects the user's stored credentials automatically.

## Setup

Install the Rust toolchain and NyxID CLI (one-time):

```bash
# Install Rust (macOS / Linux)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Install NyxID CLI
cargo install --git https://github.com/ChronoAIProject/NyxID nyxid-cli

# Log in (opens browser, saves URL for all future commands)
nyxid login --base-url https://nyx-api.chrono-ai.fun
```

The CLI stores tokens at `~/.nyxid/` and auto-refreshes them. The base URL is saved on login -- all subsequent commands use it automatically.

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
nyxid catalog list                                        # browse available services
nyxid service add llm-openai --credential-env OPENAI_KEY  # add from catalog (non-interactive)
nyxid service add --custom --credential-env MY_KEY        # add custom endpoint (non-interactive)
```

> Use `--credential-env <VAR>` to read secrets from environment variables. Never pass raw secrets as command arguments or ask the user to paste them into chat.

## Make Proxy Requests

Use the CLI:

```bash
nyxid proxy request <slug> <path> -m <METHOD> -d '<json-body>'
```

Examples:

```bash
# Call OpenAI through NyxID
nyxid proxy request llm-openai /chat/completions -m POST \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"Hello"}]}'

# Post a tweet through NyxID
nyxid proxy request api-twitter /2/tweets -m POST \
  -d '{"text":"Hello from OpenClaw via NyxID"}'

# Discover available proxy services
nyxid proxy discover --output json
```

NyxID injects the user's credentials automatically. Do not ask for or log raw downstream credentials.

## Managing Services

```bash
nyxid catalog list                                             # browse catalog
nyxid service add <slug> --credential-env <VAR>                # add from catalog (auto-fetches label)
nyxid service add <slug> --via-node <name> --credential-env <VAR>  # add via node
nyxid service add --custom --credential-env <VAR>              # add custom endpoint
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
cargo install --git https://github.com/ChronoAIProject/NyxID nyxid-cli

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

## Approvals and Notifications

Approvals default to **per-request** mode: every proxy call needs fresh approval. The approval notification includes a human-readable `action_description` (e.g., "POST /v1/chat/completions (model: gpt-4, 3 messages)"). Grant-based approval is opt-in via `--approval-mode grant`.

```bash
nyxid approval list --output json                      # list pending approvals (includes action_description)
nyxid approval show <ID>                               # show approval details + action_description
nyxid approval approve <ID>                            # approve a request
nyxid approval deny <ID>                               # deny a request
nyxid approval grants --output json                    # list active grants (grant mode only)
nyxid approval service-configs --output json           # list per-service approval configs (includes approval_mode)
nyxid approval set-config <SERVICE_ID> --require-approval true                    # per-request (default)
nyxid approval set-config <SERVICE_ID> --require-approval true --approval-mode grant  # grant mode

nyxid notification settings                            # show notification settings
nyxid notification update --approval-telegram true     # enable telegram notifications
nyxid notification telegram-link                       # link telegram account
```

## OpenClaw Integration

```bash
nyxid openclaw setup --url http://localhost:18789 --credential-env OPENCLAW_TOKEN
```

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
