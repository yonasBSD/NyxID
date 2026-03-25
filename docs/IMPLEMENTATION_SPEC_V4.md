> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V4: Node Endpoint Resolution, Node Setup Helper, OAuth Credential Input

This spec builds on `IMPLEMENTATION_SPEC.md` (Phase 0), `IMPLEMENTATION_SPEC_V2.md` (Phase 1), and `IMPLEMENTATION_SPEC_V3.md` (Phase 2). It covers three new requirements:

1. **Node Endpoint Resolution** -- node agent stores target_url locally, NyxID sends empty base_url for node-routed services
2. **Node Setup Helper** -- copyable CLI command block shown on key-detail page when node routing is configured
3. **OAuth Credential Input in Add Key Dialog** -- for user/both credential_mode providers, collect client_id/client_secret before starting OAuth

---

## Table of Contents

1. [Requirement 1: Node Endpoint Resolution](#1-node-endpoint-resolution)
2. [Requirement 2: Node Setup Helper](#2-node-setup-helper)
3. [Requirement 3: OAuth Credential Input](#3-oauth-credential-input)
4. [Complete File List](#4-complete-file-list)

---

## 1. Node Endpoint Resolution

### 1.1 Problem

Currently, NyxID always resolves the target URL from `UserEndpoint.url` and sends it to the node agent in every proxy request via `NodeProxyRequest.base_url`. The node agent treats an empty `base_url` as an error (proxy_executor.rs:109-122).

For self-hosted services routed through a node, the target URL is known to the node (e.g., `http://localhost:18789` for a local OpenClaw instance), and storing it on NyxID is redundant or undesirable.

### 1.2 Design

**Two routing modes:**

| Mode | base_url in WS message | URL resolution |
|------|----------------------|----------------|
| **Direct routing** (no node) | Required from `UserEndpoint.url` | NyxID resolves |
| **Node routing** | Empty string if `UserEndpoint.url` is empty; otherwise NyxID's value | Node resolves from local config if empty |

**Key rule:** When `UserService.node_id` is set, `endpoint_url` becomes optional. When no node, `endpoint_url` remains required.

### 1.3 Node Agent Changes

#### 1.3.1 `node-agent/src/config.rs` -- Add `target_url` to CredentialConfig

```rust
// Add to CredentialConfig struct (line 88-104)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialConfig {
    /// "header" or "query_param"
    pub injection_method: String,

    /// Target URL for this service (e.g., "https://api.openai.com/v1").
    /// Used when NyxID sends an empty base_url (node-resolved routing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,

    // ... existing fields unchanged ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_value_encrypted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_value_encrypted: Option<String>,
}
```

Update `add_header_credential_via` and `add_query_param_credential_via` to accept an optional `target_url: Option<&str>` parameter:

```rust
// config.rs -- add target_url param
pub fn add_header_credential_via(
    &mut self,
    service_slug: &str,
    header_name: &str,
    header_value: &str,
    target_url: Option<&str>,
    backend: &SecretBackend,
) -> Result<()> {
    let encrypted = backend.store_credential_value(service_slug, header_value)?;
    self.credentials.insert(
        service_slug.to_string(),
        CredentialConfig {
            injection_method: "header".to_string(),
            target_url: target_url.map(String::from),
            header_name: Some(header_name.to_string()),
            header_value_encrypted: encrypted,
            param_name: None,
            param_value_encrypted: None,
        },
    );
    Ok(())
}
```

Same pattern for `add_query_param_credential_via`. Update all call sites (main.rs `cmd_credentials`, openclaw connect).

Also update the `#[cfg(test)]` helpers `add_header_credential` and `add_query_param_credential` to set `target_url: None`.

#### 1.3.2 `node-agent/src/cli.rs` -- Add `--url` flag to `credentials add`

```rust
// CredentialCommands::Add (line 131-153)
CredentialCommands::Add {
    /// Service slug (e.g., "openai", "github-api")
    #[arg(long)]
    service: String,

    /// Target URL for this service (e.g., "https://api.openai.com/v1").
    /// Stored locally; used when NyxID sends an empty base_url.
    #[arg(long)]
    url: Option<String>,

    /// Header name to inject (e.g., "Authorization"). The value will be prompted securely.
    #[arg(long)]
    header: Option<String>,

    // ... existing fields unchanged ...
},
```

#### 1.3.3 `node-agent/src/credential_store.rs` -- Expose `target_url` on ServiceCredential

```rust
/// A single service's decrypted credential.
#[derive(Clone)]
pub struct ServiceCredential {
    pub injection: CredentialInjection,
    /// Local target URL for this service (used when NyxID sends empty base_url).
    pub target_url: Option<String>,
}
```

Update `from_config` and `from_config_with_backend` to populate `target_url` from `cred_config.target_url.clone()`.

Add accessor:

```rust
impl ServiceCredential {
    /// Local target URL for this service.
    pub fn target_url(&self) -> Option<&str> {
        self.target_url.as_deref()
    }
}
```

#### 1.3.4 `node-agent/src/proxy_executor.rs` -- Fallback to local target_url

Replace the current error on empty `base_url` (lines 109-122) with a fallback:

```rust
// proxy_executor.rs, after line 107
let base_url = request["base_url"].as_str().unwrap_or("");

// If NyxID sent an empty base_url, resolve from local credential config
let effective_base_url = if base_url.is_empty() {
    match cred.target_url() {
        Some(url) => url,
        None => {
            metrics.record_error();
            let _ = send_ws_message(
                tx,
                proxy_error_response(
                    request_id,
                    &format!(
                        "No target URL configured for service '{service_slug}'. \
                         Run: nyxid-node credentials add --service {service_slug} --url <URL> ..."
                    ),
                    502,
                    false,
                ),
            )
            .await;
            return;
        }
    }
} else {
    base_url
};
```

Then use `effective_base_url` instead of `base_url` in the URL construction below (line 129).

#### 1.3.5 `node-agent/src/main.rs` -- Pass `--url` to credential add

Update `cmd_credentials` to accept and pass through the `url` field:

```rust
// main.rs cmd_credentials, CredentialCommands::Add match arm
CredentialCommands::Add {
    service,
    url,          // NEW
    header,
    query_param,
    secret_format,
    value,
} => {
    // ... existing config loading ...

    if let Some(header_name) = header {
        // ... existing logic ...
        // Pass target_url to add_header_credential_via
        config.add_header_credential_via(
            &service, &header_name, &secret, url.as_deref(), &backend
        )?;
    } else if let Some(param_name) = query_param {
        // ... existing logic ...
        config.add_query_param_credential_via(
            &service, &param_name, &secret, url.as_deref(), &backend
        )?;
    }
    // ...
}
```

Also update `cmd_openclaw_connect` to pass the OpenClaw URL as `target_url` when adding the credential.

### 1.4 Backend Changes

#### 1.4.1 `backend/src/services/unified_key_service.rs` -- Optional endpoint_url when node_id is set

In `create_key()`, change the custom path validation (line 180):

```rust
// Custom path: endpoint_url required UNLESS caller provides node_id
let ep_url = endpoint_url.unwrap_or("");
if ep_url.is_empty() && node_id.is_none() {
    return Err(AppError::BadRequest(
        "endpoint_url is required for custom endpoints without node routing".to_string(),
    ));
}
```

Add a `node_id: Option<&str>` parameter to `create_key()`.

For the catalog path (line 98-114), similarly allow empty `ep_url` when `node_id` is provided:

```rust
let ep_url = if let Some(url) = endpoint_url {
    url.to_string()
} else if node_id.is_some() {
    // Node-routed: endpoint URL stored on node, not on NyxID
    String::new()
} else if let Some(ref pid) = svc.provider_config_id {
    // ... existing gateway_url check ...
} else {
    svc.base_url.clone()
};
```

Pass `node_id` through to `user_service_service::create_user_service()`.

#### 1.4.2 `backend/src/models/user_endpoint.rs` -- Allow empty URL

No model change needed. The `url` field is already a `String`. An empty string is a valid sentinel for "node-resolved".

Update `user_endpoint_service::create_endpoint` to skip URL validation when `url` is empty:

```rust
// user_endpoint_service.rs create_endpoint()
pub async fn create_endpoint(
    db: &mongodb::Database,
    user_id: &str,
    label: &str,
    url: &str,
    catalog_service_id: Option<&str>,
) -> AppResult<UserEndpoint> {
    // ... existing label validation ...

    // Skip URL validation for node-resolved endpoints (empty URL)
    if !url.is_empty() {
        validate_base_url(url)?;
    }

    // ... rest unchanged ...
}
```

#### 1.4.3 `backend/src/handlers/proxy.rs` -- Send empty base_url for node-routed services

In `execute_proxy_inner()`, when building the `NodeProxyRequest` (line 392-406), conditionally clear the base_url:

```rust
let node_request = NodeProxyRequest {
    request_id: uuid::Uuid::new_v4().to_string(),
    service_id: service_id.to_string(),
    service_slug: target.service.slug.clone(),
    // If the endpoint URL is empty (node-resolved), send empty base_url
    // so the node agent resolves from its local config
    base_url: if target.base_url.is_empty() {
        String::new()
    } else {
        target.base_url.clone()
    },
    method: method_str.clone(),
    // ... rest unchanged ...
};
```

No additional logic needed -- the existing `target.base_url` will already be empty when the `UserEndpoint.url` is empty, as `proxy_service::resolve_proxy_target_from_user_service` copies `endpoint.url` directly into `ProxyTarget.base_url`.

#### 1.4.4 `backend/src/handlers/keys.rs` -- Accept optional endpoint_url and node_id in create request

Add `node_id: Option<String>` to the create key request body. Pass it through to `unified_key_service::create_key()`.

```rust
// In CreateKeyRequest (handlers/keys.rs)
#[derive(Deserialize)]
pub struct CreateKeyRequest {
    // ... existing fields ...
    pub endpoint_url: Option<String>,
    pub node_id: Option<String>,  // NEW
}
```

### 1.5 Frontend Changes

#### 1.5.1 `frontend/src/components/dashboard/add-key-dialog.tsx` -- Optional endpoint URL

In `KeyForm`, when a node is configured for routing, the endpoint URL field becomes optional and shows a hint.

This is a future enhancement -- for V4, the add-key dialog does not yet support selecting a node at creation time. Users configure node routing after creation via the key-detail page. The endpoint URL field should be labeled "optional if routing via node" but this can be deferred.

#### 1.5.2 `frontend/src/pages/key-detail.tsx` -- Show empty endpoint state

In `EndpointSection`, handle empty `endpointUrl`:

```tsx
// EndpointSection -- handle node-resolved endpoint
{endpointUrl ? (
  // ... existing URL display + edit UI ...
) : (
  <div className="flex items-center gap-2 text-sm text-muted-foreground">
    <span>Resolved by node agent</span>
    <Button size="icon" variant="ghost" onClick={() => setEditing(true)}>
      <Pencil className="h-4 w-4" />
    </Button>
  </div>
)}
```

---

## 2. Node Setup Helper

### 2.1 Problem

When a user configures an AI service to route via a node, they need to manually set up the node agent with the correct credentials. Currently they must figure out the CLI syntax on their own.

### 2.2 Design

Show a copyable command block in the `RoutingSection` of `key-detail.tsx` when a node is selected. The block includes:
- Service slug
- Default endpoint URL (from catalog or current endpoint)
- Auth method and key name
- Placeholder for the credential value

### 2.3 Frontend Changes

#### 2.3.1 `frontend/src/pages/key-detail.tsx` -- Add NodeSetupHelper component

Add a new component after the `RoutingSection`:

```tsx
function NodeSetupHelper({
  slug,
  endpointUrl,
  authMethod,
  authKeyName,
  catalogServiceName,
}: {
  readonly slug: string;
  readonly endpointUrl: string;
  readonly authMethod: string;
  readonly authKeyName: string;
  readonly catalogServiceName: string | null;
}) {
  // Build the CLI command based on auth method
  const urlFlag = endpointUrl ? ` \\\n  --url ${endpointUrl}` : "";

  let credentialFlags: string;
  switch (authMethod) {
    case "bearer":
      credentialFlags = ` \\\n  --header ${authKeyName} \\\n  --secret-format bearer`;
      break;
    case "header":
      credentialFlags = ` \\\n  --header ${authKeyName}`;
      break;
    case "query":
      credentialFlags = ` \\\n  --query-param ${authKeyName}`;
      break;
    case "basic":
      credentialFlags = ` \\\n  --header ${authKeyName} \\\n  --secret-format basic`;
      break;
    case "none":
      credentialFlags = "";
      break;
    default:
      credentialFlags = ` \\\n  --header ${authKeyName}`;
  }

  const command = `nyxid-node credentials add \\\n  --service ${slug}${urlFlag}${credentialFlags}`;

  function handleCopy() {
    void copyToClipboard(command).then(() => {
      toast.success("Command copied to clipboard");
    });
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2">
          <Terminal className="h-4 w-4 text-primary" />
          <CardTitle className="text-sm">Node Setup</CardTitle>
        </div>
        <CardDescription>
          Run this on your node to configure credentials
          {catalogServiceName ? ` for ${catalogServiceName}` : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="relative">
          <pre className="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-relaxed">
            {command}
          </pre>
          <Button
            size="icon"
            variant="ghost"
            className="absolute right-2 top-2 h-7 w-7"
            onClick={handleCopy}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">
          The agent will prompt for the secret value securely. After adding, the
          credential will be encrypted and stored locally on the node.
        </p>
      </CardContent>
    </Card>
  );
}
```

Add `Terminal` and `Copy` to the lucide-react imports.

#### 2.3.2 `frontend/src/pages/key-detail.tsx` -- Render NodeSetupHelper

In `KeyDetailPage`, render the helper when `nodeId` is set:

```tsx
<div className="grid gap-4 md:grid-cols-2">
  <EndpointSection ... />
  <ApiKeySection ... />
  <ServiceSection ... />
  <RoutingSection ... />

  {keyInfo.node_id && (
    <NodeSetupHelper
      slug={keyInfo.slug}
      endpointUrl={keyInfo.endpoint_url}
      authMethod={keyInfo.auth_method}
      authKeyName={keyInfo.auth_key_name}
      catalogServiceName={keyInfo.catalog_service_name}
    />
  )}
</div>
```

The helper spans the full width of the grid when displayed (use `md:col-span-2`).

---

## 3. OAuth Credential Input in Add Key Dialog

### 3.1 Problem

For OAuth providers with `credential_mode` of `"user"` or `"both"` (e.g., Twitter/X), users must provide their own OAuth app credentials (client_id + client_secret) before the OAuth redirect can start. Currently, the add-key dialog skips straight to the OAuth flow using admin-configured credentials.

The existing `UserCredentialsDialog` in `provider-grid.tsx` handles this for the old Providers page, but the new add-key dialog does not.

### 3.2 Design

**Flow for OAuth with user credentials:**

1. User selects an OAuth service (e.g., Twitter/X) in the catalog grid
2. Check `credential_mode` from catalog entry
3. If `credential_mode` is `"user"` or `"both"`:
   a. Show a new `"oauth_credentials"` step with client_id + client_secret inputs
   b. User fills in credentials -> saved via `PUT /api/v1/providers/{id}/credentials`
   c. On success, proceed to the `"oauth"` step (existing OAuth redirect)
4. If `credential_mode` is `"admin"` (default) or not set: proceed directly to OAuth step (current behavior)

**Same flow for device_code with user credentials:**

Same pattern -- show credential input step first, then proceed to device code flow.

### 3.3 Backend Changes

#### 3.3.1 `backend/src/services/catalog_service.rs` -- Add credential_mode to CatalogEntry

```rust
// catalog_service.rs -- CatalogEntry struct
pub struct CatalogEntry {
    // ... existing fields ...
    pub credential_mode: Option<String>,  // NEW
}
```

Populate from provider config:

```rust
// In list_catalog and get_catalog_entry mapping
CatalogEntry {
    // ... existing fields ...
    credential_mode: provider.map(|p| p.credential_mode.clone()),
}
```

#### 3.3.2 `backend/src/handlers/catalog.rs` -- Add credential_mode to CatalogEntryResponse

```rust
// catalog.rs -- CatalogEntryResponse struct
#[derive(Debug, Serialize)]
pub struct CatalogEntryResponse {
    // ... existing fields ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_mode: Option<String>,  // NEW
}
```

Map in `catalog_entry_response()`:

```rust
fn catalog_entry_response(entry: catalog_service::CatalogEntry) -> CatalogEntryResponse {
    CatalogEntryResponse {
        // ... existing fields ...
        credential_mode: entry.credential_mode,
    }
}
```

### 3.4 Frontend Changes

#### 3.4.1 `frontend/src/types/keys.ts` -- Add credential_mode to CatalogEntry type

```typescript
export interface CatalogEntry {
  // ... existing fields ...
  readonly credential_mode: string | null;  // NEW: "admin" | "user" | "both" | null
}
```

#### 3.4.2 `frontend/src/components/dashboard/add-key-dialog.tsx` -- New wizard step

**Add new step type:**

```typescript
type WizardStep = "catalog" | "form" | "oauth_credentials" | "oauth" | "device_code";
```

**Add OAuthCredentialsStep component:**

```tsx
function OAuthCredentialsStep({
  catalogEntry,
  onBack,
  onComplete,
}: {
  readonly catalogEntry: CatalogEntry;
  readonly onBack: () => void;
  readonly onComplete: () => void;
}) {
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const setCredentials = useSetProviderCredentials();

  async function handleSave() {
    if (!catalogEntry.provider_config_id) return;
    setError(null);

    try {
      await setCredentials.mutateAsync({
        providerId: catalogEntry.provider_config_id,
        client_id: clientId.trim(),
        client_secret: clientSecret.trim() || undefined,
      });
      onComplete();
    } catch (err) {
      const message =
        err instanceof ApiError
          ? err.message
          : "Failed to save OAuth credentials";
      setError(message);
    }
  }

  return (
    <div className="space-y-4">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="h-3 w-3" />
        Back to catalog
      </button>

      <div className="rounded-lg border border-border bg-muted/50 p-3">
        <p className="text-sm font-medium">{catalogEntry.name}</p>
        <p className="text-xs text-muted-foreground">
          This service requires your own OAuth app credentials.
        </p>
      </div>

      {catalogEntry.documentation_url && (
        <a
          href={catalogEntry.documentation_url}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
        >
          How to create an OAuth app
          <ExternalLink className="h-3 w-3" />
        </a>
      )}

      {error && (
        <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="space-y-3">
        <div className="space-y-1.5">
          <Label htmlFor="oauth-client-id">
            Client ID <span className="text-destructive">*</span>
          </Label>
          <Input
            id="oauth-client-id"
            placeholder="Your OAuth app Client ID"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            autoComplete="off"
          />
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="oauth-client-secret">Client Secret</Label>
          <Input
            id="oauth-client-secret"
            type="password"
            placeholder="Your OAuth app Client Secret (optional for public clients)"
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            autoComplete="off"
          />
        </div>
      </div>

      <Button
        className="w-full"
        onClick={() => void handleSave()}
        disabled={setCredentials.isPending || !clientId.trim()}
      >
        {setCredentials.isPending ? "Saving..." : "Continue to Authentication"}
      </Button>
    </div>
  );
}
```

**Add import for `useSetProviderCredentials`:**

```typescript
import {
  useInitiateOAuth,
  useInitiateDeviceCode,
  usePollDeviceCode,
  useSetProviderCredentials,  // NEW
} from "@/hooks/use-providers";
```

**Update `handleSelectCatalog` routing logic:**

```typescript
function handleSelectCatalog(entry: CatalogEntry) {
  setSelectedEntry(entry);

  const needsUserCreds =
    entry.credential_mode === "user" || entry.credential_mode === "both";

  // OAuth providers
  if (entry.provider_type === "oauth2" && entry.provider_config_id) {
    if (needsUserCreds) {
      setStep("oauth_credentials");
    } else {
      setStep("oauth");
    }
    return;
  }

  // Device code providers
  if (entry.provider_type === "device_code" && entry.provider_config_id) {
    if (needsUserCreds) {
      setStep("oauth_credentials");
    } else {
      setStep("device_code");
    }
    return;
  }

  // Default: API key input form
  setForm({
    ...INITIAL_FORM,
    label: entry.name,
    authMethod: entry.auth_method ?? "bearer",
    authKeyName: entry.auth_key_name ?? "Authorization",
  });
  setStep("form");
}
```

**Add handler for when credentials are saved:**

```typescript
function handleCredentialsSaved() {
  if (!selectedEntry) return;

  // After user credentials saved, proceed to the appropriate auth flow
  if (selectedEntry.provider_type === "device_code") {
    setStep("device_code");
  } else {
    setStep("oauth");
  }
}
```

**Add rendering for the new step:**

```tsx
{step === "oauth_credentials" && selectedEntry && (
  <OAuthCredentialsStep
    catalogEntry={selectedEntry}
    onBack={() => setStep("catalog")}
    onComplete={handleCredentialsSaved}
  />
)}
```

**Update `dialogTitle` and `dialogDescription`:**

```typescript
function dialogTitle(): string {
  switch (step) {
    case "oauth_credentials":
      return `Setup ${selectedEntry?.name ?? "Service"} Credentials`;
    // ... existing cases ...
  }
}

function dialogDescription(): string {
  switch (step) {
    case "oauth_credentials":
      return `Enter your OAuth app credentials for ${selectedEntry?.name ?? "the service"}.`;
    // ... existing cases ...
  }
}
```

#### 3.4.3 `frontend/src/types/keys.ts` -- Add documentation_url if not present

Already present in `CatalogEntry` type. No change needed.

### 3.5 API Call Sequence

For an OAuth provider with `credential_mode: "user"`:

1. `GET /api/v1/catalog` -- user browses catalog, sees provider_type "oauth2" and credential_mode "user"
2. User clicks service -> `oauth_credentials` step shown
3. User enters client_id + client_secret -> `PUT /api/v1/providers/{id}/credentials` (existing endpoint)
4. On success -> `oauth` step shown
5. User clicks "Connect" -> `GET /api/v1/providers/{id}/connect/oauth` (existing endpoint)
6. Redirect to OAuth provider -> callback -> token stored

No new backend endpoints needed. The existing `PUT /providers/{id}/credentials` and `GET /providers/{id}/connect/oauth` endpoints handle this flow. The only backend change is exposing `credential_mode` in the catalog response.

---

## 4. Complete File List

### Node Agent (Requirement 1)

| File | Change |
|------|--------|
| `node-agent/src/config.rs` | Add `target_url: Option<String>` to `CredentialConfig`; update `add_header_credential_via`, `add_query_param_credential_via` to accept `target_url` param; update test helpers |
| `node-agent/src/cli.rs` | Add `--url` flag to `CredentialCommands::Add` |
| `node-agent/src/credential_store.rs` | Add `target_url: Option<String>` to `ServiceCredential`; add `target_url()` accessor; populate from config in `from_config`/`from_config_with_backend` |
| `node-agent/src/proxy_executor.rs` | Replace empty `base_url` error with fallback to `cred.target_url()`; use `effective_base_url` variable |
| `node-agent/src/main.rs` | Pass `url` field from CLI to `add_header_credential_via`/`add_query_param_credential_via`; update openclaw connect to pass URL as `target_url` |

### Backend (Requirements 1, 3)

| File | Change |
|------|--------|
| `backend/src/services/unified_key_service.rs` | Add `node_id: Option<&str>` param to `create_key()`; allow empty endpoint_url when node_id is set |
| `backend/src/services/user_endpoint_service.rs` | Skip URL validation when `url` is empty (node-resolved) |
| `backend/src/handlers/keys.rs` | Add `node_id: Option<String>` to `CreateKeyRequest`; pass to `unified_key_service::create_key()` |
| `backend/src/services/catalog_service.rs` | Add `credential_mode: Option<String>` to `CatalogEntry`; populate from `ProviderConfig` |
| `backend/src/handlers/catalog.rs` | Add `credential_mode: Option<String>` to `CatalogEntryResponse`; map from `CatalogEntry` |

### Frontend (Requirements 1, 2, 3)

| File | Change |
|------|--------|
| `frontend/src/types/keys.ts` | Add `credential_mode: string \| null` to `CatalogEntry` |
| `frontend/src/components/dashboard/add-key-dialog.tsx` | Add `"oauth_credentials"` wizard step; add `OAuthCredentialsStep` component; update `handleSelectCatalog` to check `credential_mode`; import `useSetProviderCredentials` |
| `frontend/src/pages/key-detail.tsx` | Add `NodeSetupHelper` component; render when `node_id` is set; handle empty `endpointUrl` in `EndpointSection`; add `Terminal`, `Copy` imports |

### No Changes Required

| File | Reason |
|------|--------|
| `backend/src/models/user_service.rs` | `endpoint_id` is already `String` (non-optional), no change needed -- the endpoint is still created, just with empty URL |
| `backend/src/models/user_endpoint.rs` | `url` is already `String`, empty string is valid sentinel |
| `backend/src/handlers/proxy.rs` | Already passes `target.base_url` to `NodeProxyRequest.base_url` -- when endpoint URL is empty, this naturally sends empty string |
| `backend/src/services/node_ws_manager.rs` | `WsProxyRequest` and `NodeProxyRequest` already use `String` for `base_url` |
