# Agent Isolation Design

## Implementation Status

All core phases are implemented on the `feat/agent-isolation` branch.

| Phase | Status | Notes |
|---|---|---|
| **1:** Agent identity propagation | Complete | `AuthUser` carries `api_key_id`, `api_key_name`; audit logs include per-agent attribution |
| **2:** Per-agent credential override | Complete | `agent_service_bindings` collection, REST API, proxy integration |
| **3:** CLI profile system | Complete | `--profile` flag, `NYXID_PROFILE` env var, profile-aware token storage |
| **4:** Node multi-instance | Complete | Profile-aware service labels and config directories |
| **5:** Platform integration | Complete | `nyxid ai-setup agent` commands, `platform` field on `ApiKey`, per-platform setup instructions |
| **6:** Per-agent rate limiting | Complete | `PerAgentRateLimiter` with per-API-key buckets, `rate_limit_per_second`/`burst` on `ApiKey` |
| **7a:** API key detail page | Complete | Platform selector, rate limit editor, credential bindings CRUD, usage stats, bindings count in table |
| **7b:** Agent usage dashboard | Complete | `/keys` page shows per-agent request counts, error rates, top services, 7-day activity bars, provider-reported token totals, and reported cost when the upstream response includes a cost field. |
| **7c:** Admin audit filtering | Complete | Admin audit log supports `api_key_id` filter and exposes `api_key_id` / `api_key_name` columns |

### Implementation notes

- The `PerAgentRateLimiter` uses an in-memory token bucket per API key with a background cleanup task (60-second interval, evicts entries idle for 120 seconds)
- `audit_service::log_async` signature expanded from 6 to 8 arguments to include `api_key_id` and `api_key_name`
- All callers of `audit_service::log_async` across the codebase were updated to pass the two new `Option<String>` arguments (passing `None` for non-proxy paths)
- The `AiToolTarget` enum gained a `Generic` variant for platforms without specific skill integrations
- Profile name validation enforces alphanumeric + hyphens + underscores, 1-64 characters, preventing path traversal
- Proxy responses include `X-NyxID-Agent-Id` response header when the request is authenticated via an API key. The header value is the API key ID. This allows downstream consumers and observability tools to attribute traffic to a specific agent
- New usage endpoints: `GET /api/v1/api-keys/usage` and `GET /api/v1/api-keys/{id}/usage`
- LLM and proxy paths emit `llm_usage_reported` audit events with provider-reported prompt/completion/total tokens and reported cost when the upstream includes it
- Admin audit log endpoint now accepts `api_key_id` filtering and returns `api_key_id` + `api_key_name` on each entry

---

## Problem Statement

A single NyxID user often operates multiple AI agents -- a coding agent (Claude Code), a research agent (Codex), a chat agent (OpenClaw workspace), a customer-support bot, etc. Today:

1. **All agents share the same external credentials.** Two agents accessing the same provider (e.g., OpenAI) get the same `UserApiKey` injected. No way to give a coding agent a $50/month key and a research agent a $500/month key.

2. **No per-agent credential binding at proxy time.** The proxy resolves credentials via `UserService.api_key_id` (external credential). It cannot pick a different credential based on which NyxID API key made the request.

3. **No agent-level audit trail.** Logs record `user_id` but not which API key (agent) initiated the request. Cannot answer: "How many tokens did my coding agent consume?"

4. **No per-agent rate limits or quotas.** A runaway agent exhausts the budget for all other agents.

5. **CLI is single-profile.** `~/.nyxid/` stores one set of tokens. On a machine running Claude Code, Codex, and an OpenClaw workspace, all three share one session. No way for each agent to authenticate with its own scoped API key.

6. **Node agent is single-instance per machine.** The daemon uses a hardcoded service label (`dev.nyxid.node` / `nyxid-node.service`). Cannot run multiple node daemons for different agent contexts.

---

## How Agent Platforms Work Today

Understanding how each platform manages credentials informs our design:

| Platform | Credential model | How they'd consume NyxID |
|---|---|---|
| **Claude Code** | MCP server config in `.claude/` per project; env vars interpolated | MCP server config with `NYXID_ACCESS_TOKEN` env var per project |
| **OpenAI Codex** | `OPENAI_API_KEY` env var; tool configs per session | CLI tool calls via `nyxid proxy request`; env var for token |
| **OpenClaw** | Per-workspace config; each workspace has its own gateway credentials + bearer token | NyxID skill installed per workspace; each workspace uses its own API key |
| **CrewAI / LangGraph / AutoGen** | Environment variables or tool init params per agent | Env var `NYXID_ACCESS_TOKEN` or CLI calls per agent process |

**Common pattern:** Every platform supports per-agent/per-workspace environment variables. The simplest isolation mechanism is: **each agent gets its own NyxID API key, passed via environment variable.**

---

## Current State

### What already works

| Capability | Status | How |
|---|---|---|
| Service allow-list per API key | Working | `ApiKey.allowed_service_ids` checked at proxy time |
| Node allow-list per API key | Working | `ApiKey.allowed_node_ids` checked at proxy time |
| Scoped key prefix (`nyxid_ag_`) | Working | Created by `key_service` when scope fields are set |
| Scope enforcement on new UserService path | Working | `proxy.rs:338-361` |
| Token override via env var | Working | `AuthArgs.access_token_env` reads from `NYXID_ACCESS_TOKEN` |

### What's missing

| Gap | Impact |
|---|---|
| `AuthUser` does not carry `api_key_id` | Cannot distinguish agents at request time |
| No per-agent credential override | All agents share one credential per service |
| No agent-aware audit events | Cannot attribute usage/cost per agent |
| No per-agent rate limiting | One agent can starve others |
| CLI has no `--profile` flag | Cannot manage multiple agent identities on one machine |
| Node daemon is single-instance | Cannot run separate node contexts for different agents |
| NyxID skill has no agent identity concept | All AI agents use the same session |

### Data flow today

```
Agent request (with API key)
  |
  v
AuthUser { user_id, allowed_service_ids }    <-- no api_key_id
  |
  v
resolve UserService by slug + user_id
  |
  v
inject credential from UserService.api_key_id  <-- same for ALL agents
  |
  v
proxy to target
  |
  v
audit log { user_id, service_id }            <-- no api_key_id
```

---

## Design Proposal

### Core Principle: One API Key = One Agent Identity

Instead of introducing a new `Agent` model, we extend the existing `ApiKey` + CLI profile system:

- Each agent (Claude Code project, OpenClaw workspace, Codex session) gets its own scoped NyxID API key
- The CLI `--profile` flag lets users manage multiple agent identities on one machine
- The API key carries through to credential resolution, audit, and rate limiting
- Platform-specific setup is handled by the NyxID skill and `nyxid ai-setup`

### Target data flow

```
Agent request (with scoped API key "coding-agent")
  |
  v
AuthUser { user_id, api_key_id: "coding-agent", ... }
  |
  v
resolve UserService by slug + user_id
  |
  v
lookup agent_service_bindings(api_key_id, user_service_id)
  |
  +-- found --> inject OVERRIDE credential (agent-specific)
  +-- not found --> inject UserService.api_key_id (default)
  |
  v
proxy to target (with agent-specific credential)
  |
  v
audit log { user_id, api_key_id, api_key_name, service_id }
```

---

## Phase 1: Agent Identity Propagation (Backend)

**Goal:** Every request made with an API key carries the key's identity through the system.

### 1a. Add `api_key_id` to `AuthUser`

```rust
pub struct AuthUser {
    // ... existing fields ...
    pub api_key_id: Option<String>,   // NEW: populated when auth_method == ApiKey
    pub api_key_name: Option<String>, // NEW: human-readable label for audit
}
```

Populated in `mw/auth.rs` when validating an API key (the `ApiKey` record is already fetched).

### 1b. Include `api_key_id` in audit events

Update `audit_service::log_async` and the `AuditEvent` model to accept an optional `api_key_id` + `api_key_name`. All proxy-related audit events include them when present.

### 1c. Add `X-NyxID-Agent-Id` response header

Returned on proxy responses when the request was made with an API key. Helps agents self-identify in their own logs.

**Migration:** Zero-downtime. New fields are `Option`, old events have `None`.

---

## Phase 2: Per-Agent Credential Override (Backend)

**Goal:** Different API keys (agents) can use different external credentials for the same service.

### 2a. New collection: `agent_service_bindings`

```
agent_service_bindings
  _id: String (UUID)
  api_key_id: String         -- FK to ApiKey (the agent)
  user_service_id: String    -- FK to UserService
  user_api_key_id: String    -- FK to UserApiKey (credential to inject)
  user_id: String            -- denormalized for query efficiency
  created_at: DateTime<Utc>
  updated_at: DateTime<Utc>
```

Index: `{ api_key_id: 1, user_service_id: 1 }` unique.

### 2b. Credential resolution change

Resolution order in proxy:
1. If request has `api_key_id`, look up `agent_service_bindings(api_key_id, user_service_id)`
2. If binding found, use the override credential
3. If not found, fall back to `UserService.api_key_id` (existing behavior)

Single indexed lookup on a small collection (agents x services per user). Minimal hot-path impact.

### 2c. API endpoints

```
POST   /api/v1/api-keys/{key_id}/bindings          -- create binding
GET    /api/v1/api-keys/{key_id}/bindings          -- list bindings for key
DELETE /api/v1/api-keys/{key_id}/bindings/{id}     -- remove binding
```

Validation:
- `api_key_id` must belong to the requesting user
- `user_service_id` must belong to the user AND be in the key's `allowed_service_ids` (if scoped)
- `user_api_key_id` must belong to the user

---

## Phase 3: CLI Profile System

**Goal:** Multiple agent identities on one machine. Each profile is an isolated auth context.

### 3a. `--profile` global flag

```rust
#[derive(Args, Clone)]
pub struct AuthArgs {
    // ... existing fields ...
    /// Agent profile name (isolates tokens and config)
    #[arg(long, env = "NYXID_PROFILE")]
    pub profile: Option<String>,
}
```

### 3b. Profile-aware token storage

Current: `~/.nyxid/{access_token, refresh_token, base_url}`

New layout:

```
~/.nyxid/
  access_token              <-- "default" profile (backward compat)
  refresh_token
  base_url
  profiles/
    coding-agent/
      access_token          <-- per-profile tokens
      refresh_token
      base_url
    research-agent/
      access_token
      refresh_token
      base_url
```

Resolution: if `--profile` or `NYXID_PROFILE` is set, read from `~/.nyxid/profiles/{name}/`. Otherwise, read from `~/.nyxid/` (existing behavior, full backward compat).

### 3c. Profile-aware login

```bash
# Login as default (existing behavior)
nyxid login --base-url https://nyx-api.chrono-ai.fun

# Login for a specific agent profile
nyxid login --base-url https://nyx-api.chrono-ai.fun --profile coding-agent

# Or use API key directly (no login needed)
nyxid api-key create --name "coding-agent" --scopes "proxy read" \
  --allowed-services "svc-openai,svc-github" --allow-all-services false
```

For agent use, the preferred flow is: **create a scoped API key server-side, then configure the agent platform to pass it via `NYXID_ACCESS_TOKEN`** -- no interactive login needed.

### 3d. Profile-aware commands

All commands that use `AuthArgs` automatically support `--profile`:

```bash
nyxid service list --profile coding-agent
nyxid proxy request llm-openai /chat/completions --profile coding-agent -m POST -d '...'
nyxid api-key list --profile research-agent
```

Environment variable override: `NYXID_PROFILE=coding-agent nyxid proxy request ...`

---

## Phase 4: Node Agent Multi-Instance

**Goal:** Run multiple node daemons on the same machine for different agent contexts.

### 4a. Profile-aware service identity

Current: hardcoded `dev.nyxid.node` (macOS) / `nyxid-node.service` (Linux).

New:

```rust
fn launchd_label(profile: &str) -> String {
    if profile == "default" { "dev.nyxid.node".to_string() }
    else { format!("dev.nyxid.node.{profile}") }
}

fn systemd_unit(profile: &str) -> String {
    if profile == "default" { "nyxid-node.service".to_string() }
    else { format!("nyxid-node-{profile}.service") }
}
```

### 4b. Profile-aware config directory

Current: `~/.nyxid-node/`

New:
```
~/.nyxid-node/                 <-- "default" profile (backward compat)
  config.toml
  .encryption_key
  daemon.toml
  logs/

~/.nyxid-node/profiles/
  coding-agent/
    config.toml
    .encryption_key
    daemon.toml
    logs/
  research-agent/
    config.toml
    .encryption_key
    daemon.toml
    logs/
```

### 4c. Multi-daemon commands

```bash
# Default node (existing behavior, backward compat)
nyxid node register --token nyx_nreg_...
nyxid node daemon install
nyxid node daemon start

# Profile-specific node
nyxid node register --token nyx_nreg_... --profile coding-agent
nyxid node credentials setup --service llm-openai --profile coding-agent
nyxid node daemon install --profile coding-agent
nyxid node daemon start --profile coding-agent

# Each profile gets its own daemon process
nyxid node daemon status                      # default: running
nyxid node daemon status --profile coding-agent  # coding-agent: running
```

macOS result: two LaunchAgents (`dev.nyxid.node.plist` + `dev.nyxid.node.coding-agent.plist`).

### 4d. Docker deployment

For users who prefer Docker over native daemons, each node profile runs as a separate container with its config directory mounted as a volume. Docker provides natural process isolation per agent -- each container has its own filesystem, credentials, and lifecycle.

**Setup:**

```bash
# 1. Register the node on the host (creates config.toml + .keyfile)
nyxid node register --token nyx_nreg_... --url wss://nyx-api.chrono-ai.fun/api/v1/nodes/ws
nyxid node credentials setup --service llm-openai

# 2. Build the node agent image (once)
nyxid node docker build

# 3. Start the container (mounts ~/.nyxid-node/ into /app/config)
nyxid node docker start
```

**Multi-profile with Docker:**

```bash
# Register separate profiles
nyxid node register --token nyx_nreg_... --url wss://... --profile coding-agent
nyxid node register --token nyx_nreg_... --url wss://... --profile research-agent

# Add credentials per profile
nyxid node credentials setup --service llm-openai --profile coding-agent
nyxid node credentials setup --service llm-openai --profile research-agent

# Start separate containers (each gets its own config volume)
nyxid node docker start --profile coding-agent
nyxid node docker start --profile research-agent

# Check status
nyxid node docker status --profile coding-agent   # nyxid-node-coding-agent: running
nyxid node docker status --profile research-agent  # nyxid-node-research-agent: running
```

Each container reads from `~/.nyxid-node/profiles/{name}/`, so credentials are fully isolated per agent. The config volume is mounted read-write so the agent can update OAuth tokens during refresh.

**Docker vs native daemon comparison:**

| Aspect | Native daemon (launchd/systemd) | Docker container |
|---|---|---|
| Process isolation | Shared OS, separate service labels | Full container isolation |
| Multi-profile | `--profile` flag on all commands | One container per profile |
| Credential storage | File or keychain backend | File backend only (no OS keychain) |
| Auto-restart | launchd/systemd restart policy | `--restart unless-stopped` |
| Logs | `nyxid node daemon logs` | `docker logs -f <container>` |
| Credential updates | Hot-reload within 5s | Same (watches mounted volume) |
| OS support | macOS (launchd) + Linux (systemd) | Any Docker host |

**Files:**
- `cli/Dockerfile.node` -- Multi-stage build for the node agent image
- `docker-compose.node.yml` -- Compose file with default + example profile services
- `nyxid node docker` -- CLI subcommand for Docker container lifecycle (build/start/stop/status/logs)

---

## Phase 5: Platform Integration (Skill + ai-setup)

**Goal:** Each agent platform can be configured to use its own isolated NyxID identity.

### 5a. `nyxid ai-setup` agent provisioning

New command to create a complete agent identity in one step:

```bash
# Create a scoped API key + optional credential bindings for an agent
nyxid ai-setup agent create \
  --name "coding-agent" \
  --platform claude-code \
  --services llm-openai,api-github \
  --output json
```

This does:
1. Creates a scoped `ApiKey` with `allowed_service_ids` matching the requested services
2. Returns the API key value (shown once)
3. Outputs platform-specific configuration instructions

Output example:
```json
{
  "api_key_id": "uuid",
  "api_key": "nyxid_ag_xxx...",
  "name": "coding-agent",
  "platform": "claude-code",
  "setup_instructions": "Add to .claude/settings.json:\n..."
}
```

### 5b. Platform-specific setup instructions

#### Claude Code

```bash
nyxid ai-setup agent create --name "coding-agent" --platform claude-code --services llm-openai
```

Generates MCP server config for `.claude/settings.json`:

```json
{
  "mcpServers": {
    "nyxid": {
      "command": "nyxid",
      "args": ["mcp", "serve"],
      "env": {
        "NYXID_ACCESS_TOKEN": "nyxid_ag_xxx..."
      }
    }
  }
}
```

Each Claude Code project can have a different `NYXID_ACCESS_TOKEN`, isolating which services and credentials that project's agent can access.

#### OpenAI Codex

```bash
nyxid ai-setup agent create --name "research-agent" --platform codex --services llm-openai
```

Generates environment setup:

```bash
# Add to your shell profile or Codex project config:
export NYXID_ACCESS_TOKEN="nyxid_ag_yyy..."
```

Codex calls `nyxid proxy request` which reads `NYXID_ACCESS_TOKEN` automatically.

#### OpenClaw (per-workspace)

```bash
nyxid ai-setup agent create --name "support-bot" --platform openclaw --services llm-openai,llm-anthropic
```

Generates OpenClaw workspace config:

```yaml
# Add to your OpenClaw workspace config:
skills:
  nyxid:
    env:
      NYXID_ACCESS_TOKEN: "nyxid_ag_zzz..."
```

Each OpenClaw workspace gets its own NyxID API key. When the skill calls `nyxid proxy request`, it uses the workspace-specific token, which resolves to the workspace-specific credential bindings.

#### Generic (any platform)

```bash
nyxid ai-setup agent create --name "my-agent" --platform generic --services llm-openai
```

Returns the API key and universal instructions:

```
Set NYXID_ACCESS_TOKEN=nyxid_ag_... in your agent's environment.
All nyxid CLI commands will use this token automatically.
Proxy: nyxid proxy request <slug> <path> -m POST -d '...'
Direct API: curl -H "X-API-Key: nyxid_ag_..." https://nyx-api.../api/v1/proxy/s/<slug>/<path>
```

### 5c. Agent management commands

```bash
# List all agent identities
nyxid ai-setup agent list --output json

# Show agent details (services, bindings, usage)
nyxid ai-setup agent show coding-agent --output json

# Bind a specific credential to an agent for a service
nyxid ai-setup agent bind coding-agent \
  --service llm-openai \
  --credential openai-premium-key

# Rotate an agent's API key
nyxid ai-setup agent rotate coding-agent

# Delete an agent identity
nyxid ai-setup agent delete coding-agent --yes
```

### 5d. Update NyxID skill (SKILL.md)

Add agent identity section to the skill:

```markdown
## Agent Identity

Each agent should use its own NyxID API key for credential isolation.
The key is set via the NYXID_ACCESS_TOKEN environment variable.

If NYXID_ACCESS_TOKEN is set, all `nyxid` commands use it automatically.
Different agents (workspaces, projects) can use different keys,
each scoped to specific services with independent credential bindings.

To check which agent identity is active:
  nyxid whoami --output json

To see which services this agent can access:
  nyxid service list --output json
```

---

## Phase 6: Per-Agent Rate Limiting

**Goal:** Prevent one agent from exhausting another's budget.

### 6a. Add rate limit fields to `ApiKey`

```rust
pub struct ApiKey {
    // ... existing fields ...
    pub rate_limit_per_second: Option<u32>,
    pub rate_limit_burst: Option<u32>,
}
```

### 6b. Rate limit middleware change

Current: rate limit key = `user_id`.

New: if `AuthUser.api_key_id` is present and the key has rate limit overrides, use `api_key_id` as the rate limit key. Otherwise, fall back to user-level limits.

---

## Phase 7: Frontend & Observability

### 7a. API Key detail page

Extend to show:
- Credential bindings per service
- Usage stats filtered by `api_key_id`
- Rate limit configuration
- Platform label (claude-code, codex, openclaw, etc.)

### 7b. Agent usage dashboard

New section on the keys page:
- Per-agent request counts over time (implemented)
- Error rates per agent (implemented)
- Top services per agent (implemented)
- 7-day activity buckets (implemented)
- Provider-reported token attribution per agent (implemented)
- Provider-reported cost attribution per agent when the upstream includes a cost field (implemented)

### 7c. Admin audit filtering

Add `api_key_id` column to audit log viewer.

---

## End-to-End Example: Three Agents on One Machine

### Setup

```bash
# User logs in once (creates default profile)
nyxid login --base-url https://nyx-api.chrono-ai.fun

# User has two OpenAI credentials: a cheap one and a premium one
# (already added via nyxid service add)
nyxid service list --output json
# -> llm-openai (default credential: $50/mo key)
# -> api-github (OAuth connected)

# Create three agent identities
nyxid ai-setup agent create --name "claude-coding" \
  --platform claude-code \
  --services llm-openai,api-github

nyxid ai-setup agent create --name "codex-research" \
  --platform codex \
  --services llm-openai

nyxid ai-setup agent create --name "openclaw-support" \
  --platform openclaw \
  --services llm-openai,llm-anthropic

# Bind premium OpenAI key to the research agent
nyxid ai-setup agent bind codex-research \
  --service llm-openai \
  --credential openai-premium

# Set rate limit on support bot
nyxid api-key update <support-key-id> \
  --rate-limit-per-second 5 --rate-limit-burst 10
```

### Runtime behavior

```
claude-coding calls /proxy/s/llm-openai/chat/completions
  -> AuthUser.api_key_id = "claude-coding-key-id"
  -> No binding found -> uses default $50/mo OpenAI key
  -> Audit: { api_key_name: "claude-coding", service: "llm-openai" }

codex-research calls /proxy/s/llm-openai/chat/completions
  -> AuthUser.api_key_id = "codex-research-key-id"
  -> Binding found -> uses $500/mo premium OpenAI key
  -> Audit: { api_key_name: "codex-research", service: "llm-openai" }

openclaw-support calls /proxy/s/llm-openai/chat/completions
  -> AuthUser.api_key_id = "openclaw-support-key-id"
  -> No binding found -> uses default $50/mo OpenAI key
  -> Rate limited to 5 req/s independently
  -> Audit: { api_key_name: "openclaw-support", service: "llm-openai" }

claude-coding tries /proxy/s/llm-anthropic/messages
  -> allowed_service_ids does not include llm-anthropic
  -> 403 ApiKeyScopeForbidden
```

### With node agent (credential-on-machine)

```bash
# Register a node for the coding agent context
nyxid node register --token nyx_nreg_... --profile claude-coding
nyxid node credentials setup --service llm-openai --profile claude-coding
nyxid node daemon install --profile claude-coding
nyxid node daemon start --profile claude-coding

# Register a separate node for the research agent
nyxid node register --token nyx_nreg_... --profile codex-research
nyxid node credentials setup --service llm-openai --profile codex-research
nyxid node daemon install --profile codex-research
nyxid node daemon start --profile codex-research

# Two LaunchAgents running:
# dev.nyxid.node.claude-coding  -> injects coding agent's OpenAI key
# dev.nyxid.node.codex-research -> injects research agent's premium key
```

---

## Data Model Changes Summary

### Modified collections

| Collection | Change | Phase |
|---|---|---|
| `api_keys` | Add `rate_limit_per_second`, `rate_limit_burst`, `platform` | 5, 6 |
| `audit_events` | Add `api_key_id`, `api_key_name` | 1 |

### New collection

| Collection | Purpose | Phase |
|---|---|---|
| `agent_service_bindings` | Per-agent credential overrides | 2 |

### Modified structs

| Struct | Change | Phase |
|---|---|---|
| `AuthUser` | Add `api_key_id`, `api_key_name` | 1 |
| `AuditEvent` | Add `api_key_id`, `api_key_name` | 1 |
| `ApiKey` | Add `rate_limit_per_second`, `rate_limit_burst`, `platform` | 5, 6 |

---

## API Changes Summary

### New endpoints

| Method | Path | Description | Phase |
|---|---|---|---|
| POST | `/api/v1/api-keys/{key_id}/bindings` | Create agent-service credential binding | 2 |
| GET | `/api/v1/api-keys/{key_id}/bindings` | List bindings for an agent key | 2 |
| DELETE | `/api/v1/api-keys/{key_id}/bindings/{id}` | Remove a binding | 2 |

### Modified endpoints

| Method | Path | Change | Phase |
|---|---|---|---|
| POST | `/api/v1/api-keys` | Accept `platform` field | 5 |
| PATCH | `/api/v1/api-keys/{key_id}` | Accept `rate_limit_*` fields | 6 |

---

## CLI Changes Summary

### New flags

| Flag | Scope | Description | Phase |
|---|---|---|---|
| `--profile <name>` | Global (AuthArgs) | Agent profile for token isolation | 3 |
| `NYXID_PROFILE` env var | Global | Same as `--profile` | 3 |

### New commands

| Command | Description | Phase |
|---|---|---|
| `nyxid ai-setup agent create` | Create agent identity with scoped key | 5 |
| `nyxid ai-setup agent list` | List agent identities | 5 |
| `nyxid ai-setup agent show <name>` | Show agent details + bindings | 5 |
| `nyxid ai-setup agent bind <name>` | Bind credential to agent for a service | 5 |
| `nyxid ai-setup agent rotate <name>` | Rotate agent's API key | 5 |
| `nyxid ai-setup agent delete <name>` | Delete agent identity | 5 |

### Modified commands

| Command | Change | Phase |
|---|---|---|
| `nyxid login` | Support `--profile` | 3 |
| `nyxid node daemon install/start/stop/...` | Support `--profile` for multi-instance | 4 |
| `nyxid node register` | Support `--profile` | 4 |
| `nyxid node credentials *` | Support `--profile` | 4 |

---

## Skill Changes (SKILL.md)

### New section: Agent Identity

Document that each agent/workspace should use its own `NYXID_ACCESS_TOKEN` for isolation. Include setup instructions for each supported platform.

### Updated: Setup section

Add agent provisioning flow:
```bash
nyxid ai-setup agent create --name "my-agent" --platform <platform> --services <slugs>
```

### Updated: Working Rules

Add rule: "If multiple agents share a machine, each should have its own NYXID_ACCESS_TOKEN. Never share API keys across agent contexts."

---

## File Changes Summary

### Backend

| File | Change | Phase |
|---|---|---|
| `mw/auth.rs` | Add `api_key_id`, `api_key_name` to `AuthUser` | 1 |
| `services/audit_service.rs` | Accept + store `api_key_id` | 1 |
| `models/audit_event.rs` | Add `api_key_id`, `api_key_name` fields | 1 |
| `handlers/proxy.rs` | Pass `api_key_id` to audit; credential override lookup | 1, 2 |
| `models/agent_service_binding.rs` | NEW: binding model | 2 |
| `services/agent_binding_service.rs` | NEW: CRUD + lookup | 2 |
| `handlers/agent_bindings.rs` | NEW: REST endpoints | 2 |
| `routes.rs` | Register new routes | 2 |
| `db.rs` | Add indexes for `agent_service_bindings` | 2 |
| `models/api_key.rs` | Add `rate_limit_*`, `platform` fields | 5, 6 |
| `mw/rate_limit.rs` | Per-agent rate limit buckets | 6 |

### CLI

| File | Change | Phase |
|---|---|---|
| `cli/src/cli.rs` | Add `--profile` to `AuthArgs` | 3 |
| `cli/src/auth.rs` | Profile-aware token storage paths | 3 |
| `cli/src/node/daemon.rs` | Profile-aware service labels + config dirs | 4 |
| `cli/src/node/config.rs` | Profile-aware config directory resolution | 4 |
| `cli/src/commands/ai_setup.rs` | NEW: `agent create/list/show/bind/rotate/delete` | 5 |

### Skill

| File | Change | Phase |
|---|---|---|
| `skills/nyxid/SKILL.md` | Add Agent Identity section; update setup + working rules | 5 |

---

## Migration Strategy

All changes are additive. No breaking changes.

| Phase | Breaking? | Backward compat |
|---|---|---|
| 1 - Identity propagation | No | New `AuthUser` fields are `Option`, default `None` |
| 2 - Credential override | No | New collection; no bindings = existing behavior |
| 3 - CLI profiles | No | No `--profile` flag = reads from `~/.nyxid/` (unchanged) |
| 4 - Node multi-instance | No | No `--profile` = uses `~/.nyxid-node/` and hardcoded labels |
| 5 - Platform integration | No | New commands only; existing `ai-setup` untouched |
| 6 - Rate limiting | No | No `rate_limit_*` on key = user-level limits (unchanged) |
| 7 - Frontend | No | Additive UI |

---

## Implementation Order

| Phase | Effort | Depends on | Independently shippable? |
|---|---|---|---|
| **1:** Agent identity propagation | S | None | Yes |
| **2:** Per-agent credential override | M | Phase 1 | Yes |
| **3:** CLI profile system | S | None | Yes (parallel with 1) |
| **4:** Node multi-instance | M | Phase 3 | Yes |
| **5:** Platform integration (ai-setup + skill) | M | Phase 1, 2, 3 | Yes |
| **6:** Per-agent rate limiting | S | Phase 1 | Yes |
| **7:** Frontend & observability | M | Phase 1, 2 | Yes |

**Recommended:** Phase 1 + 3 in parallel -> Phase 2 -> Phase 5 -> Phase 4, 6, 7 as needed.

Phases 3 (CLI profiles) can ship independently and provides immediate value: agents can use `NYXID_PROFILE` or `NYXID_ACCESS_TOKEN` to authenticate with different scoped keys today, even before credential overrides exist.

---

## Open Questions

1. **Should unscoped keys (`allow_all_services: true`) support credential overrides?** Proposal: yes. The binding is per-service, not per-scope.

2. **Quota/budget limits beyond rate limiting?** e.g., "Agent A can make at most 1000 requests/day". Bigger feature (counter storage, reset schedules). Could be Phase 8.

3. **Should bindings support endpoint overrides?** e.g., Agent A uses a different base URL. Proposal: no. Different URL = create a separate UserService. Keeps bindings credential-only.

4. **Naming: "Agent" vs "Scoped Key"?** The UI currently says "API Keys". Proposal: keep "API Keys" in the generic UI but use "Agent" in the `ai-setup agent` CLI commands and skill docs, since that's the mental model for AI platform users.

5. **Should `nyxid ai-setup agent create` auto-login the profile?** Proposal: no. The command creates a scoped API key server-side and returns it. The agent platform uses it via `NYXID_ACCESS_TOKEN` env var. Interactive login is only needed for human CLI use.

6. **Node profile vs CLI profile -- same namespace?** Proposal: yes, `--profile coding-agent` resolves to `~/.nyxid/profiles/coding-agent/` for CLI tokens and `~/.nyxid-node/profiles/coding-agent/` for node config. Same name, different directories.
