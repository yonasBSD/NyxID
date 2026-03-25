> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V6: SSH in AI Services, Route-First UX, Node-Native OAuth

This spec builds on V1-V5. It covers three requirements:

1. **SSH Services in AI Services** -- SSH services addable through the unified AI Services page, routed through nodes
2. **Route-First UX** -- restructure the add-key wizard so routing choice comes before credential input
3. **Node-Native OAuth** -- node agent handles OAuth flows directly, tokens never touch NyxID

---

## Table of Contents

1. [Requirement 1: SSH Services in AI Services](#1-ssh-services-in-ai-services)
2. [Requirement 2: Route-First UX](#2-route-first-ux)
3. [Requirement 3: Node-Native OAuth](#3-node-native-oauth)
4. [Catalog API Changes](#4-catalog-api-changes)
5. [Complete File List](#5-complete-file-list)

---

## 1. SSH Services in AI Services

### 1.1 Problem

SSH services exist as `service_type: "ssh"` in `DownstreamService` with `SshServiceConfig` (host, port, CA key, principals, certificate TTL). They are currently admin-managed through the old services system and require node agents for execution (ssh_exec, ssh_tunnel, ssh_web_terminal). However, they are excluded from the AI Services catalog (`catalog_service.rs` filters to `service_type: "http"` only) and cannot be added through the unified `/keys` flow.

### 1.2 Design

SSH services join the catalog but with key constraints:

- SSH services **must** be node-routed. The NyxID backend does not SSH directly -- it delegates to node agents. The wizard enforces this.
- SSH services have **no credential** on NyxID. The node handles SSH connections using ephemeral certificates issued by NyxID's CA. The `UserApiKey` record has `credential_type: "ssh_certificate"` and no `credential_encrypted`.
- Connection info (CA public key, principals, `sshd_config` setup) comes from the `SshServiceConfig` on the `DownstreamService` and is displayed on the key detail page.

### 1.3 Backend Changes

#### 1.3.1 `backend/src/services/catalog_service.rs`

**Change catalog query** to include SSH services:

```rust
// Before:
"service_type": "http",

// After:
"service_type": { "$in": ["http", "ssh"] },
```

**Add SSH fields to `CatalogEntry`:**

```rust
pub struct CatalogEntry {
    // ... existing fields ...
    pub service_type: String,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_ca_public_key: Option<String>,
    pub ssh_allowed_principals: Option<Vec<String>>,
    pub ssh_certificate_ttl_minutes: Option<u32>,
}
```

**Populate SSH fields** from `DownstreamService.ssh_config`:

```rust
CatalogEntry {
    // ... existing ...
    service_type: svc.service_type.clone(),
    ssh_host: svc.ssh_config.as_ref().map(|c| c.host.clone()),
    ssh_port: svc.ssh_config.as_ref().map(|c| c.port),
    ssh_ca_public_key: svc.ssh_config.as_ref().and_then(|c| c.ca_public_key.clone()),
    ssh_allowed_principals: svc.ssh_config.as_ref().map(|c| c.allowed_principals.clone()),
    ssh_certificate_ttl_minutes: svc.ssh_config.as_ref().map(|c| c.certificate_ttl_minutes),
}
```

#### 1.3.2 `backend/src/handlers/catalog.rs`

**Add SSH fields to `CatalogEntryResponse`:**

```rust
pub struct CatalogEntryResponse {
    // ... existing fields ...
    pub service_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_ca_public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_allowed_principals: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_certificate_ttl_minutes: Option<u32>,
}
```

#### 1.3.3 `backend/src/services/unified_key_service.rs`

**Handle SSH service creation** in the catalog path of `create_key`:

- When `service_type == "ssh"`: require `node_id` (return `BadRequest` if missing), set `credential_type = "ssh_certificate"`, skip credential encryption, set `UserApiKey.status = "active"` with no `credential_encrypted`.
- Endpoint URL: use `ssh://{host}:{port}` from `SshServiceConfig`.

```rust
// Inside catalog path of create_key:
let is_ssh = svc.service_type == "ssh";

if is_ssh && node_id.is_none() {
    return Err(AppError::BadRequest(
        "SSH services must be routed through a node agent".to_string(),
    ));
}

let credential_type = if is_ssh {
    "ssh_certificate".to_string()
} else if let Some(ref pid) = svc.provider_config_id {
    // ... existing provider type logic ...
} else {
    // ... existing auth_type logic ...
};
```

**Add `service_type` to `KeyView`:**

```rust
pub struct KeyView {
    // ... existing fields ...
    pub service_type: String,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_ca_public_key: Option<String>,
    pub ssh_allowed_principals: Option<Vec<String>>,
    pub ssh_certificate_ttl_minutes: Option<u32>,
}
```

In `build_key_view`, look up `DownstreamService.ssh_config` when `catalog_service_id` is present and `service_type == "ssh"`.

#### 1.3.4 `backend/src/models/user_service.rs`

**Add `service_type` field:**

```rust
pub struct UserService {
    // ... existing fields ...
    /// "http" (default) | "ssh"
    #[serde(default = "default_service_type")]
    pub service_type: String,
}

fn default_service_type() -> String {
    "http".to_string()
}
```

#### 1.3.5 `backend/src/services/user_service_service.rs`

**Add `service_type` parameter** to `create_user_service`:

```rust
pub async fn create_user_service(
    db: &mongodb::Database,
    user_id: &str,
    slug: &str,
    endpoint_id: &str,
    api_key_id: &str,
    auth_method: &str,
    auth_key_name: &str,
    catalog_service_id: Option<&str>,
    node_id: Option<&str>,
    node_priority: i32,
    service_type: &str,  // NEW
) -> AppResult<UserService>
```

#### 1.3.6 `backend/src/services/user_api_key_service.rs`

**Handle no-credential creation** for SSH and node-managed services:

When `credential` is empty string AND `credential_type` is `"ssh_certificate"` or `"node_managed"`, create the `UserApiKey` with `credential_encrypted: None`, `status: "active"`.

```rust
pub async fn create_api_key(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    label: &str,
    credential_type: &str,
    credential: &str,
    provider_config_id: Option<&str>,
) -> AppResult<UserApiKey> {
    let credential_encrypted = if credential.is_empty() {
        None
    } else {
        Some(encryption_keys.encrypt(credential.as_bytes()).await?)
    };
    // ... rest unchanged ...
}
```

### 1.4 Frontend Changes

#### 1.4.1 `frontend/src/types/keys.ts`

**Add SSH fields to `CatalogEntry`:**

```typescript
export interface CatalogEntry {
  // ... existing fields ...
  readonly service_type: string;
  readonly ssh_host: string | null;
  readonly ssh_port: number | null;
  readonly ssh_ca_public_key: string | null;
  readonly ssh_allowed_principals: readonly string[] | null;
  readonly ssh_certificate_ttl_minutes: number | null;
}
```

**Add SSH fields to `KeyInfo`:**

```typescript
export interface KeyInfo {
  // ... existing fields ...
  readonly service_type: string;
  readonly ssh_host: string | null;
  readonly ssh_port: number | null;
  readonly ssh_ca_public_key: string | null;
  readonly ssh_allowed_principals: readonly string[] | null;
  readonly ssh_certificate_ttl_minutes: number | null;
}
```

#### 1.4.2 `frontend/src/components/dashboard/add-key-dialog.tsx`

**SSH catalog entries** show a "SSH" badge in the catalog grid:

```typescript
{entry.service_type === "ssh" && (
  <Badge variant="secondary" className="text-[10px]">SSH</Badge>
)}
```

When an SSH catalog entry is selected, the wizard flows:
`catalog` -> `routing` (forced node selection) -> submit.

No credential step. No endpoint URL step. See [Requirement 2](#2-route-first-ux) for full flow.

#### 1.4.3 `frontend/src/pages/key-detail.tsx`

**Add SSH connection info section** when `key.service_type === "ssh"`:

Display:
- SSH Host: `key.ssh_host`:`key.ssh_port`
- CA Public Key (copyable)
- Allowed Principals
- Certificate TTL
- Setup instructions for the target machine:
  1. Add CA public key to `/etc/ssh/trusted-user-ca-keys.pem`
  2. Add `TrustedUserCAKeys /etc/ssh/trusted-user-ca-keys.pem` to `/etc/ssh/sshd_config`
  3. Restart sshd
- Node setup instructions for the node agent:
  1. `nyxid-node credentials add --service <slug> --header Authorization --secret-format bearer`
  2. Configure SSH allowed targets in node config

### 1.5 Node Agent Changes

No node agent code changes needed for SSH in AI Services. The existing SSH exec/tunnel infrastructure in the node agent already handles SSH connections through nodes. The `ssh_exec` handler and `ssh_tunnel` handler resolve node routes via `node_routing_service` and delegate to the connected node.

The `UserService.node_id` field binds the SSH service to a specific node, and the existing proxy infrastructure handles routing.

---

## 2. Route-First UX

### 2.1 Problem

The current wizard flow is:

| Service Type | Current Steps |
|---|---|
| API key (catalog) | `catalog` -> `form` (credential) -> `routing` -> submit |
| API key (custom) | `catalog` -> `form` (credential) -> `routing` -> submit |
| OAuth | `catalog` -> `oauth` -> close |
| Device code | `catalog` -> `device_code` -> close |

The credential step comes before the routing step. This is wrong because:

1. When routing via node, there are **no credentials to enter on NyxID**. Credentials live on the node.
2. OAuth/device-code flows have no routing step at all, making it impossible to route them via node.
3. SSH services have no credential step and must be node-routed.

### 2.2 Design

New wizard flow -- routing comes immediately after catalog selection:

| Service Type | New Steps |
|---|---|
| API key (catalog, direct) | `catalog` -> `routing` -> `form` (credential) -> submit |
| API key (catalog, via node) | `catalog` -> `routing` -> `node_setup` -> submit |
| API key (custom, direct) | `catalog` -> `routing` -> `form` (credential) -> submit |
| API key (custom, via node) | `catalog` -> `routing` -> `node_setup` -> submit |
| OAuth (direct) | `catalog` -> `routing` -> `oauth_credentials`? -> `oauth` -> close |
| OAuth (via node) | `catalog` -> `routing` -> `node_setup` -> submit |
| Device code (direct) | `catalog` -> `routing` -> `device_code` -> close |
| Device code (via node) | `catalog` -> `routing` -> `node_setup` -> submit |
| SSH (always via node) | `catalog` -> `routing` (node forced) -> `node_setup` -> submit |

Key principle: **choosing "via node" always skips the credential step** and shows the node setup helper instead.

### 2.3 New WizardStep Type

```typescript
type WizardStep =
  | "catalog"
  | "routing"        // NEW: moved before credential input
  | "form"           // credential input (direct routing only)
  | "node_setup"     // NEW: node setup helper (via node routing only)
  | "oauth_credentials"
  | "oauth"
  | "device_code";
```

### 2.4 Frontend Changes

#### 2.4.1 `frontend/src/components/dashboard/add-key-dialog.tsx`

**Revised `handleSelectCatalog`:**

After selecting a catalog entry, always go to routing step first (except when routing choice is predetermined):

```typescript
function handleSelectCatalog(entry: CatalogEntry) {
  setSelectedEntry(entry);
  setForm({
    ...INITIAL_FORM,
    label: entry.name,
    endpointUrl: entry.base_url,
    authMethod: entry.auth_method ?? "bearer",
    authKeyName: entry.auth_key_name ?? "Authorization",
  });

  // SSH services: force node routing, skip routing choice
  if (entry.service_type === "ssh") {
    setStep("routing");  // routing step will show node-only UI
    return;
  }

  // All other services: go to routing step
  setStep("routing");
}

function handleSelectCustom() {
  setSelectedEntry(null);
  setForm(INITIAL_FORM);
  setStep("routing");
}
```

**Revised `RoutingStep` component:**

The routing step now determines the entire downstream flow:

```typescript
function RoutingStep({
  catalogEntry,
  form,
  onChange,
  onDirect,    // proceed to credential input / OAuth
  onViaNode,   // proceed to node setup (skip credentials)
  isSshOnly,   // SSH services force node routing
}: {
  readonly catalogEntry: CatalogEntry | null;
  readonly form: FormState;
  readonly onChange: (updates: Partial<FormState>) => void;
  readonly onDirect: () => void;
  readonly onViaNode: () => void;
  readonly isSshOnly: boolean;
}) {
  const { data: nodes, isLoading } = useNodes();
  const onlineNodes = nodes?.filter((n) => n.status === "online") ?? [];
  const isCustom = catalogEntry === null;
  const [routingChoice, setRoutingChoice] = useState<"direct" | "node">(
    isSshOnly ? "node" : "direct"
  );

  function handleNext() {
    if (routingChoice === "node" && !form.nodeId) return;
    if (routingChoice === "node") {
      onViaNode();
    } else {
      onDirect();
    }
  }

  return (
    <div className="space-y-4">
      {/* Back button */}
      {/* Catalog entry info card */}

      <div className="space-y-3">
        <Label>How should requests reach this service?</Label>

        {/* Radio group: Direct vs Via Node */}
        {!isSshOnly && (
          <RadioGroup value={routingChoice} onValueChange={setRoutingChoice}>
            <RadioGroupItem value="direct">
              Route directly (NyxID to endpoint)
            </RadioGroupItem>
            <RadioGroupItem value="node">
              Route via credential node
            </RadioGroupItem>
          </RadioGroup>
        )}

        {isSshOnly && (
          <p className="text-sm text-muted-foreground">
            SSH services must be routed through a credential node.
          </p>
        )}

        {/* Node selector (shown when "node" is selected) */}
        {routingChoice === "node" && (
          <Select
            value={form.nodeId}
            onValueChange={(v) => onChange({ nodeId: v })}
          >
            {/* ... node list ... */}
          </Select>
        )}
      </div>

      <Button onClick={handleNext} disabled={routingChoice === "node" && !form.nodeId}>
        {routingChoice === "node" ? "Next: Node Setup" : "Next: Enter Credentials"}
      </Button>
    </div>
  );
}
```

**New `NodeSetupStep` component:**

Shown when user selects "via node" routing. Displays setup instructions and submits without credentials.

```typescript
function NodeSetupStep({
  catalogEntry,
  form,
  onSubmit,
  onBack,
  isPending,
}: {
  readonly catalogEntry: CatalogEntry | null;
  readonly form: FormState;
  readonly onSubmit: () => void;
  readonly onBack: () => void;
  readonly isPending: boolean;
}) {
  const slug = catalogEntry?.slug ?? form.slug;
  const isSsh = catalogEntry?.service_type === "ssh";
  const isOAuth = catalogEntry?.provider_type === "oauth2" || catalogEntry?.provider_type === "device_code";

  return (
    <div className="space-y-4">
      <button type="button" onClick={onBack} className="...">
        <ArrowLeft /> Back
      </button>

      <div className="rounded-lg border bg-muted/50 p-4 space-y-3">
        <p className="text-sm font-medium">Node Setup Instructions</p>

        {isSsh ? (
          <>
            <p className="text-xs text-muted-foreground">
              Configure your node agent to allow SSH connections to this target.
              Credentials are managed via NyxID SSH certificates.
            </p>
            <div className="space-y-2">
              <p className="text-xs font-medium">1. Allow the SSH target in node config:</p>
              <CopyableCode>
                {`[ssh]\nallowed_targets = [{ host = "${catalogEntry?.ssh_host ?? "host"}", port = ${catalogEntry?.ssh_port ?? 22} }]`}
              </CopyableCode>
              <p className="text-xs font-medium">2. On the target machine, trust NyxID CA:</p>
              <CopyableCode>
                {`echo '${catalogEntry?.ssh_ca_public_key ?? "ssh-ed25519 AAAA..."}' >> /etc/ssh/trusted-user-ca-keys.pem`}
              </CopyableCode>
              <p className="text-xs font-medium">3. Add to /etc/ssh/sshd_config:</p>
              <CopyableCode>
                TrustedUserCAKeys /etc/ssh/trusted-user-ca-keys.pem
              </CopyableCode>
            </div>
          </>
        ) : isOAuth ? (
          <>
            <p className="text-xs text-muted-foreground">
              The node handles OAuth authentication directly. Run this command on your node:
            </p>
            <CopyableCode>
              {`nyxid-node credentials add-oauth --service ${slug} --from-catalog`}
            </CopyableCode>
            <p className="text-xs text-muted-foreground">
              The node will fetch OAuth configuration from the catalog and guide you through the authorization flow.
              Tokens are stored locally on the node and never touch NyxID.
            </p>
          </>
        ) : (
          <>
            <p className="text-xs text-muted-foreground">
              Add the credential on your node agent. The credential stays on the node and is never sent to NyxID.
            </p>
            <CopyableCode>
              {`nyxid-node credentials add --service ${slug} --header Authorization --secret-format bearer`}
            </CopyableCode>
            {catalogEntry?.api_key_url && (
              <a href={catalogEntry.api_key_url} target="_blank" rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-xs text-primary hover:underline">
                Get API key <ExternalLink className="h-3 w-3" />
              </a>
            )}
          </>
        )}
      </div>

      <Button className="w-full" onClick={onSubmit} disabled={isPending}>
        {isPending ? "Creating..." : "Create Service"}
      </Button>
    </div>
  );
}
```

**Revised wizard orchestration in `AddKeyDialog`:**

```typescript
function handleRoutingDirect() {
  // "Direct" was chosen -- proceed to credential step
  if (!selectedEntry) {
    setStep("form");  // custom endpoint: need credential + URL + slug
    return;
  }

  const needsUserCreds =
    selectedEntry.credential_mode === "user" || selectedEntry.credential_mode === "both";

  if (selectedEntry.provider_type === "oauth2" && selectedEntry.provider_config_id) {
    setStep(needsUserCreds ? "oauth_credentials" : "oauth");
    return;
  }

  if (selectedEntry.provider_type === "device_code" && selectedEntry.provider_config_id) {
    setStep(needsUserCreds ? "oauth_credentials" : "device_code");
    return;
  }

  setStep("form");  // API key credential input
}

function handleRoutingViaNode() {
  // "Via Node" was chosen -- skip credentials, show node setup
  setStep("node_setup");
}

function handleNodeSetupSubmit() {
  // Submit with no credential for node-routed services
  const params = selectedEntry
    ? {
        credential: "",  // empty -- node manages credentials
        label: form.label,
        service_slug: selectedEntry.slug,
        node_id: form.nodeId,
        service_type: selectedEntry.service_type,
      }
    : {
        credential: "",
        label: form.label,
        endpoint_url: form.endpointUrl.trim() || undefined,
        slug: form.slug.trim(),
        auth_method: form.authMethod,
        auth_key_name: form.authKeyName,
        node_id: form.nodeId,
      };

  createKey.mutate(params, {
    onSuccess: (key) => {
      toast.success("Service created");
      handleOpenChange(false);
      void navigate({ to: "/keys/$keyId", params: { keyId: key.id } });
    },
    onError: (err) => {
      const message = err instanceof ApiError ? err.message : "Failed to create service";
      toast.error(message);
    },
  });
}
```

**Updated wizard rendering:**

```typescript
{step === "routing" && (
  <RoutingStep
    catalogEntry={selectedEntry}
    form={form}
    onChange={handleFormChange}
    onDirect={handleRoutingDirect}
    onViaNode={handleRoutingViaNode}
    isSshOnly={selectedEntry?.service_type === "ssh"}
  />
)}

{step === "node_setup" && (
  <NodeSetupStep
    catalogEntry={selectedEntry}
    form={form}
    onSubmit={handleNodeSetupSubmit}
    onBack={() => setStep("routing")}
    isPending={createKey.isPending}
  />
)}

{step === "form" && (
  <KeyForm
    catalogEntry={selectedEntry}
    form={form}
    onChange={handleFormChange}
    onSubmit={handleFormSubmit}
    onBack={() => setStep("routing")}  // Back goes to routing now
  />
)}
```

**`KeyForm` changes:**

- The submit button now submits directly (no routing step after). The form step is only reached for direct routing.
- The button label changes from "Next: Configure Routing" to "Create Service".
- Include `node_id: undefined` in the submit (direct routing).

### 2.5 Backend Changes

#### 2.5.1 `backend/src/handlers/keys.rs`

**`CreateKeyRequest` -- make credential optional:**

```rust
#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub service_slug: Option<String>,
    pub credential: Option<String>,  // Changed from String to Option<String>
    pub label: String,
    pub endpoint_url: Option<String>,
    pub slug: Option<String>,
    pub auth_method: Option<String>,
    pub auth_key_name: Option<String>,
    pub node_id: Option<String>,
}
```

In `create_key` handler, pass empty string when credential is None:

```rust
let credential = body.credential.as_deref().unwrap_or("");
```

#### 2.5.2 `backend/src/services/unified_key_service.rs`

**Allow empty credential when node_id is present:**

In both catalog and custom paths, when `credential.is_empty()` and `node_id.is_some()`:
- Set `credential_type = "node_managed"` (unless SSH, which uses `"ssh_certificate"`)
- Create `UserApiKey` with `credential_encrypted: None`, `status: "active"`

```rust
// In catalog path:
let credential_type = if is_ssh {
    "ssh_certificate".to_string()
} else if credential.is_empty() && node_id.is_some() {
    "node_managed".to_string()
} else {
    // ... existing provider type / auth_type logic ...
};

// In custom path:
if credential.is_empty() && node_id.is_none() {
    return Err(AppError::BadRequest(
        "Credential is required for direct routing (or select a node)".to_string(),
    ));
}
```

#### 2.5.3 `backend/src/services/user_api_key_service.rs`

**Handle empty credential gracefully:**

```rust
let credential_encrypted = if credential.is_empty() {
    None
} else {
    Some(encryption_keys.encrypt(credential.as_bytes()).await?)
};
```

#### 2.5.4 `backend/src/handlers/keys.rs`

**Skip credential push for node-managed keys:**

```rust
// Only push credentials to node when we actually have a credential to push
if result.service.node_id.is_some()
    && result.api_key.credential_encrypted.is_some()
{
    // ... existing credential_push_service call ...
}
```

### 2.6 Frontend Hook Changes

#### 2.6.1 `frontend/src/hooks/use-keys.ts`

**Update `CreateKeyParams` to make credential optional:**

```typescript
interface CreateKeyParams {
  readonly service_slug?: string;
  readonly credential?: string;  // Changed: now optional
  readonly label: string;
  readonly endpoint_url?: string;
  readonly slug?: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
  readonly node_id?: string;
  readonly service_type?: string;
}
```

---

## 3. Node-Native OAuth

### 3.1 Problem

Currently, OAuth flows run through NyxID:
1. User initiates OAuth in the browser
2. NyxID handles the redirect/device-code exchange
3. NyxID stores the tokens (encrypted in `UserApiKey`)
4. For node-routed services, `credential_push_service` pushes decrypted tokens to the node via WebSocket

This has several issues:
- Tokens transit through NyxID even when the node is the only consumer
- OAuth refresh requires NyxID to be online
- The credential push is fragile (node must be online at push time)

### 3.2 Design

The node agent handles OAuth flows directly:

1. User runs `nyxid-node credentials add-oauth --service <slug>` on the node
2. Node fetches OAuth config from NyxID catalog API (`GET /api/v1/catalog/{slug}`)
3. Node runs device code flow (or starts local HTTP server for authorization_code)
4. User authorizes in browser
5. Tokens stored locally on node (encrypted), never sent to NyxID
6. Background task on the node refreshes tokens before expiry

The `credential_push_service` from V5 becomes **optional** -- it's still used when:
- User chooses "direct" routing and later changes to node routing
- User has existing NyxID-managed credentials they want to migrate to a node

For "via node" routing in the wizard, credentials never touch NyxID.

### 3.3 Catalog API Changes (Expose OAuth Config)

The node needs OAuth URLs to run the flow locally. The catalog API already returns `provider_type` and `provider_config_id`, but not the actual OAuth URLs.

#### 3.3.1 `backend/src/services/catalog_service.rs`

**Add OAuth fields to `CatalogEntry`:**

```rust
pub struct CatalogEntry {
    // ... existing fields ...
    // OAuth config (for node-native OAuth)
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub device_code_url: Option<String>,
    pub device_verification_url: Option<String>,
    pub device_token_url: Option<String>,
    pub default_scopes: Option<Vec<String>>,
    pub supports_pkce: bool,
    pub device_code_format: Option<String>,
    pub token_endpoint_auth_method: Option<String>,
    pub extra_auth_params: Option<HashMap<String, String>>,
}
```

**Populate from `ProviderConfig`:**

```rust
CatalogEntry {
    // ... existing ...
    authorization_url: provider.and_then(|p| p.authorization_url.clone()),
    token_url: provider.and_then(|p| p.token_url.clone()),
    device_code_url: provider.and_then(|p| p.device_code_url.clone()),
    device_verification_url: provider.and_then(|p| p.device_verification_url.clone()),
    device_token_url: provider.and_then(|p| p.device_token_url.clone()),
    default_scopes: provider.and_then(|p| p.default_scopes.clone()),
    supports_pkce: provider.map_or(false, |p| p.supports_pkce),
    device_code_format: provider.map(|p| p.device_code_format.clone()),
    token_endpoint_auth_method: provider.map(|p| p.token_endpoint_auth_method.clone()),
    extra_auth_params: provider.and_then(|p| p.extra_auth_params.clone()),
}
```

**Important:** Do NOT expose `client_id_encrypted` or `client_secret_encrypted` from the catalog. These are NyxID's OAuth app credentials. For node-native OAuth, the user provides their own client_id/secret.

#### 3.3.2 `backend/src/handlers/catalog.rs`

**Add OAuth fields to `CatalogEntryResponse`:**

```rust
pub struct CatalogEntryResponse {
    // ... existing fields ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_token_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_pkce: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<HashMap<String, String>>,
}
```

### 3.4 Node Agent Changes

#### 3.4.1 `node-agent/src/cli.rs`

**Add `AddOAuth` subcommand to `CredentialCommands`:**

```rust
#[derive(Subcommand)]
pub enum CredentialCommands {
    // ... existing Add, List, Remove ...

    /// Add an OAuth credential for a service (runs device code or authorization code flow)
    AddOauth {
        /// Service slug (e.g., "api-twitter", "llm-openai")
        #[arg(long)]
        service: String,

        /// Fetch OAuth config from NyxID catalog (requires --api-url or server config)
        #[arg(long)]
        from_catalog: bool,

        /// OAuth client ID (your own app's client ID)
        #[arg(long)]
        client_id: Option<String>,

        /// OAuth client secret (your own app's client secret, prompted if not provided)
        #[arg(long)]
        client_secret: Option<String>,

        /// OAuth authorization URL (not needed with --from-catalog)
        #[arg(long)]
        authorization_url: Option<String>,

        /// OAuth token URL (not needed with --from-catalog)
        #[arg(long)]
        token_url: Option<String>,

        /// Device code URL (for device code flow, not needed with --from-catalog)
        #[arg(long)]
        device_code_url: Option<String>,

        /// Scopes to request (space-separated)
        #[arg(long)]
        scopes: Option<String>,

        /// Target URL for this service
        #[arg(long)]
        url: Option<String>,

        /// NyxID API base URL (defaults to server URL from config)
        #[arg(long)]
        api_url: Option<String>,

        /// NyxID access token (defaults to NYXID_ACCESS_TOKEN env var)
        #[arg(long)]
        access_token: Option<String>,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },
}
```

#### 3.4.2 `node-agent/src/config.rs`

**Add OAuth fields to `CredentialConfig`:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialConfig {
    // ... existing fields (injection_method, target_url, header_name, etc.) ...

    /// OAuth-managed credential: token refresh handled automatically
    #[serde(default)]
    pub oauth_managed: bool,

    /// OAuth token URL (for refresh)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_token_url: Option<String>,

    /// AES-GCM encrypted OAuth access token (base64)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_access_token_encrypted: Option<String>,

    /// AES-GCM encrypted OAuth refresh token (base64)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_refresh_token_encrypted: Option<String>,

    /// Token expiry time (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_token_expires_at: Option<String>,

    /// AES-GCM encrypted OAuth client ID (base64)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_id_encrypted: Option<String>,

    /// AES-GCM encrypted OAuth client secret (base64)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_client_secret_encrypted: Option<String>,

    /// OAuth scopes (space-separated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_scopes: Option<String>,

    /// Token endpoint auth method: "client_secret_post" | "client_secret_basic"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_token_endpoint_auth_method: Option<String>,
}
```

Example config.toml after `add-oauth`:

```toml
[credentials.api-twitter]
injection_method = "header"
header_name = "Authorization"
# header_value_encrypted is populated by the refresh mechanism
header_value_encrypted = "base64..."
target_url = "https://api.twitter.com/2"
oauth_managed = true
oauth_token_url = "https://api.twitter.com/2/oauth2/token"
oauth_access_token_encrypted = "base64..."
oauth_refresh_token_encrypted = "base64..."
oauth_token_expires_at = "2026-03-24T17:00:00Z"
oauth_client_id_encrypted = "base64..."
oauth_client_secret_encrypted = "base64..."
oauth_scopes = "tweet.read users.read"
oauth_token_endpoint_auth_method = "client_secret_post"
```

#### 3.4.3 `node-agent/src/oauth.rs` (NEW FILE)

New module handling the OAuth flows on the node:

```rust
//! Node-native OAuth flow: device code and authorization code.
//!
//! Fetches OAuth config from NyxID catalog or uses CLI-provided URLs.
//! Runs the flow, stores tokens locally, never sends them to NyxID.

use serde::{Deserialize, Serialize};

use crate::config::CredentialConfig;
use crate::error::{Error, Result};

/// OAuth config fetched from NyxID catalog or provided via CLI.
pub struct OAuthConfig {
    pub authorization_url: Option<String>,
    pub token_url: String,
    pub device_code_url: Option<String>,
    pub device_verification_url: Option<String>,
    pub device_token_url: Option<String>,
    pub default_scopes: Vec<String>,
    pub supports_pkce: bool,
    pub device_code_format: String,  // "rfc8628" | "openai"
    pub token_endpoint_auth_method: String,
    pub extra_auth_params: Option<HashMap<String, String>>,
}

/// Token response from OAuth token endpoint.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// Device code response.
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Fetch OAuth config from NyxID catalog API.
pub async fn fetch_catalog_oauth_config(
    api_base_url: &str,
    access_token: Option<&str>,
    service_slug: &str,
) -> Result<OAuthConfig> {
    let client = reqwest::Client::new();
    let url = format!("{api_base_url}/api/v1/catalog/{service_slug}");

    let mut req = client.get(&url);
    if let Some(token) = access_token {
        req = req.bearer_auth(token);
    }

    let resp = req.send().await
        .map_err(|e| Error::Config(format!("Failed to fetch catalog: {e}")))?;

    if !resp.status().is_success() {
        return Err(Error::Config(format!(
            "Catalog returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| Error::Config(format!("Failed to parse catalog response: {e}")))?;

    let token_url = body["token_url"].as_str()
        .ok_or_else(|| Error::Config("Catalog entry has no token_url".to_string()))?
        .to_string();

    Ok(OAuthConfig {
        authorization_url: body["authorization_url"].as_str().map(String::from),
        token_url,
        device_code_url: body["device_code_url"].as_str().map(String::from),
        device_verification_url: body["device_verification_url"].as_str().map(String::from),
        device_token_url: body["device_token_url"].as_str().map(String::from),
        default_scopes: body["default_scopes"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        supports_pkce: body["supports_pkce"].as_bool().unwrap_or(false),
        device_code_format: body["device_code_format"]
            .as_str()
            .unwrap_or("rfc8628")
            .to_string(),
        token_endpoint_auth_method: body["token_endpoint_auth_method"]
            .as_str()
            .unwrap_or("client_secret_post")
            .to_string(),
        extra_auth_params: None,  // Not exposed for node use
    })
}

/// Run RFC 8628 device code flow.
pub async fn run_device_code_flow(
    config: &OAuthConfig,
    client_id: &str,
    client_secret: Option<&str>,
    scopes: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let device_code_url = config.device_code_url.as_deref()
        .or(config.token_url.strip_suffix("/token").map(|base| {
            // Guess device code URL from token URL
            // This is a fallback; prefer explicit URL from catalog
        }))
        .ok_or_else(|| Error::Config("No device_code_url available".to_string()))?;

    // Step 1: Request device code
    let resp = client
        .post(device_code_url)
        .form(&[
            ("client_id", client_id),
            ("scope", scopes),
        ])
        .send()
        .await
        .map_err(|e| Error::Config(format!("Device code request failed: {e}")))?;

    let device_resp: DeviceCodeResponse = resp.json().await
        .map_err(|e| Error::Config(format!("Failed to parse device code response: {e}")))?;

    // Step 2: Display code to user
    println!();
    println!("  Your code: {}", device_resp.user_code);
    println!("  Visit: {}", device_resp.verification_uri);
    println!();
    println!("  Waiting for authorization...");

    // Step 3: Poll for token
    let token_poll_url = config.device_token_url.as_deref()
        .unwrap_or(&config.token_url);

    let mut interval = std::time::Duration::from_secs(device_resp.interval);
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(device_resp.expires_in);

    loop {
        tokio::time::sleep(interval).await;

        if std::time::Instant::now() > deadline {
            return Err(Error::Config("Device code expired".to_string()));
        }

        let mut form = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &device_resp.device_code),
            ("client_id", client_id),
        ];
        if let Some(secret) = client_secret {
            form.push(("client_secret", secret));
        }

        let resp = client.post(token_poll_url).form(&form).send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let token: TokenResponse = r.json().await
                    .map_err(|e| Error::Config(format!("Failed to parse token: {e}")))?;
                println!("  Authorization successful.");
                return Ok(token);
            }
            Ok(r) if r.status().as_u16() == 428 || r.status().as_u16() == 400 => {
                // authorization_pending or slow_down
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                let error = body["error"].as_str().unwrap_or("authorization_pending");
                match error {
                    "slow_down" => {
                        interval += std::time::Duration::from_secs(5);
                    }
                    "authorization_pending" => {}
                    "expired_token" => {
                        return Err(Error::Config("Device code expired".to_string()));
                    }
                    "access_denied" => {
                        return Err(Error::Config("Authorization denied".to_string()));
                    }
                    other => {
                        return Err(Error::Config(format!("OAuth error: {other}")));
                    }
                }
            }
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                return Err(Error::Config(format!("Token poll error {status}: {text}")));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token poll request failed, retrying");
            }
        }
    }
}

/// Refresh an OAuth token using refresh_token grant.
pub async fn refresh_token(
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
    auth_method: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();

    let mut req = client.post(token_url);

    match auth_method {
        "client_secret_basic" => {
            req = req.basic_auth(client_id, client_secret);
            req = req.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ]);
        }
        _ => {
            // client_secret_post (default)
            let mut form = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
            ];
            if let Some(secret) = client_secret {
                form.push(("client_secret", secret));
            }
            req = req.form(&form);
        }
    }

    let resp = req.send().await
        .map_err(|e| Error::Config(format!("Token refresh failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::Config(format!("Token refresh error {status}: {text}")));
    }

    resp.json().await
        .map_err(|e| Error::Config(format!("Failed to parse refresh response: {e}")))
}
```

#### 3.4.4 `node-agent/src/main.rs`

**Add `cmd_credentials_add_oauth` function:**

Pattern follows `cmd_openclaw_connect`:

```rust
async fn cmd_credentials_add_oauth(
    config_file: &Path,
    config_dir: &Path,
    service: &str,
    from_catalog: bool,
    client_id: Option<String>,
    client_secret: Option<String>,
    authorization_url: Option<String>,
    token_url: Option<String>,
    device_code_url: Option<String>,
    scopes: Option<String>,
    target_url: Option<String>,
    api_url: Option<String>,
    access_token: Option<String>,
) -> Result<()> {
    let mut config = NodeConfig::load(config_file)?;
    let backend = SecretBackend::from_config(&config, config_dir)?;

    // 1. Get OAuth config
    let oauth_config = if from_catalog {
        let base_api_url = api_url.unwrap_or_else(|| {
            config.server.url
                .replace("ws://", "http://")
                .replace("wss://", "https://")
                .replace("/api/v1/nodes/ws", "")
        });
        let token = access_token
            .or_else(|| std::env::var("NYXID_ACCESS_TOKEN").ok())
            .filter(|s| !s.is_empty());

        oauth::fetch_catalog_oauth_config(
            &base_api_url,
            token.as_deref(),
            service,
        ).await?
    } else {
        // Build from CLI args
        let tok_url = token_url.ok_or_else(|| {
            Error::Validation("--token-url is required when not using --from-catalog".to_string())
        })?;
        oauth::OAuthConfig {
            authorization_url,
            token_url: tok_url,
            device_code_url,
            device_verification_url: None,
            device_token_url: None,
            default_scopes: scopes.as_deref()
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
            supports_pkce: false,
            device_code_format: "rfc8628".to_string(),
            token_endpoint_auth_method: "client_secret_post".to_string(),
            extra_auth_params: None,
        }
    };

    // 2. Get client credentials
    let cid = match client_id {
        Some(id) => id,
        None => prompt_secret("OAuth Client ID")?,
    };
    let csecret = match client_secret {
        Some(s) => Some(s),
        None => {
            let s = prompt_secret("OAuth Client Secret (enter to skip for public clients)")?;
            if s.is_empty() { None } else { Some(s) }
        }
    };

    // 3. Determine scopes
    let final_scopes = scopes.unwrap_or_else(|| {
        oauth_config.default_scopes.join(" ")
    });

    // 4. Run the appropriate OAuth flow
    let token_response = if oauth_config.device_code_url.is_some() {
        // Device code flow (preferred for CLI)
        oauth::run_device_code_flow(
            &oauth_config,
            &cid,
            csecret.as_deref(),
            &final_scopes,
        ).await?
    } else if oauth_config.authorization_url.is_some() {
        // Authorization code flow with local HTTP redirect
        oauth::run_authorization_code_flow(
            &oauth_config,
            &cid,
            csecret.as_deref(),
            &final_scopes,
        ).await?
    } else {
        return Err(Error::Validation(
            "No OAuth flow available (need device_code_url or authorization_url)".to_string(),
        ));
    };

    // 5. Store tokens locally
    let header_value = format!(
        "{} {}",
        token_response.token_type.as_deref().unwrap_or("Bearer"),
        token_response.access_token
    );

    let expires_at = token_response.expires_in.map(|secs| {
        (chrono::Utc::now() + chrono::Duration::seconds(secs as i64)).to_rfc3339()
    });

    // Store the header credential (for immediate use by proxy_executor)
    config.add_header_credential_via(
        service,
        "Authorization",
        &header_value,
        target_url.as_deref().or_else(|| {
            // Try to get base_url from catalog config
            None  // Will be populated if --url is provided
        }),
        &backend,
    )?;

    // Store OAuth metadata for refresh
    if let Some(cred) = config.credentials.get_mut(service) {
        cred.oauth_managed = true;
        cred.oauth_token_url = Some(oauth_config.token_url.clone());
        cred.oauth_access_token_encrypted = Some(
            backend.store_credential_value(&format!("{service}:oauth_access"), &token_response.access_token)?
                .unwrap_or_default()
        );
        if let Some(ref rt) = token_response.refresh_token {
            cred.oauth_refresh_token_encrypted = Some(
                backend.store_credential_value(&format!("{service}:oauth_refresh"), rt)?
                    .unwrap_or_default()
            );
        }
        cred.oauth_token_expires_at = expires_at;
        cred.oauth_client_id_encrypted = Some(
            backend.store_credential_value(&format!("{service}:oauth_cid"), &cid)?
                .unwrap_or_default()
        );
        if let Some(ref cs) = csecret {
            cred.oauth_client_secret_encrypted = Some(
                backend.store_credential_value(&format!("{service}:oauth_csecret"), cs)?
                    .unwrap_or_default()
            );
        }
        cred.oauth_scopes = if final_scopes.is_empty() { None } else { Some(final_scopes) };
        cred.oauth_token_endpoint_auth_method = Some(oauth_config.token_endpoint_auth_method);
    }

    config.save(config_file)?;
    println!("OAuth credential stored for service '{service}'.");
    Ok(())
}
```

#### 3.4.5 `node-agent/src/main.rs` -- Token Refresh Background Task

**Add `oauth_refresh_loop` alongside `credential_reload_loop`:**

```rust
/// Background task that refreshes OAuth tokens before they expire.
async fn oauth_refresh_loop(
    config_file: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    sender: std::sync::Arc<SharedCredentialsSender>,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;

        let config = match NodeConfig::load(&config_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let backend = match SecretBackend::from_config(&config, &config_dir) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let mut config_changed = false;
        let mut updated_config = config.clone();

        for (slug, cred) in &config.credentials {
            if !cred.oauth_managed {
                continue;
            }

            // Check if token expires within 5 minutes
            let needs_refresh = match &cred.oauth_token_expires_at {
                Some(expires_str) => {
                    match chrono::DateTime::parse_from_rfc3339(expires_str) {
                        Ok(expires) => {
                            let now = chrono::Utc::now();
                            let buffer = chrono::Duration::minutes(5);
                            expires.with_timezone(&chrono::Utc) - buffer < now
                        }
                        Err(_) => false,
                    }
                }
                None => false,  // No expiry info, don't refresh
            };

            if !needs_refresh {
                continue;
            }

            // Load refresh token
            let refresh_token = match &cred.oauth_refresh_token_encrypted {
                Some(enc) => match backend.load_credential_value(
                    &format!("{slug}:oauth_refresh"),
                    Some(enc.as_str()),
                ) {
                    Ok(t) => t,
                    Err(_) => continue,
                },
                None => continue,
            };

            let client_id = match &cred.oauth_client_id_encrypted {
                Some(enc) => match backend.load_credential_value(
                    &format!("{slug}:oauth_cid"),
                    Some(enc.as_str()),
                ) {
                    Ok(t) => t,
                    Err(_) => continue,
                },
                None => continue,
            };

            let client_secret = cred.oauth_client_secret_encrypted.as_ref().and_then(|enc| {
                backend.load_credential_value(
                    &format!("{slug}:oauth_csecret"),
                    Some(enc.as_str()),
                ).ok()
            });

            let token_url = match &cred.oauth_token_url {
                Some(url) => url.as_str(),
                None => continue,
            };

            let auth_method = cred.oauth_token_endpoint_auth_method
                .as_deref()
                .unwrap_or("client_secret_post");

            // Attempt refresh
            match oauth::refresh_token(
                token_url,
                &client_id,
                client_secret.as_deref(),
                &refresh_token,
                auth_method,
            ).await {
                Ok(new_token) => {
                    tracing::info!(service = %slug, "OAuth token refreshed");

                    // Update the header credential with the new access token
                    let header_value = format!(
                        "{} {}",
                        new_token.token_type.as_deref().unwrap_or("Bearer"),
                        new_token.access_token
                    );

                    if let Some(cred_mut) = updated_config.credentials.get_mut(slug) {
                        // Update header value
                        let encrypted = backend.store_credential_value(slug, &header_value).ok().flatten();
                        cred_mut.header_value_encrypted = encrypted;

                        // Update OAuth tokens
                        cred_mut.oauth_access_token_encrypted = backend
                            .store_credential_value(&format!("{slug}:oauth_access"), &new_token.access_token)
                            .ok().flatten();

                        if let Some(ref rt) = new_token.refresh_token {
                            cred_mut.oauth_refresh_token_encrypted = backend
                                .store_credential_value(&format!("{slug}:oauth_refresh"), rt)
                                .ok().flatten();
                        }

                        cred_mut.oauth_token_expires_at = new_token.expires_in.map(|secs| {
                            (chrono::Utc::now() + chrono::Duration::seconds(secs as i64)).to_rfc3339()
                        });

                        config_changed = true;
                    }
                }
                Err(e) => {
                    tracing::warn!(service = %slug, error = %e, "OAuth token refresh failed");
                }
            }
        }

        if config_changed {
            if let Err(e) = updated_config.save(&config_file) {
                tracing::error!(error = %e, "Failed to save config after OAuth refresh");
            }
            // Config file change will be picked up by credential_reload_loop
        }
    }
}
```

**Spawn the refresh loop in `cmd_start`:**

```rust
async fn cmd_start(config_path: Option<&str>) -> Result<()> {
    // ... existing setup ...

    // Spawn credential reload loop
    let reload_handle = tokio::spawn(credential_reload_loop(
        config_file.clone(),
        config_dir.clone(),
        cred_sender.clone(),
        Duration::from_secs(5),
    ));

    // Spawn OAuth refresh loop (check every 60 seconds)
    let refresh_handle = tokio::spawn(oauth_refresh_loop(
        config_file.clone(),
        config_dir.clone(),
        cred_sender.clone(),
        Duration::from_secs(60),
    ));

    ws_client::run_with_shutdown(/* ... */).await;

    reload_handle.abort();
    refresh_handle.abort();
    Ok(())
}
```

#### 3.4.6 `node-agent/src/credential_store.rs`

No changes needed. The `CredentialStore` already reads from `header_value_encrypted` (or `param_value_encrypted`). The OAuth refresh loop updates `header_value_encrypted` with the current Bearer token, so `proxy_executor` picks it up automatically through the existing hot-reload mechanism.

### 3.5 Authorization Code Flow (Optional, Lower Priority)

For providers that don't support device code flow, the node can run a local HTTP server for the authorization_code redirect:

```rust
/// Run OAuth authorization code flow with local HTTP redirect.
pub async fn run_authorization_code_flow(
    config: &OAuthConfig,
    client_id: &str,
    client_secret: Option<&str>,
    scopes: &str,
) -> Result<TokenResponse> {
    let auth_url = config.authorization_url.as_deref()
        .ok_or_else(|| Error::Config("No authorization_url available".to_string()))?;

    // 1. Start local HTTP server on a random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    // 2. Generate PKCE challenge (if supported)
    let (code_verifier, code_challenge) = if config.supports_pkce {
        let verifier = generate_pkce_verifier();
        let challenge = compute_pkce_challenge(&verifier);
        (Some(verifier), Some(challenge))
    } else {
        (None, None)
    };

    // 3. Build authorization URL
    let state = generate_state();
    let mut url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&state={}&scope={}",
        auth_url,
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&state),
        urlencoding::encode(scopes),
    );
    if let Some(ref challenge) = code_challenge {
        url += &format!("&code_challenge={}&code_challenge_method=S256", challenge);
    }

    // 4. Open browser
    println!("Opening browser for authorization...");
    println!("If the browser doesn't open, visit: {url}");
    let _ = open::that(&url);

    // 5. Wait for callback
    let code = wait_for_callback(listener, &state).await?;

    // 6. Exchange code for token
    exchange_code_for_token(
        &config.token_url,
        client_id,
        client_secret,
        &code,
        &redirect_uri,
        code_verifier.as_deref(),
        &config.token_endpoint_auth_method,
    ).await
}
```

This is lower priority than device code flow and can be deferred to a follow-up.

---

## 4. Catalog API Changes (Summary)

The catalog endpoint (`GET /api/v1/catalog` and `GET /api/v1/catalog/{slug}`) gains new response fields:

### New fields for SSH services:

| Field | Type | Description |
|---|---|---|
| `service_type` | `string` | `"http"` or `"ssh"` |
| `ssh_host` | `string?` | SSH target hostname |
| `ssh_port` | `number?` | SSH target port |
| `ssh_ca_public_key` | `string?` | NyxID SSH CA public key for `authorized_keys` |
| `ssh_allowed_principals` | `string[]?` | Allowed SSH principals |
| `ssh_certificate_ttl_minutes` | `number?` | Certificate validity window |

### New fields for node-native OAuth:

| Field | Type | Description |
|---|---|---|
| `authorization_url` | `string?` | OAuth authorization endpoint |
| `token_url` | `string?` | OAuth token endpoint |
| `device_code_url` | `string?` | Device code request endpoint |
| `device_verification_url` | `string?` | User-facing verification URL |
| `device_token_url` | `string?` | Device code poll endpoint |
| `default_scopes` | `string[]?` | Default OAuth scopes |
| `supports_pkce` | `boolean?` | Whether PKCE is supported |
| `device_code_format` | `string?` | `"rfc8628"` or `"openai"` |
| `token_endpoint_auth_method` | `string?` | `"client_secret_post"` or `"client_secret_basic"` |
| `extra_auth_params` | `object?` | Extra auth URL params |

**Security note:** NyxID's own OAuth client credentials (`client_id_encrypted`, `client_secret_encrypted` from `ProviderConfig`) are never exposed through the catalog API. Users provide their own client credentials for node-native OAuth.

---

## 5. Complete File List

### Backend

| File | Action | Description |
|---|---|---|
| `backend/src/services/catalog_service.rs` | **modify** | Include SSH in catalog query; add SSH + OAuth fields to `CatalogEntry` |
| `backend/src/handlers/catalog.rs` | **modify** | Add SSH + OAuth fields to `CatalogEntryResponse` |
| `backend/src/services/unified_key_service.rs` | **modify** | Handle SSH/node-managed key creation (empty credential); add `service_type` to `KeyView`; SSH validation (require node_id) |
| `backend/src/handlers/keys.rs` | **modify** | Make `credential` optional in `CreateKeyRequest`; add `service_type` to `KeyResponse`; skip credential push for node-managed |
| `backend/src/models/user_service.rs` | **modify** | Add `service_type` field (default `"http"`) |
| `backend/src/services/user_service_service.rs` | **modify** | Add `service_type` parameter to `create_user_service` |
| `backend/src/services/user_api_key_service.rs` | **modify** | Handle empty credential (store `None` for `credential_encrypted`) |

### Node Agent

| File | Action | Description |
|---|---|---|
| `node-agent/src/cli.rs` | **modify** | Add `AddOauth` subcommand to `CredentialCommands` |
| `node-agent/src/config.rs` | **modify** | Add OAuth fields to `CredentialConfig` |
| `node-agent/src/oauth.rs` | **create** | OAuth flow module: catalog fetch, device code flow, authorization code flow, token refresh |
| `node-agent/src/main.rs` | **modify** | Add `cmd_credentials_add_oauth`; spawn `oauth_refresh_loop` in `cmd_start`; route `AddOauth` command |
| `node-agent/src/credential_store.rs` | no change | Already reads from `header_value_encrypted` which OAuth refresh updates |
| `node-agent/src/proxy_executor.rs` | no change | Already injects credentials from `CredentialStore` |

### Frontend

| File | Action | Description |
|---|---|---|
| `frontend/src/types/keys.ts` | **modify** | Add SSH fields to `CatalogEntry` and `KeyInfo`; add `service_type` |
| `frontend/src/components/dashboard/add-key-dialog.tsx` | **modify** | Route-first UX: reorder wizard steps, add `NodeSetupStep`, SSH catalog badge, revised `RoutingStep` |
| `frontend/src/pages/key-detail.tsx` | **modify** | Add SSH connection info section (CA public key, principals, setup instructions) |
| `frontend/src/hooks/use-keys.ts` | **modify** | Make `credential` optional in `CreateKeyParams` |

### No Changes Required

| File | Reason |
|---|---|
| `backend/src/services/credential_push_service.rs` | Kept for backward compat; skip is handled in `handlers/keys.rs` |
| `backend/src/handlers/ssh_exec.rs` | Already uses node routing via `node_routing_service` |
| `backend/src/handlers/ssh_tunnel.rs` | Already uses node routing |
| `backend/src/services/ssh_service.rs` | Unchanged (reads `SshServiceConfig` from `DownstreamService`) |
| `backend/src/models/downstream_service.rs` | Unchanged (`SshServiceConfig` already exists) |
| `backend/src/models/user_api_key.rs` | `credential_encrypted` is already `Option<Vec<u8>>` |
| `backend/src/models/user_endpoint.rs` | Unchanged |
| `node-agent/src/proxy_executor.rs` | Unchanged (reads from `CredentialStore` which is hot-reloaded) |
| `node-agent/src/credential_store.rs` | Unchanged (already reads from `header_value_encrypted`) |

---

## Migration Notes

- **No database migration needed.** New fields use `#[serde(default)]` for backward compatibility.
- **Existing keys are unaffected.** The new `service_type` field defaults to `"http"` and `credential` remains required for the direct routing path.
- **credential_push_service remains operational.** It continues to work for users who created keys with the old flow and want to change routing to a node later.
- **Node agent is backward compatible.** New OAuth fields in `CredentialConfig` use `#[serde(default)]`, so existing config.toml files continue to work.
