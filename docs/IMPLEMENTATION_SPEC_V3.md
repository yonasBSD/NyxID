> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V3: Merge Agent Groups into API Keys, Sidebar Cleanup, Auto-Connect

This spec builds on `IMPLEMENTATION_SPEC.md` (Phase 0) and `IMPLEMENTATION_SPEC_V2.md` (Phase 1). It covers five requirements:

1. **Merge Agent Groups into NyxID API Keys** -- eliminate the separate AgentGroup model
2. **Remove Services/Connections/Providers from normal user sidebar**
3. **Auto-connect model** -- configured credential = connected, no separate step
4. **Fix auth_key_name missing in frontend** for non-custom catalog services
5. **Reference service account model** for design pattern

---

## Table of Contents

1. [Design Decision: Absorb AgentGroup into ApiKey](#1-design-decision-absorb-agentgroup-into-apikey)
2. [API Key Scope Model](#2-api-key-scope-model)
3. [Backend Changes](#3-backend-changes)
4. [Frontend Page Restructure](#4-frontend-page-restructure)
5. [Sidebar Cleanup](#5-sidebar-cleanup)
6. [Auto-Connect Logic](#6-auto-connect-logic)
7. [Auth Key Name Fix](#7-auth-key-name-fix)
8. [Complete File List](#8-complete-file-list)

---

## 1. Design Decision: Absorb AgentGroup into ApiKey

### Analysis

The current v2 design has:
- `AgentGroup` (collection: `agent_groups`) -- holds scope (allowed_service_ids, allowed_node_ids, allow_all flags)
- `ApiKey` (collection: `api_keys`) -- holds auth (key_hash, prefix, scopes, expiry) + optional `agent_group_id` FK

The user says these are the same concept: an API key optionally has scoped access to specific external services. A separate collection adds indirection without benefit.

### Decision: Absorb scope fields directly into `ApiKey`

**Why absorb rather than keep separate?**
- Service accounts use a similar pattern: `ServiceAccount` has `allowed_scopes` directly on the model, not in a separate "scope" collection.
- The 1:1 relationship between AgentGroup and ApiKey means there's no cardinality benefit to a separate collection.
- Eliminates a join on every proxy request (currently: load ApiKey -> check agent_group_id -> load AgentGroup -> check scope).
- Simpler UI: one entity to display, not two.

**Migration path:** The `agent_groups` collection is new from v2 and may have zero production data (or very little). We delete the collection and model entirely.

### New `ApiKey` Model

```rust
// backend/src/models/api_key.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;

pub const COLLECTION_NAME: &str = "api_keys";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    pub name: String,
    /// First 8+ characters of the key, used for identification in the UI
    pub key_prefix: String,
    /// SHA-256 hash of the full API key
    pub key_hash: String,
    pub scopes: String,
    #[serde(default, with = "bson_datetime::optional")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, with = "bson_datetime::optional")]
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,

    // --- Service Scope (absorbed from AgentGroup) ---

    /// Optional description of what this key is used for
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// List of UserService IDs this key can access via proxy.
    /// Only checked when `allow_all_services` is false.
    #[serde(default)]
    pub allowed_service_ids: Vec<String>,

    /// List of Node IDs this key can route through.
    /// Only checked when `allow_all_nodes` is false.
    #[serde(default)]
    pub allowed_node_ids: Vec<String>,

    /// If true, key can access ALL of the user's external services.
    /// Default: true (backward compatible -- existing keys have no restrictions).
    #[serde(default = "default_true")]
    pub allow_all_services: bool,

    /// If true, key can route through ALL of the user's nodes.
    /// Default: true (backward compatible).
    #[serde(default = "default_true")]
    pub allow_all_nodes: bool,
}

fn default_true() -> bool {
    true
}
```

**Key design choices:**
- `allow_all_services` and `allow_all_nodes` default to `true`. This means all existing API keys (which had no scope) continue to work unrestricted -- backward compatible.
- The `agent_group_id` field is **removed** entirely.
- `description` is added (was on AgentGroup, useful for any key).
- Keys with `nyxid_ag_` prefix continue to work; the prefix is just cosmetic.

---

## 2. API Key Scope Model

### How Scope Works

Every NyxID API key now optionally has a service scope:

| `allow_all_services` | `allowed_service_ids` | Behavior |
|:---:|:---:|---|
| `true` | (ignored) | Key can proxy through ANY of the user's configured external services |
| `false` | `["svc-1", "svc-2"]` | Key can ONLY proxy through those specific UserService records |
| `false` | `[]` | Key cannot proxy at all (auth-only key) |

Same logic for nodes:

| `allow_all_nodes` | `allowed_node_ids` | Behavior |
|:---:|:---:|---|
| `true` | (ignored) | Key can route through ANY of the user's nodes |
| `false` | `["node-1"]` | Key can ONLY route through those specific nodes |
| `false` | `[]` | Key can only use direct routing (no nodes) |

### Proxy Enforcement

The proxy handler (`handlers/proxy.rs`) already has a `PreResolved` struct with `user_service_id`. The scope check replaces the old `agent_group_service::check_agent_access`:

```rust
// In proxy handler, after resolving user_service:
if let Some(ref user_service_id) = pre_resolved.user_service_id {
    if !auth_user_api_key.allow_all_services
        && !auth_user_api_key.allowed_service_ids.contains(user_service_id)
    {
        return Err(AppError::AgentGroupForbidden(
            "API key does not have access to this service".to_string(),
        ));
    }
}
if let Some(ref node_id) = pre_resolved.node_id {
    if !auth_user_api_key.allow_all_nodes
        && !auth_user_api_key.allowed_node_ids.contains(node_id)
    {
        return Err(AppError::AgentGroupForbidden(
            "API key does not have access to this node".to_string(),
        ));
    }
}
```

### AuthUser Changes

The `AuthUser` struct currently carries `agent_group_id: Option<String>`. This gets replaced with the scope fields from the ApiKey:

```rust
// In mw/auth.rs, AuthUser struct:
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    pub scope: String,
    pub acting_client_id: Option<String>,
    pub approval_owner_user_id: Option<String>,
    pub auth_method: AuthMethod,
    // REMOVED: pub agent_group_id: Option<String>,
    // NEW: scope fields from ApiKey (only populated for API key auth)
    pub allow_all_services: bool,
    pub allow_all_nodes: bool,
    pub allowed_service_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
}
```

When authenticating via API key, populate these from the `ApiKey` record. For session/JWT/SA auth, default to `allow_all_services: true, allow_all_nodes: true` (no restrictions).

---

## 3. Backend Changes

### 3.1 Model Changes

**`backend/src/models/api_key.rs`** -- Replace entirely with new model (see section 1 above).

**Remove:**
- `backend/src/models/agent_group.rs` -- DELETE entirely

**`backend/src/models/mod.rs`** -- Remove `pub mod agent_group;`

### 3.2 Service Changes

**`backend/src/services/key_service.rs`** -- Major changes:

1. **Remove** `create_api_key_for_agent_group` function. Merge its logic into the main `create_api_key`:

```rust
pub async fn create_api_key(
    db: &mongodb::Database,
    user_id: &str,
    name: &str,
    scopes: &str,
    expires_at: Option<DateTime<Utc>>,
    // NEW optional scope params:
    description: Option<&str>,
    allowed_service_ids: Option<&[String]>,
    allowed_node_ids: Option<&[String]>,
    allow_all_services: Option<bool>,
    allow_all_nodes: Option<bool>,
) -> AppResult<CreatedApiKey>
```

If `allowed_service_ids` is provided (even empty) OR `allow_all_services` is explicitly `Some(false)`, generate key with `nyxid_ag_` prefix. Otherwise use `nyxid_` prefix. This preserves the visual distinction.

2. **Add** `update_api_key_scope` function:

```rust
pub async fn update_api_key_scope(
    db: &Database,
    user_id: &str,
    key_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    allowed_service_ids: Option<&[String]>,
    allowed_node_ids: Option<&[String]>,
    allow_all_services: Option<bool>,
    allow_all_nodes: Option<bool>,
) -> AppResult<ApiKey>
```

3. **Keep** `list_api_keys`, `delete_api_key`, `rotate_api_key`, `validate_api_key` -- minor adjustments to include new fields.

4. **Modify** `list_api_keys` to optionally filter by key type:

```rust
pub async fn list_api_keys(
    db: &Database,
    user_id: &str,
    include_agent_keys: bool,
) -> AppResult<Vec<ApiKey>>
```

If `include_agent_keys` is false, filter out keys where `allow_all_services == false` (i.e., scoped keys). This lets the "NyxID API Keys" tab show only unscoped keys, and a future UI could separate them.

Actually, simpler: just return all keys. The frontend can filter by prefix or scope presence.

**Remove:**
- `backend/src/services/agent_group_service.rs` -- DELETE entirely

**`backend/src/services/mod.rs`** -- Remove `pub mod agent_group_service;`

### 3.3 Handler Changes

**Remove:**
- `backend/src/handlers/agent_groups.rs` -- DELETE entirely

**`backend/src/handlers/mod.rs`** -- Remove `pub mod agent_groups;`

**`backend/src/handlers/user_api_keys_external.rs`** -- No changes (handles external user_api_keys, separate concept).

**`backend/src/handlers/keys.rs`** -- The existing unified key handler. No changes needed.

**New: Add scope management to existing API key handlers**

The existing NyxID API key handlers (in the handler that serves `/api/v1/api-keys`) need:
- **Create**: Accept optional scope fields (`description`, `allowed_service_ids`, `allowed_node_ids`, `allow_all_services`, `allow_all_nodes`)
- **Update**: New `PUT /api/v1/api-keys/{key_id}` endpoint to update scope
- **Get**: Return enriched scope info (resolved service names, node names)

```rust
// Add to the existing api_keys handler module:

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: String,
    pub expires_at: Option<DateTime<Utc>>,
    // NEW scope fields:
    pub description: Option<String>,
    #[serde(default)]
    pub allowed_service_ids: Vec<String>,
    #[serde(default)]
    pub allowed_node_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_all_services: bool,
    #[serde(default = "default_true")]
    pub allow_all_nodes: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub allowed_service_ids: Option<Vec<String>>,
    pub allowed_node_ids: Option<Vec<String>>,
    pub allow_all_services: Option<bool>,
    pub allow_all_nodes: Option<bool>,
}

// Response includes enriched scope info
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub key_prefix: String,
    pub scopes: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    // Scope
    pub allowed_service_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub allow_all_services: bool,
    pub allow_all_nodes: bool,
    // Enriched
    pub allowed_services: Vec<AllowedServiceInfo>,
    pub allowed_nodes: Vec<AllowedNodeInfo>,
}

/// PUT /api/v1/api-keys/{key_id}
pub async fn update_api_key(...) -> AppResult<Json<ApiKeyResponse>>
```

### 3.4 Route Changes

**`backend/src/routes.rs`:**
- **Remove** the `/agent-groups` route nest entirely
- **Add** `PUT` method to the existing `/api-keys/{key_id}` route

```rust
// REMOVE:
// .nest("/agent-groups", agent_group_routes)

// MODIFY existing api-keys routes:
.route(
    "/{key_id}",
    get(handlers::api_keys::get_api_key)   // NEW
        .put(handlers::api_keys::update_api_key)  // NEW
        .delete(handlers::api_keys::delete_api_key),
)
```

### 3.5 Database Changes

**`backend/src/db.rs`:**
- **Remove** the `agent_groups` index creation block
- No new indexes needed (ApiKey already has user_id index)

### 3.6 Auth Middleware

**`backend/src/mw/auth.rs`:**
- Remove `agent_group_id` from `AuthUser`
- Add `allow_all_services`, `allow_all_nodes`, `allowed_service_ids`, `allowed_node_ids`
- When authenticating via API key, populate from the `ApiKey` record
- For all other auth methods, set `allow_all_services: true, allow_all_nodes: true, allowed_*: vec![]`

### 3.7 Proxy Handler

**`backend/src/handlers/proxy.rs`:**
- Remove `use crate::services::agent_group_service`
- Replace the `agent_group_service::check_agent_access(...)` call with inline scope check using `auth_user.allow_all_services`, `auth_user.allowed_service_ids`, etc.

### 3.8 Error Codes

Keep the existing error variants and codes (9000-9002). Just update the messages:
- `AgentGroupForbidden` -> used as "API key does not have access to this service/node"
- `AgentGroupInactive` -> can be removed (inactive keys are already caught by `validate_api_key`)
- `AgentGroupNotFound` -> can be removed (no separate group to look up)

Simplification: Remove `AgentGroupInactive` and `AgentGroupNotFound`, keep only `AgentGroupForbidden` (rename to `ApiKeyScopeForbidden` if desired, but that changes error codes). Safer: keep the variant name for backward compatibility, just change the message strings.

---

## 4. Frontend Page Restructure

### 4.1 Tab Structure: 2 Tabs

The AI Services page (`/keys`) changes from 3 tabs to 2:

```
+------------------------------------------------------------------+
|  AI Services                                    [+ Add Service]  |
|                                                                  |
|  [External Services]  [NyxID API Keys]                           |
|  ~~~~~~~~~~~~~~~~~~                                              |
|                                                                  |
|  +------------------+  +------------------+  +----------------+  |
|  | OpenAI API       |  | Anthropic        |  | Custom API     |  |
|  | Active           |  | Active           |  | Active         |  |
|  | api.openai.com   |  | api.anthropic.com|  | internal.co/api|  |
|  +------------------+  +------------------+  +----------------+  |
+------------------------------------------------------------------+

Switching to "NyxID API Keys" tab:

+------------------------------------------------------------------+
|  AI Services                                    [+ Create Key]   |
|                                                                  |
|  [External Services]  [NyxID API Keys]                           |
|                       ~~~~~~~~~~~~~~~~                           |
|                                                                  |
|  +---------------------------------------------------------------+
|  | Name       | Prefix      | Scopes | Services    | Actions    |
|  |------------|-------------|--------|-------------|------------|
|  | Production | nyxid_ab..  | read   | All         | [Edit]     |
|  | CI Agent   | nyxid_ag_cd | proxy  | 3 services  | [Edit]     |
|  | Read Only  | nyxid_ef..  | read   | All         | [Edit]     |
|  +---------------------------------------------------------------+
+------------------------------------------------------------------+
```

All API keys (regular `nyxid_` and scoped `nyxid_ag_`) appear in the same table. Scoped keys show their service count; unscoped keys show "All".

### 4.2 Keys Page Changes (`pages/keys.tsx`)

```tsx
type TabValue = "services" | "nyxid";  // Remove "agents"

// Remove AgentGroupsTab
// Remove CreateAgentGroupDialog import and state
// Remove "Agent Groups" TabsTrigger
// Remove "Agent Groups" TabsContent
```

The `AddButton` for the "nyxid" tab opens a new `ApiKeyCreateDialog` that includes optional scope configuration.

### 4.3 API Key Table Enhancement

The existing `ApiKeyTable` component shows: name, prefix, scopes, last_used, actions.

Add a "Services" column:
- If `allow_all_services`: show "All services"
- If `!allow_all_services` and `allowed_service_ids.length > 0`: show "{N} services"
- If `!allow_all_services` and `allowed_service_ids.length === 0`: show "None"

### 4.4 API Key Create Dialog Enhancement

The existing `ApiKeyCreateDialog` gets a new optional section: "Service Scope".

```
+-----------------------------------------+
|  Create NyxID API Key                   |
|                                         |
|  Name: [________________]               |
|  Scopes: [read] [write] [proxy]         |
|  Expiration: [optional date picker]     |
|                                         |
|  -- Service Scope (optional) ---------- |
|  [ ] Restrict to specific services      |
|                                         |
|  If checked:                            |
|  Select services:                       |
|  [x] OpenAI API                         |
|  [ ] Anthropic                          |
|  [x] Custom API                         |
|                                         |
|  -- Node Scope (optional) ------------- |
|  [ ] Restrict to specific nodes         |
|  ...                                    |
|                                         |
|  [Create Key]                           |
+-----------------------------------------+
```

When "Restrict to specific services" is unchecked: `allow_all_services: true` (default).
When checked: `allow_all_services: false`, `allowed_service_ids: [selected IDs]`.

### 4.5 API Key Detail Page (New)

Route: `/keys/api-key/$keyId` (replaces `/keys/agent-group/$groupId`)

Shows:
- Name, description (editable)
- Key prefix, scopes, last used, expiry
- Service scope editor (toggle allow-all, pick services)
- Node scope editor (toggle allow-all, pick nodes)
- Rotate key button
- Delete button

This replaces the `AgentGroupDetailPage`.

### 4.6 Remove Agent Group Files

Delete:
- `frontend/src/pages/agent-group-detail.tsx`
- `frontend/src/components/dashboard/agent-group-card.tsx`
- `frontend/src/components/dashboard/create-agent-group-dialog.tsx`
- `frontend/src/hooks/use-agent-groups.ts`
- `frontend/src/types/agent-groups.ts`

### 4.7 Router Changes

```diff
- const agentGroupDetailRoute = createRoute({
-   path: "/keys/agent-group/$groupId",
-   ...
-   component: AgentGroupDetailPage,
- });

+ const apiKeyDetailRoute = createRoute({
+   path: "/keys/api-key/$keyId",
+   getParentRoute: () => dashboardLayout,
+   component: ApiKeyDetailPage,
+ });
```

Update the route tree: remove `agentGroupDetailRoute`, add `apiKeyDetailRoute`.

### 4.8 Update Types

**`frontend/src/types/keys.ts`** -- Add to existing:

```typescript
export interface NyxIdApiKeyInfo {
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly key_prefix: string;
  readonly scopes: string;
  readonly last_used_at: string | null;
  readonly expires_at: string | null;
  readonly is_active: boolean;
  readonly created_at: string;
  readonly allowed_service_ids: readonly string[];
  readonly allowed_node_ids: readonly string[];
  readonly allow_all_services: boolean;
  readonly allow_all_nodes: boolean;
  readonly allowed_services: readonly AllowedServiceInfo[];
  readonly allowed_nodes: readonly AllowedNodeInfo[];
}

export interface AllowedServiceInfo {
  readonly id: string;
  readonly slug: string;
  readonly label: string;
  readonly catalog_service_name: string | null;
}

export interface AllowedNodeInfo {
  readonly id: string;
  readonly name: string;
  readonly status: string;
}
```

**Remove** `frontend/src/types/agent-groups.ts` entirely.

---

## 5. Sidebar Cleanup

### Current State

The sidebar (`sidebar.tsx`) has these items in `NAV_ITEMS`:

```typescript
const NAV_ITEMS = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
  { to: "/keys", icon: Cable, label: "AI Services" },
  { to: "/services", icon: Server, label: "Services" },         // REMOVE
  { to: "/connections", icon: Link2, label: "Connections" },     // REMOVE
  { to: "/providers", icon: Plug, label: "Providers" },          // REMOVE
  { to: "/nodes", icon: HardDrive, label: "Nodes" },
  { to: "/settings", icon: Settings, label: "Settings" },
  { to: "/settings/consents", icon: KeyRound, label: "Authorized Apps" },
  { to: "/guide", icon: BookOpen, label: "Guide" },
];
```

### Target State

Remove Services, Connections, and Providers from the normal user sidebar. These are admin catalog management pages.

```typescript
const NAV_ITEMS = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
  { to: "/keys", icon: Cable, label: "AI Services" },
  { to: "/nodes", icon: HardDrive, label: "Nodes" },
  { to: "/settings", icon: Settings, label: "Settings" },
  { to: "/settings/consents", icon: KeyRound, label: "Authorized Apps" },
  { to: "/guide", icon: BookOpen, label: "Guide" },
];
```

### Admin Section

Add Services/Providers to the admin nav:

```typescript
const ADMIN_NAV_ITEMS = [
  { to: "/admin/users", icon: Users, label: "Users" },
  { to: "/admin/service-accounts", icon: Bot, label: "Service Accounts" },
  { to: "/admin/roles", icon: ShieldCheck, label: "Roles" },
  { to: "/admin/groups", icon: UsersRound, label: "Groups" },
  { to: "/admin/nodes", icon: HardDrive, label: "Nodes" },
  { to: "/admin/services", icon: Server, label: "Services" },       // NEW
  { to: "/admin/providers", icon: Plug, label: "Providers" },       // NEW
];
```

### Router Changes for Admin Service/Provider Pages

The existing `/services` and `/providers` routes stay accessible (they're already functional pages). We add admin-prefixed routes that point to the same components, and redirect the old paths:

```typescript
// Redirect old paths to admin
const servicesRedirectRoute = createRoute({
  path: "/services",
  getParentRoute: () => dashboardLayout,
  beforeLoad: () => {
    const { user } = useAuthStore.getState();
    if (user?.is_admin) {
      throw redirect({ to: "/admin/services" });
    }
    // Non-admin: redirect to AI Services
    throw redirect({ to: "/keys" });
  },
  component: () => null,
});

const connectionsRedirectRoute = createRoute({
  path: "/connections",
  getParentRoute: () => dashboardLayout,
  beforeLoad: () => {
    throw redirect({ to: "/keys" });
  },
  component: () => null,
});

const providersRedirectRoute = createRoute({
  path: "/providers",
  getParentRoute: () => dashboardLayout,
  beforeLoad: () => {
    const { user } = useAuthStore.getState();
    if (user?.is_admin) {
      throw redirect({ to: "/admin/providers" });
    }
    throw redirect({ to: "/keys" });
  },
  component: () => null,
});
```

For the admin routes, reuse the existing page components:

```typescript
const adminServicesLayout = createRoute({
  path: "services",
  getParentRoute: () => adminLayout,
  component: ServicesPage,
});

const adminServicesIndexRoute = createRoute({
  path: "/",
  getParentRoute: () => adminServicesLayout,
  component: ServiceListPage,
});

// ... service detail, edit routes under admin

const adminProvidersLayout = createRoute({
  path: "providers",
  getParentRoute: () => adminLayout,
  component: ProvidersLayout,
});
// ... provider sub-routes under admin
```

Note: The Connections page (`/connections`) is fully replaced by AI Services (`/keys`). It can be redirected without an admin equivalent.

---

## 6. Auto-Connect Logic

### Current State

Currently, when a user adds an external service credential via `POST /api/v1/keys`, the system auto-provisions a `UserEndpoint` + `UserApiKey` + `UserService`. This IS the auto-connect model already -- there's no separate "connect" step for the new collections.

The old model (`/connections`, `/providers/*/connect/*`) had a separate concept of "connecting" to a service. That's what the user wants to eliminate entirely.

### What Changes

1. **No changes to the new unified key system.** `POST /api/v1/keys` already auto-provisions everything. A configured service IS connected.

2. **Clarify in the UI:** The External Services tab shows configured services. Each card represents a "connected" service. There's no "connect/disconnect" toggle -- adding a key connects, deleting the key disconnects.

3. **API key scope = access control:** If an API key has `allow_all_services: true`, it can proxy through any configured service. If `allow_all_services: false` with specific `allowed_service_ids`, only those.

4. **No separate connection concept in proxy resolution:** The proxy already resolves by UserService. If a UserService exists and is active, it's proxyable. No separate "is_connected" check.

### Summary

The auto-connect model is already implemented in v1/v2 for the new collections. The only action needed is removing the old Services/Connections/Providers pages from the normal user sidebar (section 5 above), which eliminates user confusion about "connecting" vs "configuring."

---

## 7. Auth Key Name Fix

### Problem

When a user selects a catalog service (not "Custom Endpoint") in `AddKeyDialog`, the form only shows:
1. Label input
2. API Key / Credential input
3. Endpoint URL (only if `requires_gateway_url`)

It does NOT show:
- Auth Method selector
- Auth Key Name input

For catalog services, these default to the catalog's values (e.g., OpenAI defaults to `bearer` / `Authorization`). This is correct for most services. However, some catalog services may use `header` / `X-API-Key` or other non-default auth configs. The user should be able to override these.

### Current Code (add-key-dialog.tsx)

```tsx
// Line 216-262: Auth method/key name fields only shown when isCustom === true
{isCustom && (
  <>
    <div className="space-y-1.5">
      <Label>Slug</Label>
      ...
    </div>
    <div className="grid grid-cols-2 gap-3">
      <div>Auth Method</div>
      <div>Auth Key Name</div>
    </div>
  </>
)}
```

### Fix

Show auth method and auth key name for ALL services (catalog and custom), not just custom. For catalog services, pre-populate with catalog defaults but allow override. For services with `auth_method: "none"`, skip these fields entirely.

```tsx
// Replace the isCustom check with:
const showAuthConfig = isCustom || (catalogEntry?.auth_method !== "none");

{showAuthConfig && (
  <>
    {isCustom && (
      <div className="space-y-1.5">
        <Label>Slug</Label>
        ...
      </div>
    )}
    <div className="grid grid-cols-2 gap-3">
      <div className="space-y-1.5">
        <Label>Auth Method</Label>
        <Select
          value={form.authMethod}
          onValueChange={(v) => onChange({ authMethod: v })}
        >
          ...
        </Select>
      </div>
      <div className="space-y-1.5">
        <Label>Auth Key Name</Label>
        <Input
          value={form.authKeyName}
          onChange={(e) => onChange({ authKeyName: e.target.value })}
        />
      </div>
    </div>
  </>
)}
```

Also update `handleSelectCatalog` to pre-populate auth fields from catalog entry:

```tsx
function handleSelectCatalog(entry: CatalogEntry) {
  setSelectedEntry(entry);
  setForm({
    ...INITIAL_FORM,
    label: entry.name,
    authMethod: entry.auth_method ?? "bearer",
    authKeyName: entry.auth_key_name ?? "Authorization",
  });
  setStep("form");
}
```

And include `auth_method` and `auth_key_name` in the catalog submission params:

```tsx
const params = selectedEntry
  ? {
      credential: form.credential,
      label: form.label,
      service_slug: selectedEntry.slug,
      ...(form.endpointUrl.trim() ? { endpoint_url: form.endpointUrl.trim() } : {}),
      // Include auth config if user changed from defaults
      ...(form.authMethod !== (selectedEntry.auth_method ?? "bearer")
        ? { auth_method: form.authMethod }
        : {}),
      ...(form.authKeyName !== (selectedEntry.auth_key_name ?? "Authorization")
        ? { auth_key_name: form.authKeyName }
        : {}),
    }
  : { /* custom params unchanged */ };
```

### Backend Support

Check that `POST /api/v1/keys` already accepts optional `auth_method` and `auth_key_name` even when `service_slug` is provided. If it does, the backend falls back to catalog defaults when not provided. If not, add this fallback logic in the key creation handler.

---

## 8. Complete File List

### Files to CREATE

| File | Purpose |
|------|---------|
| `frontend/src/pages/api-key-detail.tsx` | Detail page for viewing/editing a NyxID API key with scope |

### Files to MODIFY

| File | Changes |
|------|---------|
| **Backend** | |
| `backend/src/models/api_key.rs` | Add `description`, `allowed_service_ids`, `allowed_node_ids`, `allow_all_services`, `allow_all_nodes` fields. Remove `agent_group_id`. |
| `backend/src/models/mod.rs` | Remove `pub mod agent_group;` |
| `backend/src/services/key_service.rs` | Remove `create_api_key_for_agent_group`. Add scope params to `create_api_key`. Add `update_api_key_scope`. Keep `nyxid_ag_` prefix logic for scoped keys. |
| `backend/src/services/mod.rs` | Remove `pub mod agent_group_service;` |
| `backend/src/services/proxy_service.rs` | Remove agent_group_service import if present. Scope check now uses AuthUser fields. |
| `backend/src/handlers/mod.rs` | Remove `pub mod agent_groups;` |
| `backend/src/handlers/proxy.rs` | Replace `agent_group_service::check_agent_access` with inline scope check on `auth_user`. Remove `agent_group_service` import. |
| `backend/src/mw/auth.rs` | Replace `agent_group_id: Option<String>` with `allow_all_services: bool`, `allow_all_nodes: bool`, `allowed_service_ids: Vec<String>`, `allowed_node_ids: Vec<String>`. Populate from ApiKey on API key auth. |
| `backend/src/routes.rs` | Remove `/agent-groups` route nest. Add `PUT` to `/api-keys/{key_id}`. |
| `backend/src/db.rs` | Remove `agent_groups` index creation. |
| `backend/src/errors/mod.rs` | Remove `AgentGroupInactive` and `AgentGroupNotFound` variants (or keep for backward compat). Optionally rename `AgentGroupForbidden` message. |
| **Frontend** | |
| `frontend/src/pages/keys.tsx` | Remove "Agent Groups" tab. Change from 3 tabs to 2. Remove AgentGroup imports/state. |
| `frontend/src/pages/lazy.ts` | Remove `AgentGroupDetailPage` export. Add `ApiKeyDetailPage` export. |
| `frontend/src/router.tsx` | Remove `agentGroupDetailRoute`. Add `apiKeyDetailRoute`. Remove `servicesLayout`, `connectionsRoute`, `providersLayout` as direct children (replace with redirect routes). Add admin service/provider routes. |
| `frontend/src/components/dashboard/sidebar.tsx` | Remove Services, Connections, Providers from `NAV_ITEMS`. Add Services and Providers to `ADMIN_NAV_ITEMS`. |
| `frontend/src/components/dashboard/add-key-dialog.tsx` | Show auth method and auth key name for catalog services (not just custom). Pre-populate from catalog entry. |
| `frontend/src/types/keys.ts` | Add `NyxIdApiKeyInfo`, `AllowedServiceInfo`, `AllowedNodeInfo` types. |
| `frontend/src/hooks/use-keys.ts` | Add hooks for NyxID API key CRUD with scope (update, get detail). |
| `frontend/src/pages/ai-setup.tsx` | Update references if any point to removed pages. |

### Files to DELETE

| File | Reason |
|------|--------|
| `backend/src/models/agent_group.rs` | AgentGroup model absorbed into ApiKey |
| `backend/src/services/agent_group_service.rs` | Agent group logic absorbed into key_service |
| `backend/src/handlers/agent_groups.rs` | Agent group routes removed |
| `frontend/src/pages/agent-group-detail.tsx` | Replaced by api-key-detail.tsx |
| `frontend/src/components/dashboard/agent-group-card.tsx` | No longer needed (all keys in one table) |
| `frontend/src/components/dashboard/create-agent-group-dialog.tsx` | Replaced by enhanced ApiKeyCreateDialog |
| `frontend/src/hooks/use-agent-groups.ts` | No longer needed |
| `frontend/src/types/agent-groups.ts` | Types absorbed into keys.ts |

---

## Migration Notes

1. **`agent_groups` collection:** If any data exists, run a one-time migration at startup that reads each `AgentGroup`, copies its scope fields onto the linked `ApiKey`, then drops the collection. If no data exists (likely), just remove the index creation.

2. **`api_key.agent_group_id` field:** Existing documents with this field will have it ignored (no longer in the model). The serde `#[serde(default)]` on the new fields ensures backward compatibility.

3. **`allow_all_services` defaults to `true`:** All existing API keys that were created without scope will continue to have unrestricted access. Only newly created scoped keys will have `allow_all_services: false`.

4. **Error code backward compatibility:** Error codes 9000-9002 remain unchanged in value. `AgentGroupForbidden` (9000) is still the code returned when scope check fails. Clients checking for this code continue to work. The `AgentGroupInactive` (9001) and `AgentGroupNotFound` (9002) can be removed since the conditions they covered are now handled by existing `ApiKey` validation (inactive keys caught by `validate_api_key`, no separate group to look up).

---

## Summary of Conceptual Changes

```
BEFORE (v2):
  API Keys page has 3 tabs:
    - External Services (user's configured services)
    - NyxID API Keys (regular keys, table)
    - Agent Groups (scoped keys, separate model + card grid)

  Sidebar: Dashboard | AI Services | Services | Connections | Providers | Nodes | ...

  Agent Group = AgentGroup model (scope) + linked ApiKey model (auth)
  Two DB lookups on every scoped proxy request.

AFTER (v3):
  API Keys page has 2 tabs:
    - External Services (user's configured services)
    - NyxID API Keys (all keys, scope column in table)

  Sidebar: Dashboard | AI Services | Nodes | Settings | ...
  Admin sidebar: ... | Services | Providers

  Every API key optionally has scope (fields on ApiKey model).
  One DB lookup on every proxy request (ApiKey already loaded by auth middleware).
```
