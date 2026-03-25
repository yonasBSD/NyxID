> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# CLI Audit: Gap Analysis

Audit of `nyxid` CLI vs API endpoints and frontend capabilities.

---

## Section 1: Missing API Coverage

### CRITICAL -- Core User Flows

| # | API Endpoint | Description | CLI Status |
|---|---|---|---|
| C1 | `POST /api/v1/auth/register` | User registration | Missing |
| C2 | `POST /api/v1/auth/verify-email` | Email verification | Missing |
| C3 | `POST /api/v1/auth/forgot-password` | Password reset request | Missing |
| C4 | `POST /api/v1/auth/reset-password` | Password reset | Missing |
| C5 | `PUT /api/v1/users/me` | Update user profile (name, etc.) | Missing |
| C6 | `POST /api/v1/auth/mfa/*` | MFA setup, confirm, verify, disable | Missing |
| C7 | `GET /api/v1/sessions` | List active sessions | Missing |
| C8 | `GET /api/v1/proxy/services` | List proxyable services (service discovery) | Missing |
| C9 | `ANY /api/v1/proxy/s/{slug}/{path}` | Proxy request by slug | Missing (no `nyxid proxy` command) |
| C10 | `ANY /api/v1/proxy/{service_id}/{path}` | Proxy request by ID | Missing |
| C11 | SSH service creation via `POST /api/v1/keys` | Create SSH services (host, port, cert auth, principals, TTL) | Missing -- `service add` only handles http-type |
| C12 | Credential rotation for external API keys (`PUT /api/v1/api-keys/external/{key_id}`) | Rotate/update external credentials | Missing -- no `service rotate-credential` |
| C13 | Service activate/deactivate (`PUT /api/v1/user-services/{service_id}` with `is_active`) | Toggle service active state | Missing -- `service update` lacks `--active/--inactive` |

### IMPORTANT -- Features Users Will Expect

| # | API Endpoint | Description | CLI Status |
|---|---|---|---|
| I1 | `DELETE /api/v1/users/me` | Delete own account | Missing |
| I2 | `GET /api/v1/users/me/consents` | List OAuth consents | Missing |
| I3 | `DELETE /api/v1/users/me/consents/{client_id}` | Revoke OAuth consent | Missing |
| I4 | `DELETE /api/v1/nodes/{node_id}` | Delete a node | Missing -- `node` has list, show, register-token only |
| I5 | `POST /api/v1/nodes/{node_id}/rotate-token` | Rotate node auth token | Missing |
| I6 | `GET /api/v1/nodes/{node_id}/bindings` | List node service bindings | Missing |
| I7 | `POST /api/v1/nodes/{node_id}/bindings` | Create node service binding | Missing |
| I8 | `PATCH /api/v1/nodes/{node_id}/bindings/{binding_id}` | Update binding priority | Missing |
| I9 | `DELETE /api/v1/nodes/{node_id}/bindings/{binding_id}` | Delete node binding | Missing |
| I10 | `GET /api/v1/nodes/my-bindings` | List user's bound services | Missing |
| I11 | `GET /api/v1/notifications/settings` | Get notification settings | Missing |
| I12 | `PUT /api/v1/notifications/settings` | Update notification settings | Missing |
| I13 | `POST /api/v1/notifications/telegram/link` | Link Telegram account | Missing |
| I14 | `DELETE /api/v1/notifications/telegram` | Disconnect Telegram | Missing |
| I15 | `GET /api/v1/approvals/requests` | List approval requests | Missing |
| I16 | `POST /api/v1/approvals/requests/{id}/decide` | Decide (approve/deny) request | Missing |
| I17 | `GET /api/v1/approvals/grants` | List approval grants | Missing |
| I18 | `DELETE /api/v1/approvals/grants/{id}` | Revoke approval grant | Missing |
| I19 | `GET /api/v1/approvals/service-configs` | List per-service approval configs | Missing |
| I20 | `PUT /api/v1/approvals/service-configs/{id}` | Set service approval config | Missing |
| I21 | OAuth connect flow (`GET /providers/{id}/connect/oauth`) | Initiate OAuth for services | Missing -- `service add` only does api_key credential |
| I22 | Device code flow (`POST /providers/{id}/connect/device-code/initiate`, `poll`) | Device code auth | Missing -- frontend supports it, CLI does not |
| I23 | Provider credentials (`GET/PUT/DELETE /providers/{id}/credentials`) | User OAuth credentials (client_id/secret) | Missing |
| I24 | `GET /api/v1/endpoints` | List user endpoints | Missing as standalone (only used in `service update`) |
| I25 | `GET /api/v1/api-keys/external` | List external API keys | Missing as standalone |
| I26 | `GET /api/v1/user-services` | List user services | Missing as standalone |
| I27 | `POST /api/v1/ssh/{service_id}/exec` | SSH remote command execution | Missing |
| I28 | `GET /api/v1/ssh/{service_id}/terminal` | SSH web terminal | Missing |

### NICE-TO-HAVE -- Admin or Rarely-Used

| # | API Endpoint | Description | CLI Status |
|---|---|---|---|
| N1 | `GET /api/v1/connections` | List connections (old model) | Missing |
| N2 | `POST /api/v1/connections/{service_id}` | Connect service (old model) | Missing |
| N3 | `GET /api/v1/providers` | List providers | Missing |
| N4 | `GET /api/v1/llm/status` | LLM gateway status | Missing |
| N5 | `ANY /api/v1/llm/gateway/v1/{path}` | LLM gateway proxy | Missing |
| N6 | `ANY /api/v1/llm/{provider}/v1/{path}` | LLM provider proxy | Missing |
| N7 | `POST /api/v1/delegation/refresh` | Refresh delegated token | Missing |
| N8 | All `/api/v1/admin/*` routes | Admin endpoints (user mgmt, audit, roles, groups, nodes) | Missing entirely |
| N9 | All `/api/v1/developer/oauth-clients/*` | Developer app management | Missing |
| N10 | `GET /api/v1/mcp/config` | Get MCP config from server | Missing (CLI generates locally) |
| N11 | `GET /api/v1/notifications/devices` | List push notification devices | Missing |
| N12 | `POST /api/v1/notifications/devices` | Register push device | Missing |
| N13 | `POST /api/v1/integrations/openclaw/mappings` | Create OpenClaw channel mapping | Missing |
| N14 | `POST /api/v1/auth/setup` | Initial admin setup | Missing |
| N15 | `POST /api/v1/auth/cli-token` | CLI token endpoint | Present (used by login) |

---

## Section 2: Missing Frontend Feature Parity

### AI Services Page (`/keys`)

| Feature | Frontend | CLI |
|---|---|---|
| List external services (card grid) | Yes | Yes (`service list`) |
| List NyxID API keys (table) | Yes | Yes (`api-key list`) |
| Add service from catalog | Yes (wizard) | Yes (`service add <slug>`) |
| Add custom HTTP endpoint | Yes (wizard) | Yes (`service add --custom`) |
| Add custom SSH service | Yes (wizard with SSH fields) | **No** -- `service add` has no SSH fields |
| Add service via OAuth flow | Yes (redirects to OAuth) | **No** |
| Add service via device code flow | Yes (polls with UI) | **No** |
| Routing choice (direct vs node) | Yes (wizard step) | Partial (`--via-node`) |
| Node setup helper (credentials command) | Yes (copyable command) | **No** |

### Key Detail Page (`/keys/:keyId`)

| Feature | Frontend | CLI |
|---|---|---|
| Show endpoint, credential, service, routing | Yes | Partial (`service show`) -- no credential status |
| Edit endpoint URL | Yes (inline edit) | Yes (`service update --endpoint-url`) |
| Rotate external credential | Yes (inline) | **No** |
| Toggle service active/inactive | Yes | **No** |
| Change routing (direct/node picker) | Yes (dropdown) | Partial (`service update --node-id / --no-node`) |
| Node setup helper | Yes | **No** |
| SSH connection details (host, port, CA key, principals, TTL) | Yes | **No** |
| SSH setup instructions (sshd_config, TrustedUserCAKeys) | Yes | **No** |
| SSH terminal button | Yes (links to web terminal) | **No** (`ssh proxy` exists but no `ssh exec` or terminal) |
| Delete service | Yes | Yes (`service delete`) |

### API Key Detail Page (`/api-keys/:keyId`)

| Feature | Frontend | CLI |
|---|---|---|
| View key details (name, prefix, scopes, expiry) | Yes | Yes (`api-key show`) |
| Edit key name/description | Yes (inline) | Yes (`api-key update --name`) |
| Edit scopes (checkbox UI) | Yes | Yes (`api-key update --scopes`) |
| Edit service scope (allow_all_services, pick specific) | Yes (checkbox picker) | Yes (`api-key update --allowed-services`) |
| Edit node scope (allow_all_nodes, pick specific) | Yes (checkbox picker) | Yes (`api-key update --allowed-nodes`) |
| Rotate key | Yes (dialog) | Yes (`api-key rotate`) |
| Revoke key | Yes (dialog) | Yes (`api-key delete`) |

### AI Setup Page (`/ai-setup`)

| Feature | Frontend | CLI |
|---|---|---|
| MCP config for Cursor/Claude Code/VSCode | Yes (auto-generated with OAuth client) | Partial (`mcp config`) -- generates API-key-based config, not OAuth-based |
| Quick start prompts | Yes (copy-paste prompts) | **No** |
| llms.txt / llms-full.txt URLs | Yes (displayed) | **No** |
| Developer app selector for config | Yes | **No** (no developer app commands at all) |

### Other Frontend Pages Not Covered

| Page/Feature | CLI Status |
|---|---|
| Notifications settings (`/approvals/settings`) | Missing |
| Approval history (`/approvals/history`) | Missing |
| Nodes detail page with bindings | Missing (only list, show, register-token) |
| SSH terminal page (`/ssh/:serviceId/terminal`) | Missing |
| Developer apps page (`/developer/apps`) | Missing entirely |
| Provider management | Missing entirely |

---

## Section 3: Implementation Spec for Gaps

### CRITICAL Gaps

#### C11: SSH Service Creation

**Command:** `nyxid service add --custom-ssh` or `nyxid service add <slug>` where slug is SSH-type

**API:** `POST /api/v1/keys`

**Parameters needed:**
- `--ssh-host` (required for SSH)
- `--ssh-port` (default: 22)
- `--ssh-certificate-auth` (bool, default true)
- `--ssh-principals` (comma-separated)
- `--ssh-certificate-ttl-minutes` (default: 30)
- `--via-node` (required -- SSH must route through node)

**Body:**
```json
{
  "service_type": "ssh",
  "label": "...",
  "ssh_host": "...",
  "ssh_port": 22,
  "ssh_certificate_auth": true,
  "ssh_principals": ["deploy", "ubuntu"],
  "ssh_certificate_ttl_minutes": 30,
  "node_id": "..."
}
```

**Notes:** Frontend enforces SSH-requires-node. CLI should too. After creation, print SSH setup instructions (CA public key, sshd_config lines).

---

#### C9/C10: Proxy Command

**Command:** `nyxid proxy <slug-or-id> [method] [path] [--data <body>] [--header <k:v>]`

**API:** `ANY /api/v1/proxy/s/{slug}/{path}` or `ANY /api/v1/proxy/{service_id}/{path}`

**Parameters:**
- Positional: slug or service_id, optional path
- `--method` (GET, POST, etc., default GET)
- `--data` or `-d` (request body, reads from stdin if `-`)
- `--header` / `-H` (repeatable)
- `--stream` (pass through SSE/streaming responses)

**Notes:** This is the most impactful missing feature. Users should be able to `nyxid proxy openai v1/chat/completions -d @body.json` to test proxy routing from terminal.

---

#### C12: External Credential Rotation

**Command:** `nyxid service rotate-credential <service-id>`

**API:** `PUT /api/v1/api-keys/external/{key_id}` (needs to resolve `api_key_id` from service detail)

**Flow:**
1. GET `/keys/{id}` to get `api_key_id`
2. Prompt for new credential (rpassword)
3. PUT `/api-keys/external/{api_key_id}` with `{ "credential": "..." }`

---

#### C13: Service Activate/Deactivate

**Command:** `nyxid service update <id> --active` / `--inactive`

**API:** `PUT /api/v1/user-services/{service_id}` with `{ "is_active": true/false }`

**Notes:** Add `--active` and `--inactive` flags to existing `ServiceCommands::Update`. Resolve `user_service_id` from service detail response.

---

#### C8: List Proxyable Services

**Command:** `nyxid proxy list` or `nyxid service proxy-list`

**API:** `GET /api/v1/proxy/services`

**Notes:** Shows all services available for proxying with their slug, status, and proxy URL.

---

### IMPORTANT Gaps

#### I4-I9: Node Management

**Commands to add:**
- `nyxid node delete <id> [--yes]` -- `DELETE /api/v1/nodes/{node_id}`
- `nyxid node rotate-token <id>` -- `POST /api/v1/nodes/{node_id}/rotate-token`
- `nyxid node bindings <id>` -- `GET /api/v1/nodes/{node_id}/bindings`
- `nyxid node bind <node_id> --service <service_id>` -- `POST /api/v1/nodes/{node_id}/bindings`
- `nyxid node unbind <node_id> --binding <binding_id>` -- `DELETE /api/v1/nodes/{node_id}/bindings/{binding_id}`

---

#### I11-I14: Notifications

**Commands to add:**
- `nyxid notifications settings` -- `GET /api/v1/notifications/settings`
- `nyxid notifications update --email true --telegram false` -- `PUT /api/v1/notifications/settings`
- `nyxid notifications telegram-link` -- `POST /api/v1/notifications/telegram/link`
- `nyxid notifications telegram-disconnect` -- `DELETE /api/v1/notifications/telegram`

---

#### I15-I20: Approvals

**Commands to add:**
- `nyxid approvals list` -- `GET /api/v1/approvals/requests`
- `nyxid approvals show <id>` -- `GET /api/v1/approvals/requests/{id}`
- `nyxid approvals approve <id>` -- `POST /api/v1/approvals/requests/{id}/decide` with `{ "decision": "approved" }`
- `nyxid approvals deny <id> [--reason <msg>]` -- same with `{ "decision": "denied" }`
- `nyxid approvals grants` -- `GET /api/v1/approvals/grants`
- `nyxid approvals revoke-grant <id>` -- `DELETE /api/v1/approvals/grants/{id}`

---

#### I21-I22: OAuth and Device Code Flows

**Command:** `nyxid service add <slug> --oauth` or auto-detect from catalog entry

**OAuth flow:**
1. Fetch catalog entry, check `provider_type === "oauth2"`
2. Create key via `POST /keys` with `service_slug`
3. If response indicates OAuth needed, open browser to OAuth URL
4. Wait for callback or prompt user to confirm completion

**Device code flow:**
1. Detect `provider_type === "device_code"` from catalog
2. `POST /providers/{provider_id}/connect/device-code/initiate`
3. Display user code and verification URL
4. Poll `POST /providers/{provider_id}/connect/device-code/poll` until success
5. Complete key creation

**Notes:** The frontend's `add-key-dialog.tsx` handles both flows. CLI should match.

---

#### I23: Provider Credentials

**Command:** `nyxid service set-credentials <service-id> --client-id <id> --client-secret <secret>`

**API:** `PUT /api/v1/providers/{provider_id}/credentials`

**Notes:** For services using OAuth that require user-provided OAuth client credentials.

---

#### I27-I28: SSH Exec and Terminal

**Commands to add:**
- `nyxid ssh exec --service-id <id> --principal <user> -- <command>` -- `POST /api/v1/ssh/{service_id}/exec`
- `nyxid ssh terminal --service-id <id> --principal <user>` -- `GET /api/v1/ssh/{service_id}/terminal` (WebSocket)

**Notes:** `ssh exec` sends a command and returns output. `ssh terminal` opens an interactive PTY session over WebSocket (similar to `ssh proxy` but for the web terminal protocol).

---

#### C1-C4: Auth Flows

**Commands to add:**
- `nyxid register --email <email> --password <password> --name <name>` -- `POST /api/v1/auth/register`
- `nyxid verify-email --token <token>` -- `POST /api/v1/auth/verify-email`
- `nyxid forgot-password --email <email>` -- `POST /api/v1/auth/forgot-password`
- `nyxid reset-password --token <token> --password <new>` -- `POST /api/v1/auth/reset-password`

---

#### C5: User Profile Update

**Command:** `nyxid profile update --name <name>`

**API:** `PUT /api/v1/users/me`

---

#### C6: MFA

**Commands to add:**
- `nyxid mfa setup` -- `POST /api/v1/auth/mfa/setup` (returns QR code / TOTP secret)
- `nyxid mfa confirm --code <code>` -- `POST /api/v1/auth/mfa/confirm`
- `nyxid mfa verify --code <code>` -- `POST /api/v1/auth/mfa/verify`
- `nyxid mfa disable --code <code>` -- `POST /api/v1/auth/mfa/disable`

---

#### C7: Sessions

**Command:** `nyxid sessions list`

**API:** `GET /api/v1/sessions`

---

## Summary

**Total CRITICAL gaps:** 13 (most impactful: proxy command, SSH service creation, credential rotation)
**Total IMPORTANT gaps:** 28 (most impactful: node management, approvals, OAuth/device-code flows)
**Total NICE-TO-HAVE gaps:** 15 (admin, developer apps, legacy endpoints)

### Priority Order for Implementation

1. **`nyxid proxy`** -- most impactful for developer workflow
2. **SSH service creation** -- frontend supports it, CLI doesn't
3. **External credential rotation** -- day-1 operational need
4. **Service activate/deactivate** -- basic service management
5. **Node management** (delete, rotate-token, bindings) -- operational need
6. **OAuth/device-code flows** -- required for many catalog services
7. **Approvals** -- important for production use
8. **SSH exec/terminal** -- extends existing SSH support
9. **Auth flows** (register, verify-email, forgot-password) -- onboarding
10. **MFA, sessions, notifications** -- account management
