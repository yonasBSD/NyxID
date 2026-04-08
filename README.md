<!-- TODO: Hero banner
     Recommended: 1280x640px, dark background, NyxID logo + tagline
     Place at: assets/banner.png
     <p align="center">
       <img src="assets/banner.png" alt="NyxID — Agent Connectivity Gateway" width="100%">
     </p>
-->

# NyxID

**Open-source Agent Connectivity Gateway.** Turn your localhost into an MCP server.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![GitHub Stars](https://img.shields.io/github/stars/ChronoAIProject/NyxID)](https://github.com/ChronoAIProject/NyxID)

NyxID lets your AI agents (Claude Code, Cursor, n8n) reach any API you have,
public or private, and handles all the credentials so your agent never sees
a raw key.

```
Claude Code / Cursor / n8n
         |
         v
      NyxID (cloud gateway)
         |
    +----+----+
    v    v    v
 Public  Internal  localhost
  APIs    APIs     services
```

NyxID proxies requests, injects credentials automatically, punches through
NAT to reach your local services, and wraps any REST API as MCP tools.

<!-- TODO: Product screenshot
     Replace the ASCII diagram above with a polished architecture diagram or dashboard screenshot.
     <p align="center">
       <img src="assets/screenshot.png" alt="NyxID Dashboard" width="80%">
     </p>
-->

## What NyxID Does

- **Reach anything** — public APIs, internal APIs, localhost services via credential nodes (`nyxid node`). SSH tunneling (`nyxid ssh`) reaches remote hosts. No VPN, no port forwarding.
- **Never expose keys** — the reverse proxy injects credentials automatically. Your agent talks to NyxID; NyxID talks to the API with the real key.
- **MCP auto-wrap** — REST APIs with OpenAPI specs become MCP tools. `nyxid mcp config --tool cursor` generates the config. Works with Claude Code, Cursor, VS Code, and any MCP client.
- **Per-agent isolation** — each agent gets a scoped token. Agent A accesses Slack and Gmail. Agent B only accesses your internal API. Revoke any session without touching the underlying credentials.
- **Full identity layer** — OIDC/OAuth 2.0 with PKCE, RBAC, service accounts, transaction approval (Telegram + mobile push), LLM gateway for 7 providers.

## Why NyxID

| | NyxID | 1Password UA | Cloudflare Tunnel | Keycloak |
|---|---|---|---|---|
| Open source | Yes | No | No | Yes |
| NAT traversal to localhost | Yes (`nyxid node`) | No | Yes (no credentials) | No |
| Credential injection | Yes (any API) | Partner integrations | No | No |
| REST to MCP auto-wrap | Yes | No | No | No |
| Per-agent isolation | Yes | No | No | No |
| OIDC / OAuth 2.0 | Yes | No | No | Yes |

<!-- TODO: Demo GIF
     15-30 second terminal recording: install CLI → login → proxy a request
     Tools: https://github.com/charmbracelet/vhs or https://asciinema.org
     <p align="center">
       <img src="assets/demo.gif" alt="NyxID Quick Start Demo" width="80%">
     </p>
-->

## Quick Start

### Hosted (recommended)

1. Sign up at the [NyxID console](https://auth.nyxid.dev) and add the API credentials you want your agents to use.

2. Install the CLI and log in:

```bash
cargo install --git https://github.com/ChronoAIProject/NyxID.git nyxid-cli
nyxid login --base-url https://auth.nyxid.dev
```

### Self-host

```bash
git clone https://github.com/ChronoAIProject/NyxID.git && cd NyxID
cp .env.example .env          # edit: set ENCRYPTION_KEY=$(openssl rand -hex 32)
                              #        set INVITE_CODE_REQUIRED=false
docker compose up -d                  # MongoDB + Mailpit
cargo run -p nyxid &                  # Backend on :3001
cd frontend && npm i && npm run dev   # Frontend on :5173
```

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for production setup.

## Connect Your AI Agent

```bash
nyxid api-key create --name my-agent    # creates an API key for MCP auth
nyxid mcp config --tool claude-code     # or: --tool cursor, --tool vscode
```

Follow the output to add NyxID to your MCP config. Your agent can now call any API you added through NyxID's authenticated proxy — credentials are injected automatically.

NyxID's MCP transport (`/mcp`) exposes your connected services as tools automatically. Service endpoints are loaded on-demand and mapped to MCP tools you can call from any MCP client.

### Reach local services (optional)

Have services behind a firewall? Deploy a credential node — it makes an outbound WebSocket connection to NyxID. No port forwarding required.

```bash
nyxid node register --token <reg-token> --url wss://<your-server>/api/v1/nodes/ws
nyxid node credentials add --service my-local-api --header Authorization --secret-format bearer
nyxid node start
```

The node makes an outbound WebSocket connection to NyxID. No port forwarding.
No VPN. Your AI agents can now reach localhost services through the tunnel.

## Quick Start (with AI assistant)

Paste this into Claude Code, Cursor, or any AI coding assistant:

> Help me set up NyxID. Install the CLI (cargo install --git
> https://github.com/ChronoAIProject/NyxID.git nyxid-cli), log in with
> nyxid login, add my OpenAI API key, and configure MCP so I can use
> NyxID-proxied tools from this session.

<!-- AI quickstart maintenance: validate this prompt against actual CLI on each release -->

## Use Cases

- Give Claude Code access to your private APIs without sharing keys
- Expose internal microservices to AI agents through a single MCP endpoint
- Secure AI agent access to self-hosted tools (Grafana, Jenkins, n8n) behind your firewall

## Resources

| Topic | Link |
|-------|------|
| API Reference | [docs/API.md](docs/API.md) |
| Architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| AI Agent Playbook | [docs/AI_AGENT_PLAYBOOK.md](docs/AI_AGENT_PLAYBOOK.md) |
| Credential Nodes | [docs/NODE_PROXY.md](docs/NODE_PROXY.md) |
| MCP Integration | [docs/MCP_DELEGATION_FLOW.md](docs/MCP_DELEGATION_FLOW.md) |
| SSH Tunneling | [docs/SSH_TUNNELING.md](docs/SSH_TUNNELING.md) |
| Security | [docs/SECURITY.md](docs/SECURITY.md) |
| Environment Variables | [docs/ENV.md](docs/ENV.md) |
| Deployment | [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) |
| Developer Guide | [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) |

## Contributing

We welcome contributions. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[Apache-2.0](LICENSE)
