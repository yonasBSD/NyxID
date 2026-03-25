> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V5: Node Selection at Creation, Auto Base URL, OAuth Credential Push

This spec builds on V1-V4. It covers three requirements:

1. **Node Selection at Key Creation** -- optional node routing step in add-key-dialog
2. **Auto-fill Base URL from Catalog** -- pre-populate endpoint URL when selecting a catalog service
3. **OAuth Credential Push to Nodes** -- push access tokens to nodes via WebSocket after OAuth callback and token refresh

---

## Table of Contents

1. [Requirement 1: Node Selection at Key Creation](#1-node-selection-at-key-creation)
2. [Requirement 2: Auto-fill Base URL from Catalog](#2-auto-fill-base-url-from-catalog)
3. [Requirement 3: OAuth Credential Push to Nodes](#3-oauth-credential-push-to-nodes)
4. [Complete File List](#4-complete-file-list)

---

## 1. Node Selection at Key Creation

### 1.1 Problem

The add-key-dialog wizard has no step for selecting node routing. Users must configure node routing separately after creation. The backend `CreateKeyRequest` already accepts `node_id: Option<String>`, but the frontend never sends it.

### 1.2 Design

Add a new optional wizard step `"routing"` between credential input (`"form"`) and final submission. The step shows two options:

- **Route directly** (default) -- requests go from NyxID to the endpoint URL
- **Route via node** -- requests go through a connected node agent; shows a dropdown of user's online nodes

When a node is selected, the endpoint URL becomes optional (the node resolves it locally per V4 spec). The `node_id` is included in the `POST /api/v1/keys` request body.

For OAuth/device-code flows, the routing step appears *after* the OAuth flow completes successfully, before the dialog closes. This requires a new post-OAuth step where the user can optionally bind the newly created service to a node.

### 1.3 Frontend Changes

#### 1.3.1 `frontend/src/components/dashboard/add-key-dialog.tsx`

**Add new WizardStep variant:**

```typescript
type WizardStep = "catalog" | "form" | "routing" | "oauth_credentials" | "oauth" | "device_code";
```

**Add `nodeId` to FormState:**

```typescript
interface FormState {
  readonly credential: string;
  readonly label: string;
  readonly endpointUrl: string;
  readonly slug: string;
  readonly authMethod: string;
  readonly authKeyName: string;
  readonly nodeId: string; // empty string = direct routing
}

const INITIAL_FORM: FormState = {
  credential: "",
  label: "",
  endpointUrl: "",
  slug: "",
  authMethod: "bearer",
  authKeyName: "Authorization",
  nodeId: "",
};
```

**New `RoutingStep` component:**

```typescript
function RoutingStep({
  form,
  onChange,
  onSubmit,
  onBack,
  isPending,
  endpointRequired,
}: {
  readonly form: FormState;
  readonly onChange: (updates: Partial<FormState>) => void;
  readonly onSubmit: () => void;
  readonly onBack: () => void;
  readonly isPending: boolean;
  readonly endpointRequired: boolean;
}) {
  const { data: nodes, isLoading } = useNodes();
  const onlineNodes = nodes?.filter((n) => n.status === "online") ?? [];

  return (
    <div className="space-y-4">
      <button type="button" onClick={onBack} className="...">
        <ArrowLeft className="h-3 w-3" /> Back
      </button>

      <div className="space-y-3">
        <Label>Request Routing</Label>
        <Select
          value={form.nodeId || "direct"}
          onValueChange={(v) => onChange({ nodeId: v === "direct" ? "" : v })}
        >
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="direct">Route directly (NyxID to endpoint)</SelectItem>
            {onlineNodes.map((node) => (
              <SelectItem key={node.id} value={node.id}>
                {node.name} ({node.status})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {!form.nodeId && endpointRequired && !form.endpointUrl.trim() && (
          <p className="text-xs text-destructive">
            Endpoint URL is required for direct routing.
          </p>
        )}

        {form.nodeId && (
          <p className="text-xs text-muted-foreground">
            Requests will be routed through the selected node.
            The node must have credentials configured locally.
          </p>
        )}
      </div>

      <Button className="w-full" onClick={onSubmit} disabled={isPending}>
        {isPending ? "Creating..." : "Create Service"}
      </Button>
    </div>
  );
}
```

**Updated wizard flow for API key services:**

In `KeyForm`, change the submit button to navigate to routing step instead of submitting directly:

```typescript
// In KeyForm: change onSubmit to onNext
<Button
  className="w-full"
  onClick={onNext} // was onSubmit
  disabled={!form.credential.trim() || !form.label.trim()}
>
  Next: Configure Routing
</Button>
```

In `handleSelectCatalog` for API key services, flow becomes: `catalog` -> `form` -> `routing` (submit).

**Updated wizard flow for OAuth/device-code services:**

After successful OAuth/device-code completion, instead of closing the dialog, transition to a post-auth routing step. This requires:

1. In `OAuthStep` and `DeviceCodeStep`, add an `onSuccess` callback prop that transitions to routing
2. In the routing step after OAuth, the `onSubmit` calls a new mutation to update the `UserService.node_id` for the just-created service (or skip if "direct")

However, for OAuth flows the service is auto-provisioned by the old provider flow (via `user_token_service::handle_oauth_callback`), not via the unified key system. The simplest approach: **defer node selection for OAuth services to the key-detail page** (already supported via edit). The add-key-dialog routing step only applies to the `form` step (API key / manual credential input).

**Final wizard flow:**

| Service Type | Steps |
|---|---|
| API key (catalog) | `catalog` -> `form` -> `routing` -> submit |
| API key (custom) | `catalog` -> `form` -> `routing` -> submit |
| OAuth (platform creds) | `catalog` -> `oauth` -> close |
| OAuth (user creds) | `catalog` -> `oauth_credentials` -> `oauth` -> close |
| Device code | `catalog` -> `device_code` -> close |

**Include `node_id` in submit:**

```typescript
function handleSubmit() {
  const params = selectedEntry
    ? {
        credential: form.credential,
        label: form.label,
        service_slug: selectedEntry.slug,
        ...(form.endpointUrl.trim() ? { endpoint_url: form.endpointUrl.trim() } : {}),
        ...(form.nodeId ? { node_id: form.nodeId } : {}),
        // ... existing auth overrides
      }
    : {
        credential: form.credential,
        label: form.label,
        endpoint_url: form.endpointUrl.trim(),
        slug: form.slug.trim(),
        auth_method: form.authMethod,
        auth_key_name: form.authKeyName,
        ...(form.nodeId ? { node_id: form.nodeId } : {}),
      };

  createKey.mutate(params, { /* existing handlers */ });
}
```

**Import `useNodes`:**

Add import from `@/hooks/use-nodes` for the `RoutingStep` component. Import `Server` icon from `lucide-react` for node routing visual.

#### 1.3.2 `frontend/src/hooks/use-keys.ts`

Update `CreateKeyParams` type to include `node_id`:

```typescript
interface CreateKeyParams {
  // ... existing fields
  readonly node_id?: string;
}
```

No backend changes needed -- `CreateKeyRequest` already has `node_id: Option<String>`.

---

## 2. Auto-fill Base URL from Catalog

### 2.1 Problem

When a user selects a catalog service (e.g., "OpenAI"), the endpoint URL should auto-fill from `CatalogEntry.base_url`. Currently the dialog only shows the endpoint URL field for custom endpoints and `requires_gateway_url` services, but does not pre-fill it.

### 2.2 Current State

The backend already returns `base_url` in `CatalogEntryResponse` (handlers/catalog.rs:18). The frontend `CatalogEntry` type already includes `base_url: string` (types/keys.ts:30). The catalog service returns `base_url` from `DownstreamService.base_url` (catalog_service.rs:75).

The dialog's `handleSelectCatalog` function sets form defaults but does NOT set `endpointUrl`:

```typescript
// Current (add-key-dialog.tsx:846-851)
setForm({
  ...INITIAL_FORM,
  label: entry.name,
  authMethod: entry.auth_method ?? "bearer",
  authKeyName: entry.auth_key_name ?? "Authorization",
});
```

### 2.3 Fix

**In `handleSelectCatalog` (add-key-dialog.tsx), add `endpointUrl` from catalog:**

```typescript
setForm({
  ...INITIAL_FORM,
  label: entry.name,
  endpointUrl: entry.base_url, // <-- auto-fill from catalog
  authMethod: entry.auth_method ?? "bearer",
  authKeyName: entry.auth_key_name ?? "Authorization",
});
```

This is a one-line change. The `KeyForm` component already shows the endpoint URL field for custom endpoints and `requires_gateway_url` services. For standard catalog services, the endpoint URL is sent in the `POST /api/v1/keys` request as `endpoint_url`, which `unified_key_service::create_key` uses to create the `UserEndpoint`.

**Also show the endpoint URL field (read-only) for all catalog services** so users can see what URL their service will target. Change the visibility logic:

```typescript
// Current:
const showEndpointUrl = isCustom || (catalogEntry?.requires_gateway_url ?? false);

// New: always show for catalog entries (read-only unless requires_gateway_url or custom)
const showEndpointUrl = true;
const endpointReadOnly = !isCustom && !(catalogEntry?.requires_gateway_url ?? false);
```

When `endpointReadOnly` is true, render the Input with `readOnly` attribute and muted styling so users see the pre-filled URL but can't accidentally change it. For `requires_gateway_url` services, the field remains editable (they need to enter their self-hosted URL).

### 2.4 Backend Verification

The backend `unified_key_service::create_key` already handles `endpoint_url`:
- When `service_slug` is provided (catalog): uses `endpoint_url` if given, otherwise falls back to catalog's `base_url`
- When custom: `endpoint_url` is required

No backend changes needed for this requirement.

---

## 3. OAuth Credential Push to Nodes

### 3.1 Problem

When a user completes an OAuth flow for a service that is routed through a node, the node agent has no way to receive the OAuth access token. The node needs the token to inject into proxy requests. Currently, node credentials are only configured via the CLI (`credentials add`).

Two scenarios require pushing credentials to nodes:
1. **Initial OAuth completion** -- user authenticates via OAuth, NyxID stores the token, but the node doesn't have it
2. **Token refresh** -- NyxID refreshes an expired OAuth token during proxy resolution, but the node still has the stale token

### 3.2 Design Overview

A new fire-and-forget WebSocket message type `credential_update` flows from NyxID to the node agent. The node agent handles it by updating its in-memory credential store and persisting to disk.

```
NyxID (after OAuth callback or token refresh)
  |
  v
NodeWsManager.send_credential_update(node_id, msg)
  |
  v  (WebSocket)
Node Agent (ws_client.rs message loop)
  |
  v
credential_store update (in-memory) + config.toml persist (encrypted)
```

### 3.3 WebSocket Message Format

**NyxID -> Node Agent:**

```json
{
  "type": "credential_update",
  "service_slug": "llm-openai",
  "injection_method": "header",
  "header_name": "Authorization",
  "header_value": "Bearer eyJhbGci...",
  "target_url": "https://api.openai.com/v1"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | string | yes | Always `"credential_update"` |
| `service_slug` | string | yes | Service slug to update credentials for |
| `injection_method` | string | yes | `"header"` or `"query_param"` |
| `header_name` | string | if header | Header name (e.g., `"Authorization"`) |
| `header_value` | string | if header | Full header value (e.g., `"Bearer token..."`) |
| `param_name` | string | if query_param | Query parameter name |
| `param_value` | string | if query_param | Query parameter value |
| `target_url` | string | no | Target URL for the service endpoint |

**Node Agent -> NyxID (acknowledgment):**

```json
{
  "type": "credential_update_ack",
  "service_slug": "llm-openai",
  "status": "ok"
}
```

Or on error:

```json
{
  "type": "credential_update_ack",
  "service_slug": "llm-openai",
  "status": "error",
  "error": "Failed to persist credential"
}
```

The ack is informational only (fire-and-forget from NyxID's perspective). NyxID logs the ack but does not block on it.

### 3.4 Backend Changes

#### 3.4.1 `backend/src/services/node_ws_manager.rs` -- New `send_credential_update` method

Add a new WS message struct and send method, following the `send_heartbeat_ping` pattern (fire-and-forget, non-blocking):

```rust
/// JSON message for pushing a credential update to a node.
#[derive(Debug, Serialize)]
struct WsCredentialUpdate {
    #[serde(rename = "type")]
    msg_type: &'static str,
    service_slug: String,
    injection_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    header_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    header_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    param_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    param_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_url: Option<String>,
}
```

Public struct for callers to build credential update requests:

```rust
/// Parameters for pushing a credential update to a node.
pub struct CredentialUpdateParams {
    pub service_slug: String,
    pub injection_method: String,
    pub header_name: Option<String>,
    pub header_value: Option<String>,
    pub param_name: Option<String>,
    pub param_value: Option<String>,
    pub target_url: Option<String>,
}
```

Send method on `NodeWsManager`:

```rust
impl NodeWsManager {
    /// Push a credential update to a connected node. Fire-and-forget.
    /// Returns Ok(()) if the message was queued, Err if node is not connected.
    pub fn send_credential_update(
        &self,
        node_id: &str,
        params: &CredentialUpdateParams,
    ) -> AppResult<()> {
        let conn = self
            .connections
            .get(node_id)
            .ok_or_else(|| AppError::NodeOffline(
                format!("Node {node_id} is not connected")
            ))?;

        let msg = WsCredentialUpdate {
            msg_type: "credential_update",
            service_slug: params.service_slug.clone(),
            injection_method: params.injection_method.clone(),
            header_name: params.header_name.clone(),
            header_value: params.header_value.clone(),
            param_name: params.param_name.clone(),
            param_value: params.param_value.clone(),
            target_url: params.target_url.clone(),
        };

        let json = serde_json::to_string(&msg).map_err(|e| {
            AppError::Internal(format!("Failed to serialize credential_update: {e}"))
        })?;

        conn.tx
            .try_send(NodeOutboundMessage::Text(json))
            .map_err(|_| {
                AppError::NodeOffline(format!(
                    "Node {node_id} connection closed or buffer full"
                ))
            })?;

        tracing::info!(
            node_id = %node_id,
            service_slug = %params.service_slug,
            "Pushed credential update to node"
        );

        Ok(())
    }
}
```

#### 3.4.2 `backend/src/services/credential_push_service.rs` -- New orchestration module

Create a new service that encapsulates the logic of "if this service is node-routed, push the credential to the node":

```rust
use std::sync::Arc;

use mongodb::bson::doc;

use crate::crypto::aes::EncryptionKeys;
use crate::errors::AppResult;
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::services::node_ws_manager::{CredentialUpdateParams, NodeWsManager};

/// After an OAuth token is stored or refreshed, check if any UserService
/// referencing this UserApiKey is node-routed. If so, push the credential
/// to the connected node.
///
/// This is fire-and-forget: errors are logged but not propagated.
pub async fn push_credential_to_node_if_routed(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    api_key_id: &str,
) {
    // Find UserServices that reference this api_key and have a node_id
    let services: Vec<UserService> = match db
        .collection::<UserService>(USER_SERVICES)
        .find(doc! {
            "user_id": user_id,
            "api_key_id": api_key_id,
            "node_id": { "$ne": null },
            "is_active": true,
        })
        .await
    {
        Ok(cursor) => {
            use futures::TryStreamExt;
            match cursor.try_collect().await {
                Ok(svcs) => svcs,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to query UserServices for credential push");
                    return;
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to query UserServices for credential push");
            return;
        }
    };

    if services.is_empty() {
        return;
    }

    // Load the UserApiKey to get the decrypted credential
    let api_key = match db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find_one(doc! { "_id": api_key_id })
        .await
    {
        Ok(Some(k)) => k,
        Ok(None) => {
            tracing::warn!(api_key_id = %api_key_id, "UserApiKey not found for credential push");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load UserApiKey for credential push");
            return;
        }
    };

    // Decrypt the credential
    let credential = match decrypt_api_key_credential(&api_key, encryption_keys).await {
        Some(c) => c,
        None => {
            tracing::warn!(api_key_id = %api_key_id, "No credential to push");
            return;
        }
    };

    for svc in &services {
        let node_id = match &svc.node_id {
            Some(id) => id,
            None => continue,
        };

        // Load endpoint URL for target_url field
        let target_url = match db
            .collection::<UserEndpoint>(USER_ENDPOINTS)
            .find_one(doc! { "_id": &svc.endpoint_id })
            .await
        {
            Ok(Some(ep)) if !ep.url.is_empty() => Some(ep.url),
            _ => None,
        };

        let params = build_credential_params(svc, &credential, target_url);

        if let Err(e) = node_ws_manager.send_credential_update(node_id, &params) {
            tracing::warn!(
                node_id = %node_id,
                service_slug = %svc.slug,
                error = %e,
                "Failed to push credential to node (node may be offline)"
            );
        }
    }
}

/// Build CredentialUpdateParams from a UserService and decrypted credential.
fn build_credential_params(
    svc: &UserService,
    credential: &str,
    target_url: Option<String>,
) -> CredentialUpdateParams {
    match svc.auth_method.as_str() {
        "bearer" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
            header_value: Some(format!("Bearer {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        "header" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
        "query" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "query_param".to_string(),
            header_name: None,
            header_value: None,
            param_name: Some(svc.auth_key_name.clone()),
            param_value: Some(credential.to_string()),
            target_url,
        },
        "basic" => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some("Authorization".to_string()),
            header_value: Some(format!("Basic {credential}")),
            param_name: None,
            param_value: None,
            target_url,
        },
        _ => CredentialUpdateParams {
            service_slug: svc.slug.clone(),
            injection_method: "header".to_string(),
            header_name: Some(svc.auth_key_name.clone()),
            header_value: Some(credential.to_string()),
            param_name: None,
            param_value: None,
            target_url,
        },
    }
}

/// Decrypt the active credential from a UserApiKey.
async fn decrypt_api_key_credential(
    api_key: &UserApiKey,
    encryption_keys: &EncryptionKeys,
) -> Option<String> {
    let encrypted = match api_key.credential_type.as_str() {
        "oauth2" => api_key.access_token_encrypted.as_ref(),
        _ => api_key.credential_encrypted.as_ref(),
    }?;

    let decrypted_bytes = match encryption_keys.decrypt(encrypted).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to decrypt credential for push");
            return None;
        }
    };

    String::from_utf8(decrypted_bytes).ok()
}
```

#### 3.4.3 `backend/src/services/mod.rs` -- Register new module

```rust
pub mod credential_push_service;
```

#### 3.4.4 `backend/src/handlers/keys.rs` -- Push after key creation

After `unified_key_service::create_key` succeeds, if the result has a `node_id`, fire-and-forget push the credential:

```rust
// In create_key handler, after the Ok(Json(...)) line:
pub async fn create_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateKeyRequest>,
) -> AppResult<Json<KeyResponse>> {
    let user_id_str = auth_user.user_id.to_string();

    let result = unified_key_service::create_key(/* ... */).await?;

    // Fire-and-forget: push credential to node if routed
    if result.service.node_id.is_some() {
        let db = state.db.clone();
        let enc = state.encryption_keys.clone();
        let ws = state.node_ws_manager.clone();
        let uid = user_id_str.clone();
        let key_id = result.api_key.id.clone();
        tokio::spawn(async move {
            credential_push_service::push_credential_to_node_if_routed(
                &db, &enc, &ws, &uid, &key_id,
            ).await;
        });
    }

    Ok(Json(key_response_from_result(&result)))
}
```

#### 3.4.5 `backend/src/services/oauth_flow.rs` -- Push after token refresh

The `refresh_oauth_token` function is called from `user_token_service::get_active_token` (line 1095). It refreshes the token and updates the DB. However, it operates on the old `UserProviderToken` model, not the new `UserApiKey` model.

**Two integration points are needed:**

**A. New `UserApiKey` refresh path (for unified key system):**

The proxy path `resolve_proxy_target_from_user_service` (proxy_service.rs:277) currently decrypts `UserApiKey.access_token_encrypted` but does NOT check expiration or refresh. We need to add refresh logic here.

Add a new function in `credential_push_service.rs`:

```rust
/// Refresh an OAuth UserApiKey token if expired, then push to node.
/// Called from proxy resolution when the UserApiKey credential_type is "oauth2".
///
/// Returns the fresh access token string.
pub async fn refresh_user_api_key_if_needed(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    api_key: &UserApiKey,
) -> AppResult<Option<String>> {
    // Check if token needs refresh (5-minute buffer)
    let needs_refresh = api_key.expires_at.is_some_and(|exp| {
        exp <= chrono::Utc::now() + chrono::Duration::minutes(5)
    });

    if !needs_refresh {
        return Ok(None); // No refresh needed, caller uses existing token
    }

    let provider_id = match &api_key.provider_config_id {
        Some(id) => id,
        None => return Ok(None), // No provider config, can't refresh
    };

    let refresh_encrypted = match &api_key.refresh_token_encrypted {
        Some(enc) => enc,
        None => return Ok(None), // No refresh token
    };

    // Decrypt refresh token
    let decrypted_rt = zeroize::Zeroizing::new(encryption_keys.decrypt(refresh_encrypted).await?);
    let refresh_token = String::from_utf8((*decrypted_rt).clone())
        .map_err(|e| AppError::Internal(format!("Failed to decode refresh_token: {e}")))?;

    // Load provider config for token_url and client credentials
    // (reuse existing oauth_flow infrastructure)
    let new_access_token = oauth_flow::refresh_user_api_key_token(
        db, encryption_keys, api_key, provider_id, &refresh_token,
    ).await?;

    // Fire-and-forget: push refreshed token to node
    let db_c = db.clone();
    let enc_c = encryption_keys.clone();
    let ws_c = node_ws_manager.clone();
    let uid = api_key.user_id.clone();
    let kid = api_key.id.clone();
    tokio::spawn(async move {
        push_credential_to_node_if_routed(&db_c, &enc_c, &ws_c, &uid, &kid).await;
    });

    Ok(Some(new_access_token))
}
```

**B. Old `UserProviderToken` refresh path (for legacy provider system):**

In the existing `oauth_flow::refresh_oauth_token` function, after successfully refreshing and updating the DB, fire-and-forget push to any node-routed UserService that shares the same `provider_config_id`.

This requires passing `node_ws_manager` into the refresh flow, which currently doesn't have access to it. Instead, handle this at the call site in `proxy_service.rs` or `user_token_service.rs`:

In `proxy_service.rs`, after calling `get_active_token` which may trigger a refresh, check if the service is node-routed and push:

```rust
// In resolve_proxy_target (legacy path), after credential resolution:
// This is handled by the caller in handlers/proxy.rs where AppState is available.
```

The cleanest approach: **add the push at the proxy handler level** where `AppState` (and thus `node_ws_manager`) is available. In `handlers/proxy.rs`, after proxy resolution returns a `UserServiceResolution` with a `node_id`, and the credential was fetched, spawn a background push. This catches both fresh and refreshed tokens.

However, this would push on every request, which is wasteful. Better approach: **add a `was_refreshed: bool` flag** to the refresh return path.

**Simplest approach adopted:** The `credential_push_service::push_credential_to_node_if_routed` function re-reads the `UserApiKey` from DB to get the current credential. So we only need to call it when we know a refresh happened. Integration points:

1. **After key creation** (handlers/keys.rs) -- always push for node-routed keys
2. **After OAuth callback** (handlers/user_tokens.rs) -- push to any UserService with matching provider
3. **After token refresh** (oauth_flow.rs / proxy path) -- push refreshed token

#### 3.4.6 `backend/src/handlers/user_tokens.rs` -- Push after OAuth callback

In `generic_oauth_callback_impl`, after a successful `handle_oauth_callback`, check if there are any `UserService` records with `node_id` for this user + provider, and push:

```rust
// In generic_oauth_callback_impl, inside Ok(token) branch (after audit log):
Ok(token) => {
    audit_service::log_async(/* ... */);

    // Fire-and-forget: push credential to any node-routed UserService
    // that references this provider
    {
        let db = state.db.clone();
        let enc = state.encryption_keys.clone();
        let ws = state.node_ws_manager.clone();
        let uid = token.user_id.clone();
        let pid = provider_id.to_string();
        tokio::spawn(async move {
            credential_push_service::push_oauth_credential_to_nodes(
                &db, &enc, &ws, &uid, &pid,
            ).await;
        });
    }

    // ... existing redirect logic
}
```

New function in `credential_push_service.rs`:

```rust
/// After an OAuth callback stores a token, find any UserService records
/// for this user + provider that are node-routed, and push the credential.
///
/// This bridges the old provider system with the new UserService model:
/// looks up UserApiKey records that have the same provider_config_id.
pub async fn push_oauth_credential_to_nodes(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>,
    user_id: &str,
    provider_config_id: &str,
) {
    // Find UserApiKeys linked to this provider
    let api_keys: Vec<UserApiKey> = match db
        .collection::<UserApiKey>(USER_API_KEYS)
        .find(doc! {
            "user_id": user_id,
            "provider_config_id": provider_config_id,
            "status": "active",
        })
        .await
    {
        Ok(cursor) => {
            use futures::TryStreamExt;
            cursor.try_collect().await.unwrap_or_default()
        }
        Err(_) => return,
    };

    for api_key in &api_keys {
        push_credential_to_node_if_routed(
            db, encryption_keys, node_ws_manager, user_id, &api_key.id,
        ).await;
    }
}
```

#### 3.4.7 `backend/src/services/proxy_service.rs` -- Push after inline token refresh

In `resolve_proxy_target_from_user_service`, when we detect an expired OAuth token, call the refresh-and-push function. This requires passing `node_ws_manager` through.

Update the function signature:

```rust
pub async fn resolve_proxy_target_from_user_service(
    db: &mongodb::Database,
    encryption_keys: &EncryptionKeys,
    node_ws_manager: &Arc<NodeWsManager>, // NEW parameter
    user_id: &str,
    slug: Option<&str>,
    catalog_service_id: Option<&str>,
) -> AppResult<Option<UserServiceResolution>> {
```

In the OAuth credential resolution section (around line 354-369), add refresh check:

```rust
"oauth2" => {
    // Check if token needs refresh
    if let Some(refreshed) = credential_push_service::refresh_user_api_key_if_needed(
        db, encryption_keys, node_ws_manager, &api_key,
    ).await? {
        refreshed // Use refreshed token
    } else {
        // Decrypt existing token (existing code)
        let encrypted = api_key.access_token_encrypted.as_ref().ok_or_else(|| { ... })?;
        let decrypted_bytes = Zeroizing::new(encryption_keys.decrypt(encrypted).await?);
        String::from_utf8((*decrypted_bytes).clone()).map_err(|e| { ... })?
    }
}
```

Update all call sites of `resolve_proxy_target_from_user_service` to pass `node_ws_manager`:
- `handlers/proxy.rs` (has access via `state.node_ws_manager`)

### 3.5 Node Agent Changes

#### 3.5.1 `node-agent/src/ws_client.rs` -- Handle `credential_update` message

Add a new match arm in the message dispatch loop (around line 397-499):

```rust
Some("credential_update") => {
    let tx_clone = tx.clone();
    let cred_sender = credential_sender.clone();
    let config_path = config_path.clone();
    let backend = secret_backend.clone();
    tokio::spawn(async move {
        handle_credential_update(
            &parsed,
            &cred_sender,
            &config_path,
            &backend,
            &tx_clone,
        ).await;
    });
}
```

New handler function:

```rust
async fn handle_credential_update(
    parsed: &serde_json::Value,
    credential_sender: &SharedCredentialsSender,
    config_path: &Path,
    backend: &SecretBackend,
    tx: &mpsc::Sender<String>,
) {
    let service_slug = match parsed["service_slug"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!("credential_update missing service_slug");
            return;
        }
    };

    let injection_method = parsed["injection_method"].as_str().unwrap_or("header");

    let result = match injection_method {
        "header" => {
            let header_name = parsed["header_name"].as_str().unwrap_or("Authorization");
            let header_value = match parsed["header_value"].as_str() {
                Some(v) => v,
                None => {
                    tracing::warn!(slug = %service_slug, "credential_update missing header_value");
                    send_credential_ack(tx, service_slug, "error", Some("missing header_value")).await;
                    return;
                }
            };
            let target_url = parsed["target_url"].as_str();

            update_header_credential(
                service_slug, header_name, header_value, target_url,
                credential_sender, config_path, backend,
            )
        }
        "query_param" => {
            let param_name = match parsed["param_name"].as_str() {
                Some(n) => n,
                None => {
                    tracing::warn!(slug = %service_slug, "credential_update missing param_name");
                    send_credential_ack(tx, service_slug, "error", Some("missing param_name")).await;
                    return;
                }
            };
            let param_value = match parsed["param_value"].as_str() {
                Some(v) => v,
                None => {
                    tracing::warn!(slug = %service_slug, "credential_update missing param_value");
                    send_credential_ack(tx, service_slug, "error", Some("missing param_value")).await;
                    return;
                }
            };
            let target_url = parsed["target_url"].as_str();

            update_query_param_credential(
                service_slug, param_name, param_value, target_url,
                credential_sender, config_path, backend,
            )
        }
        other => {
            tracing::warn!(method = %other, "Unknown injection_method in credential_update");
            send_credential_ack(tx, service_slug, "error", Some("unknown injection_method")).await;
            return;
        }
    };

    match result {
        Ok(()) => {
            tracing::info!(slug = %service_slug, "Credential updated via server push");
            send_credential_ack(tx, service_slug, "ok", None).await;
        }
        Err(e) => {
            tracing::error!(slug = %service_slug, error = %e, "Failed to update credential");
            send_credential_ack(tx, service_slug, "error", Some(&e.to_string())).await;
        }
    }
}

fn update_header_credential(
    service_slug: &str,
    header_name: &str,
    header_value: &str,
    target_url: Option<&str>,
    credential_sender: &SharedCredentialsSender,
    config_path: &Path,
    backend: &SecretBackend,
) -> Result<()> {
    // 1. Update config on disk
    let mut config = NodeConfig::load(config_path)?;
    config.add_header_credential_via(
        service_slug, header_name, header_value, target_url, backend,
    )?;
    config.save(config_path)?;

    // 2. Rebuild credential store from updated config and push to watch channel
    let new_store = CredentialStore::from_config_with_backend(&config, backend)?;
    credential_sender.update(new_store);

    Ok(())
}

fn update_query_param_credential(
    service_slug: &str,
    param_name: &str,
    param_value: &str,
    target_url: Option<&str>,
    credential_sender: &SharedCredentialsSender,
    config_path: &Path,
    backend: &SecretBackend,
) -> Result<()> {
    // 1. Update config on disk
    let mut config = NodeConfig::load(config_path)?;
    config.add_query_param_credential_via(
        service_slug, param_name, param_value, target_url, backend,
    )?;
    config.save(config_path)?;

    // 2. Rebuild credential store from updated config and push to watch channel
    let new_store = CredentialStore::from_config_with_backend(&config, backend)?;
    credential_sender.update(new_store);

    Ok(())
}

async fn send_credential_ack(
    tx: &mpsc::Sender<String>,
    service_slug: &str,
    status: &str,
    error: Option<&str>,
) {
    let mut ack = serde_json::json!({
        "type": "credential_update_ack",
        "service_slug": service_slug,
        "status": status,
    });
    if let Some(e) = error {
        ack["error"] = serde_json::Value::String(e.to_string());
    }
    let _ = send_ws_message(tx, ack.to_string()).await;
}
```

#### 3.5.2 `node-agent/src/ws_client.rs` -- Thread `SharedCredentialsSender` through

The `run_websocket_loop` function currently receives `SharedCredentials` (the receiver half). For credential updates, it also needs the `SharedCredentialsSender` to push updates.

Update the function signature of `run_websocket_loop` (or the relevant inner connection function) to also accept `SharedCredentialsSender`:

```rust
pub async fn run_websocket_loop(
    config: &NodeConfig,
    config_path: &Path,
    credentials: SharedCredentials,
    credential_sender: SharedCredentialsSender, // NEW
    backend: &SecretBackend,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
```

The caller in `main.rs` (the `cmd_start` function) creates `SharedCredentials::new(initial_store)` which returns `(sender, receiver)`. Currently only the receiver is passed. Pass the sender as well.

#### 3.5.3 `node-agent/src/ws_client.rs` -- Handle `credential_update_ack` on backend

In `backend/src/handlers/node_ws.rs` (the WS reader task), add handling for `credential_update_ack` messages from nodes:

```rust
"credential_update_ack" => {
    let slug = msg["service_slug"].as_str().unwrap_or("unknown");
    let status = msg["status"].as_str().unwrap_or("unknown");
    if status == "ok" {
        tracing::info!(node_id = %node_id, slug = %slug, "Node acknowledged credential update");
    } else {
        let error = msg["error"].as_str().unwrap_or("unknown");
        tracing::warn!(
            node_id = %node_id, slug = %slug, error = %error,
            "Node failed to apply credential update"
        );
    }
}
```

### 3.6 Security Considerations

1. **Credential in transit:** The credential value is sent in plaintext over the WebSocket. This is acceptable because the WS connection is already TLS-encrypted (wss://). The node auth token authenticates the connection.

2. **Credential at rest on node:** The node agent encrypts credentials with AES-256-GCM before writing to disk (via `SecretBackend`), same as credentials added via CLI.

3. **Authorization:** Only the NyxID server can send `credential_update` messages (the node only accepts messages from the authenticated server connection). The server only pushes credentials that belong to the user who owns the node (verified by `UserService.user_id` matching).

4. **Replay protection:** Not needed for `credential_update` since it's idempotent (last write wins) and only flows server-to-node.

5. **Rate limiting:** The push is fire-and-forget and bounded by the WS channel capacity (256). No additional rate limiting needed.

---

## 4. Complete File List

### Backend

| File | Action | Description |
|---|---|---|
| `backend/src/services/node_ws_manager.rs` | Modify | Add `WsCredentialUpdate` struct, `CredentialUpdateParams`, `send_credential_update()` method |
| `backend/src/services/credential_push_service.rs` | **New** | Orchestration: `push_credential_to_node_if_routed()`, `push_oauth_credential_to_nodes()`, `refresh_user_api_key_if_needed()` |
| `backend/src/services/mod.rs` | Modify | Add `pub mod credential_push_service;` |
| `backend/src/handlers/keys.rs` | Modify | After create_key, fire-and-forget push to node |
| `backend/src/handlers/user_tokens.rs` | Modify | After OAuth callback success, fire-and-forget push to node |
| `backend/src/services/proxy_service.rs` | Modify | Add `node_ws_manager` param to `resolve_proxy_target_from_user_service`, refresh+push for OAuth tokens |
| `backend/src/handlers/proxy.rs` | Modify | Pass `state.node_ws_manager` to `resolve_proxy_target_from_user_service` |
| `backend/src/handlers/node_ws.rs` | Modify | Handle `credential_update_ack` messages from nodes |

### Node Agent

| File | Action | Description |
|---|---|---|
| `node-agent/src/ws_client.rs` | Modify | Handle `credential_update` message, add `handle_credential_update()`, `send_credential_ack()`, thread `SharedCredentialsSender` |
| `node-agent/src/main.rs` | Modify | Pass `SharedCredentialsSender` to `run_websocket_loop` |

### Frontend

| File | Action | Description |
|---|---|---|
| `frontend/src/components/dashboard/add-key-dialog.tsx` | Modify | Add `RoutingStep` component, `nodeId` to FormState, routing step in wizard flow, auto-fill `endpointUrl` from catalog, show endpoint URL for all catalog services |
| `frontend/src/hooks/use-keys.ts` | Modify | Add `node_id` to `CreateKeyParams` type |

### Documentation

| File | Action | Description |
|---|---|---|
| `docs/IMPLEMENTATION_SPEC_V5.md` | **New** | This spec |

---

## 5. Implementation Order

1. **Phase A: Auto-fill base URL** (smallest, no dependencies)
   - Modify `add-key-dialog.tsx` to auto-fill `endpointUrl` from catalog

2. **Phase B: Node selection at creation** (frontend only)
   - Add `RoutingStep` component
   - Add `nodeId` to form state
   - Update wizard flow
   - Update `use-keys.ts` types

3. **Phase C: Credential push infrastructure** (backend + node-agent)
   - Add `send_credential_update` to `node_ws_manager.rs`
   - Create `credential_push_service.rs`
   - Handle `credential_update` in node agent `ws_client.rs`
   - Thread `SharedCredentialsSender` through node agent
   - Handle `credential_update_ack` in backend

4. **Phase D: Credential push integration** (wire up to OAuth and proxy flows)
   - Push after key creation (`handlers/keys.rs`)
   - Push after OAuth callback (`handlers/user_tokens.rs`)
   - Push after token refresh (`proxy_service.rs`)
