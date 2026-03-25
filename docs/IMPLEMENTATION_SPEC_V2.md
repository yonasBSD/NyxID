> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V2: AI Services Rename, NyxID API Key Merge, Agent Group Keys

This addendum builds on `IMPLEMENTATION_SPEC.md` (Phase 0 of the streamline services work). It covers three new requirements:

1. **Rename "Keys" to "AI Services"** -- branding change across frontend
2. **Merge NyxID API Keys into the unified page** -- one page for everything
3. **Agent Group API Key** -- scoped NyxID API keys that control which external services/nodes an agent can access

---

## Table of Contents

1. [Naming Changes](#1-naming-changes)
2. [NyxID API Key Merge](#2-nyxid-api-key-merge)
3. [Agent Group API Key Data Model](#3-agent-group-api-key-data-model)
4. [Backend Changes](#4-backend-changes)
5. [Frontend Changes](#5-frontend-changes)
6. [Proxy Enforcement](#6-proxy-enforcement)
7. [Complete File List](#7-complete-file-list)

---

## 1. Naming Changes

**Principle:** API route paths stay the same (`/api/v1/keys`, `/api/v1/api-keys`, etc.) for backward compatibility. Only UI-facing labels, page titles, sidebar text, and breadcrumbs change.

### Files Requiring Label/Title Changes

| File | What Changes |
|------|-------------|
| `frontend/src/components/dashboard/sidebar.tsx` | `{ to: "/keys", icon: Cable, label: "Keys" }` -> `{ to: "/keys", icon: Cable, label: "AI Services" }`. **Remove** the separate `{ to: "/api-keys", icon: Key, label: "API Keys" }` nav item entirely (merged into AI Services page). |
| `frontend/src/pages/keys.tsx` | Page title: `"Keys"` -> `"AI Services"`. Description: `"Manage your API keys and service connections in one place."` -> `"Manage your AI service credentials and NyxID API keys."`. Button: `"Add Key"` -> `"Add Service"`. Empty state text: `"No keys yet"` -> `"No AI services yet"`, `"Add a key to connect..."` -> `"Add an AI service to connect..."`. |
| `frontend/src/pages/key-detail.tsx` | Breadcrumb: `{ label: "Keys", to: "/keys" }` -> `{ label: "AI Services", to: "/keys" }`. Delete dialog title: `"Delete Key"` -> `"Delete Service"`. |
| `frontend/src/components/dashboard/add-key-dialog.tsx` | Dialog title: `"Add Key"` / `"Configure Key"` -> `"Add AI Service"` / `"Configure Service"`. Description: `"Pick a service from the catalog..."` -> `"Pick from the catalog or create a custom endpoint."`. Button: `"Create Key"` -> `"Create Service"`. |
| `frontend/src/pages/ai-setup.tsx` | Quick prompt titles/descriptions that reference "Keys" page -> "AI Services". Links `{ to: "/keys", label: "Keys" }` -> `{ to: "/keys", label: "AI Services" }`. The "Create and manage API keys" prompt -> update description to mention the unified AI Services page. The `{ to: "/api-keys", label: "API Keys" }` link -> `{ to: "/keys", label: "AI Services" }`. |

### What Does NOT Change

- Backend API route paths: `/api/v1/keys`, `/api/v1/api-keys`, `/api/v1/endpoints`, `/api/v1/user-services`, `/api/v1/catalog`
- Frontend URL paths: `/keys`, `/keys/$keyId` (no URL change, just label change)
- Backend model names, collection names, service function names
- Types, hooks, query keys

---

## 2. NyxID API Key Merge

### Current State

Two separate systems:

| System | Page | Backend Route | Model | Purpose |
|--------|------|---------------|-------|---------|
| NyxID API Keys | `/api-keys` | `GET/POST /api/v1/api-keys` | `ApiKey` (collection: `api_keys`) | Authenticate with NyxID itself (programmatic access) |
| External Service Keys | `/keys` | `GET/POST /api/v1/keys` | `UserApiKey` + `UserEndpoint` + `UserService` | Authenticate with external APIs (OpenAI, etc.) |

### Target State

One unified page at `/keys` (displayed as "AI Services") with **two tabs**:

```
+------------------------------------------------------------------+
|  AI Services                                    [+ Add Service]  |
|                                                                  |
|  [External Services]  [NyxID API Keys]  [Agent Groups]           |
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
|  AI Services                                    [+ Add Service]  |
|                                                                  |
|  [External Services]  [NyxID API Keys]  [Agent Groups]           |
|                       ~~~~~~~~~~~~~~~~                           |
|                                                                  |
|  +---------------------------------------------------------------+
|  | Name       | Prefix   | Scopes      | Last Used  | Actions   |
|  |------------|----------|-------------|------------|-----------|
|  | Production | nyxid_ab | read write  | 2h ago     | [Rotate]  |
|  | CI/CD      | nyxid_cd | read        | 5m ago     | [Rotate]  |
|  +---------------------------------------------------------------+
+------------------------------------------------------------------+

Switching to "Agent Groups" tab:

+------------------------------------------------------------------+
|  AI Services                                    [+ Add Service]  |
|                                                                  |
|  [External Services]  [NyxID API Keys]  [Agent Groups]           |
|                                         ~~~~~~~~~~~~~            |
|                                                                  |
|  +------------------+  +------------------+                      |
|  | Claude Agent     |  | CI Pipeline      |                      |
|  | nyxid_ag_ab...   |  | nyxid_ag_cd...   |                      |
|  | 3 services,      |  | 1 service,       |                      |
|  | 1 node           |  | 0 nodes          |                      |
|  +------------------+  +------------------+                      |
+------------------------------------------------------------------+
```

### Design Decisions

1. **Tabs, not sections.** The two key types serve different purposes and have different schemas. Tabs keep the page scannable. Three tabs: "External Services" (default), "NyxID API Keys", "Agent Groups".

2. **"+ Add Service" button is context-aware.** On the "External Services" tab it opens the catalog wizard (existing `AddKeyDialog`). On the "NyxID API Keys" tab it opens the existing `ApiKeyCreateDialog`. On the "Agent Groups" tab it opens a new `CreateAgentGroupDialog`.

3. **No backend API changes for existing NyxID API keys.** The frontend fetches from both `/api/v1/keys` and `/api/v1/api-keys` and displays them in their respective tabs. No need to merge backend collections or routes.

4. **Remove the standalone `/api-keys` page.** The route `/api-keys` becomes a redirect to `/keys?tab=nyxid`.

### Frontend Architecture

```
pages/keys.tsx (unified page)
  |-- Tab: "External Services" (default)
  |     |-- KeyCard grid (existing, from useKeys())
  |     |-- AddKeyDialog (existing catalog wizard)
  |
  |-- Tab: "NyxID API Keys"
  |     |-- ApiKeyTable (existing component, moved from api-keys page)
  |     |-- ApiKeyCreateDialog (existing component)
  |
  |-- Tab: "Agent Groups"
        |-- AgentGroupCard grid (new, from useAgentGroups())
        |-- CreateAgentGroupDialog (new)
```

### Tab URL State

Use query parameter `?tab=services|nyxid|agents` to make tabs bookmarkable and linkable. Default is `services`.

---

## 3. Agent Group API Key Data Model

### Concept

An **Agent Group API Key** is a special NyxID API key that grants scoped access to specific external service credentials (UserApiKey/UserService records) and/or nodes. When an AI agent authenticates with NyxID using this key, the proxy enforces that the agent can only access the services listed in the group.

### Key Prefix

- Regular NyxID API keys: `nyxid_` prefix
- Agent group API keys: `nyxid_ag_` prefix

The prefix distinguishes agent group keys at a glance and in validation logic.

### MongoDB Model: `AgentGroup`

New collection: `agent_groups`

```rust
// backend/src/models/agent_group.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;

pub const COLLECTION_NAME: &str = "agent_groups";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentGroup {
    #[serde(rename = "_id")]
    pub id: String,
    /// The user who owns this agent group
    pub user_id: String,
    /// Human-readable name (e.g., "Claude Agent", "CI Pipeline")
    pub name: String,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // --- Scoped Access ---

    /// List of UserService IDs this agent group can access via proxy.
    /// When empty + allow_all_services is false: no proxy access.
    /// When allow_all_services is true: this field is ignored.
    #[serde(default)]
    pub allowed_service_ids: Vec<String>,

    /// List of Node IDs this agent group can route through.
    /// When empty + allow_all_nodes is false: no node routing.
    /// When allow_all_nodes is true: this field is ignored.
    #[serde(default)]
    pub allowed_node_ids: Vec<String>,

    /// If true, agent can access ALL of the user's external services
    /// (ignores allowed_service_ids). Default: false.
    #[serde(default)]
    pub allow_all_services: bool,

    /// If true, agent can route through ALL of the user's nodes
    /// (ignores allowed_node_ids). Default: false.
    #[serde(default)]
    pub allow_all_nodes: bool,

    // --- Linked API Key ---

    /// The NyxID API key ID (from `api_keys` collection) that is linked to this group.
    /// This is a 1:1 relationship: each agent group has exactly one API key.
    pub api_key_id: String,

    pub is_active: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}
```

### Relationship to Existing `ApiKey` Model

The `ApiKey` model in `api_keys` collection gets one new optional field:

```rust
// Add to existing ApiKey struct in backend/src/models/api_key.rs:

    /// If set, this API key is an agent group key.
    /// Points to the AgentGroup that defines its scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_group_id: Option<String>,
```

**Why a separate `AgentGroup` model instead of embedding scope in `ApiKey`?**
- Separation of concerns: `ApiKey` handles authentication (hash, prefix, scopes, expiry). `AgentGroup` handles authorization (which services/nodes).
- The scope configuration (allowed services, nodes, allow-all flags) is complex enough to warrant its own document.
- Agent groups could eventually support multiple API keys or additional access control rules.

### Entity Relationship

```
ApiKey (api_keys collection)
  |-- agent_group_id (optional) --> AgentGroup (agent_groups collection)
                                       |-- allowed_service_ids --> UserService[]
                                       |-- allowed_node_ids --> Node[]
                                       |-- api_key_id --> ApiKey (back-reference)
```

### Indexes

Add to `ensure_indexes()` in `db.rs`:

```rust
    // -- agent_groups --
    let agent_groups = db.collection::<mongodb::bson::Document>("agent_groups");
    agent_groups
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .build(),
        )
        .await?;
    agent_groups
        .create_index(
            IndexModel::builder()
                .keys(doc! { "api_key_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
```

---

## 4. Backend Changes

### 4.1 New Service: `agent_group_service.rs`

```rust
// backend/src/services/agent_group_service.rs

/// List all agent groups for a user.
pub async fn list_agent_groups(db: &Database, user_id: &str) -> AppResult<Vec<AgentGroup>>
// Query: { user_id } sorted by created_at desc

/// Get single agent group by ID, verifying ownership.
pub async fn get_agent_group(db: &Database, user_id: &str, group_id: &str) -> AppResult<AgentGroup>
// Query: { _id: group_id, user_id }

/// Create a new agent group with a linked API key.
///
/// Steps:
/// 1. Validate name (1-200 chars)
/// 2. Validate allowed_service_ids: each must be an active UserService owned by user_id
/// 3. Validate allowed_node_ids: each must be a Node owned by user_id
/// 4. Create an ApiKey with:
///    - prefix: "nyxid_ag_" (8 chars)
///    - scopes: "proxy" (agent keys are limited to proxy access)
///    - agent_group_id: set to the new group ID
/// 5. Create AgentGroup with api_key_id pointing to the new ApiKey
/// 6. Return (AgentGroup, full_key) -- full_key shown only once
pub async fn create_agent_group(
    db: &Database,
    user_id: &str,
    name: &str,
    description: Option<&str>,
    allowed_service_ids: &[String],
    allowed_node_ids: &[String],
    allow_all_services: bool,
    allow_all_nodes: bool,
    expires_at: Option<DateTime<Utc>>,
) -> AppResult<(AgentGroup, String)> // Returns (group, full_api_key)

/// Update agent group scope (services, nodes, name, description).
/// Does NOT rotate the API key -- that's a separate operation.
pub async fn update_agent_group(
    db: &Database,
    user_id: &str,
    group_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    allowed_service_ids: Option<&[String]>,
    allowed_node_ids: Option<&[String]>,
    allow_all_services: Option<bool>,
    allow_all_nodes: Option<bool>,
) -> AppResult<()>
// Validates service/node ownership if IDs are provided
// $set only provided fields + updated_at

/// Delete an agent group and deactivate its linked API key.
pub async fn delete_agent_group(db: &Database, user_id: &str, group_id: &str) -> AppResult<()>
// 1. Load AgentGroup (verify ownership)
// 2. Deactivate linked ApiKey (is_active = false)
// 3. Delete AgentGroup document

/// Rotate the API key for an agent group.
/// Deactivates old key, creates new key with same scopes and agent_group_id.
pub async fn rotate_agent_group_key(
    db: &Database,
    user_id: &str,
    group_id: &str,
) -> AppResult<(AgentGroup, String)> // Returns (group, new_full_api_key)

/// Look up an agent group by its linked API key ID.
/// Used by the proxy to check scope enforcement.
/// Returns None if the API key is not an agent group key.
pub async fn find_by_api_key_id(db: &Database, api_key_id: &str) -> AppResult<Option<AgentGroup>>
```

### 4.2 New Handler: `agent_groups.rs`

```rust
// backend/src/handlers/agent_groups.rs

#[derive(Debug, Deserialize)]
pub struct CreateAgentGroupRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub allowed_service_ids: Vec<String>,
    #[serde(default)]
    pub allowed_node_ids: Vec<String>,
    #[serde(default)]
    pub allow_all_services: bool,
    #[serde(default)]
    pub allow_all_nodes: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AgentGroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub allowed_service_ids: Vec<String>,
    pub allowed_node_ids: Vec<String>,
    pub allow_all_services: bool,
    pub allow_all_nodes: bool,
    pub api_key_id: String,
    pub api_key_prefix: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
    // Enriched data (joined from other collections)
    pub allowed_services: Vec<AllowedServiceInfo>,
    pub allowed_nodes: Vec<AllowedNodeInfo>,
}

#[derive(Debug, Serialize)]
pub struct AllowedServiceInfo {
    pub id: String,
    pub slug: String,
    pub label: String,
    pub catalog_service_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AllowedNodeInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateAgentGroupResponse {
    pub group: AgentGroupResponse,
    /// The full API key -- shown only once at creation time.
    pub full_key: String,
}

#[derive(Debug, Serialize)]
pub struct AgentGroupListResponse {
    pub groups: Vec<AgentGroupResponse>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub allowed_service_ids: Option<Vec<String>>,
    pub allowed_node_ids: Option<Vec<String>>,
    pub allow_all_services: Option<bool>,
    pub allow_all_nodes: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RotateAgentGroupKeyResponse {
    pub group: AgentGroupResponse,
    pub full_key: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteAgentGroupResponse {
    pub message: String,
}

/// GET /api/v1/agent-groups
pub async fn list_agent_groups(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<AgentGroupListResponse>>

/// GET /api/v1/agent-groups/{group_id}
pub async fn get_agent_group(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(group_id): Path<String>,
) -> AppResult<Json<AgentGroupResponse>>

/// POST /api/v1/agent-groups
pub async fn create_agent_group(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateAgentGroupRequest>,
) -> AppResult<Json<CreateAgentGroupResponse>>

/// PUT /api/v1/agent-groups/{group_id}
pub async fn update_agent_group(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(group_id): Path<String>,
    Json(body): Json<UpdateAgentGroupRequest>,
) -> AppResult<Json<AgentGroupResponse>>

/// POST /api/v1/agent-groups/{group_id}/rotate-key
pub async fn rotate_agent_group_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(group_id): Path<String>,
) -> AppResult<Json<RotateAgentGroupKeyResponse>>

/// DELETE /api/v1/agent-groups/{group_id}
pub async fn delete_agent_group(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(group_id): Path<String>,
) -> AppResult<Json<DeleteAgentGroupResponse>>
```

### 4.3 New Routes

Add to `routes.rs` in the `api_v1_human_only` section:

```rust
    let agent_group_routes = Router::new()
        .route(
            "/",
            get(handlers::agent_groups::list_agent_groups)
                .post(handlers::agent_groups::create_agent_group),
        )
        .route(
            "/{group_id}",
            get(handlers::agent_groups::get_agent_group)
                .put(handlers::agent_groups::update_agent_group)
                .delete(handlers::agent_groups::delete_agent_group),
        )
        .route(
            "/{group_id}/rotate-key",
            post(handlers::agent_groups::rotate_agent_group_key),
        );

    // In api_v1_human_only, add:
    .nest("/agent-groups", agent_group_routes)
```

### 4.4 Changes to `key_service.rs` (API Key Generation)

The existing `create_api_key` function needs a small change to support the `nyxid_ag_` prefix and `agent_group_id` field:

```rust
// Modify create_api_key to accept optional agent_group_id:
pub async fn create_api_key(
    db: &mongodb::Database,
    user_id: &str,
    name: &str,
    scopes: &str,
    expires_at: Option<DateTime<Utc>>,
    agent_group_id: Option<&str>,  // NEW parameter
) -> AppResult<CreateApiKeyResult>
```

**Logic change:**
- If `agent_group_id` is `Some`, generate the key with prefix `nyxid_ag_` (instead of `nyxid_`).
- Set `agent_group_id` on the `ApiKey` document.
- Existing callers pass `None` for `agent_group_id` (backward compatible).

### 4.5 Changes to `ApiKey` Model

Add one field to `backend/src/models/api_key.rs`:

```rust
    /// If set, this API key is an agent group key with scoped access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_group_id: Option<String>,
```

### 4.6 Module Registration

In `backend/src/models/mod.rs`, add:
```rust
pub mod agent_group;
```

In `backend/src/services/mod.rs`, add:
```rust
pub mod agent_group_service;
```

In `backend/src/handlers/mod.rs`, add:
```rust
pub mod agent_groups;
```

---

## 5. Frontend Changes

### 5.1 Unified AI Services Page (`pages/keys.tsx`)

Restructure to use tabs:

```tsx
// pages/keys.tsx
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSearchParams } from "@tanstack/react-router"; // or useSearch

export function KeysPage() {
  // Read ?tab= from URL, default "services"
  const tab = /* from search params */ "services";

  return (
    <div className="space-y-8">
      <PageHeader
        title="AI Services"
        description="Manage your AI service credentials and NyxID API keys."
        actions={<AddButton tab={tab} />}
      />

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="services">External Services</TabsTrigger>
          <TabsTrigger value="nyxid">NyxID API Keys</TabsTrigger>
          <TabsTrigger value="agents">Agent Groups</TabsTrigger>
        </TabsList>

        <TabsContent value="services">
          {/* Existing KeyCard grid from useKeys() */}
          {/* Existing AddKeyDialog */}
        </TabsContent>

        <TabsContent value="nyxid">
          {/* Existing ApiKeyTable from useApiKeys() */}
          {/* Existing ApiKeyCreateDialog */}
        </TabsContent>

        <TabsContent value="agents">
          {/* New AgentGroupCard grid from useAgentGroups() */}
          {/* New CreateAgentGroupDialog */}
        </TabsContent>
      </Tabs>
    </div>
  );
}
```

### 5.2 Remove Standalone API Keys Page

- **Delete** `frontend/src/pages/api-keys.tsx` (its content moves into the `keys.tsx` "NyxID API Keys" tab)
- **Keep** `frontend/src/hooks/use-api-keys.ts` (still needed -- the hooks are reused)
- **Keep** `frontend/src/components/dashboard/api-key-table.tsx` (reused inside the tab)
- **Keep** `frontend/src/components/dashboard/api-key-create-dialog.tsx` (reused)
- **Keep** `frontend/src/schemas/api-keys.ts` (reused)

### 5.3 Router Changes (`router.tsx`)

```diff
- const apiKeysRoute = createRoute({
-   path: "/api-keys",
-   getParentRoute: () => dashboardLayout,
-   component: ApiKeysPage,
- });
+ // Redirect /api-keys to /keys?tab=nyxid
+ const apiKeysRedirectRoute = createRoute({
+   path: "/api-keys",
+   getParentRoute: () => dashboardLayout,
+   beforeLoad: () => {
+     throw redirect({ to: "/keys", search: { tab: "nyxid" } });
+   },
+   component: () => null,
+ });
```

Update the route tree to use `apiKeysRedirectRoute` instead of `apiKeysRoute`.

Remove `ApiKeysPage` from `lazy.ts` imports (no longer lazy-loaded as a standalone page).

### 5.4 New Types (`types/agent-groups.ts`)

```typescript
export interface AgentGroupInfo {
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly allowed_service_ids: readonly string[];
  readonly allowed_node_ids: readonly string[];
  readonly allow_all_services: boolean;
  readonly allow_all_nodes: boolean;
  readonly api_key_id: string;
  readonly api_key_prefix: string;
  readonly is_active: boolean;
  readonly created_at: string;
  readonly updated_at: string;
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

export interface AgentGroupListResponse {
  readonly groups: readonly AgentGroupInfo[];
}

export interface CreateAgentGroupResponse {
  readonly group: AgentGroupInfo;
  readonly full_key: string;
}

export interface RotateAgentGroupKeyResponse {
  readonly group: AgentGroupInfo;
  readonly full_key: string;
}
```

### 5.5 New Hooks (`hooks/use-agent-groups.ts`)

```typescript
export function useAgentGroups() {
  return useQuery({
    queryKey: ["agent-groups"],
    queryFn: async (): Promise<readonly AgentGroupInfo[]> => {
      const res = await api.get<AgentGroupListResponse>("/agent-groups");
      return res.groups;
    },
  });
}

export function useAgentGroup(groupId: string) {
  return useQuery({
    queryKey: ["agent-groups", groupId],
    queryFn: async (): Promise<AgentGroupInfo> => {
      return api.get<AgentGroupInfo>(`/agent-groups/${groupId}`);
    },
    enabled: Boolean(groupId),
  });
}

export function useCreateAgentGroup() { /* POST /agent-groups */ }
export function useUpdateAgentGroup() { /* PUT /agent-groups/:id */ }
export function useDeleteAgentGroup() { /* DELETE /agent-groups/:id */ }
export function useRotateAgentGroupKey() { /* POST /agent-groups/:id/rotate-key */ }
```

### 5.6 New Component: `CreateAgentGroupDialog`

A dialog with:
1. **Name** input (required)
2. **Description** input (optional)
3. **Service scope** picker:
   - Toggle: "Allow all services" (checkbox)
   - If not "allow all": multi-select list of user's active external services (from `useKeys()`)
   - Each service shows: label, slug, catalog name
4. **Node scope** picker:
   - Toggle: "Allow all nodes" (checkbox)
   - If not "allow all": multi-select list of user's nodes (from `useNodes()`)
   - Each node shows: name, status
5. **Expiration** (optional date picker)
6. **Create** button

On success: show the full API key in a one-time reveal dialog (same pattern as existing `ApiKeyCreateDialog` key reveal).

### 5.7 New Component: `AgentGroupCard`

Card showing:
- Group name
- API key prefix (`nyxid_ag_...`)
- Service count (e.g., "3 services") or "All services"
- Node count (e.g., "1 node") or "All nodes"
- Active/inactive badge
- Click navigates to detail page

### 5.8 New Page: `AgentGroupDetailPage`

Route: `/keys/agent-group/$groupId`

Shows:
- Name, description (editable)
- API key info (prefix, last used, expiry) with rotate button
- Service scope editor (add/remove services, toggle allow-all)
- Node scope editor (add/remove nodes, toggle allow-all)
- Delete button

---

## 6. Proxy Enforcement

### How Agent Group Scope is Enforced

When a request comes through the proxy (`/proxy/s/{slug}/*` or `/proxy/{service_id}/*`):

1. **Authentication** (existing): `mw/auth.rs` validates the API key and extracts `AuthUser` with `user_id` and the `ApiKey` record.

2. **Agent group check** (new): After authentication, if `api_key.agent_group_id` is `Some`:
   a. Load the `AgentGroup` by `api_key.agent_group_id`
   b. If group is not active: reject with 403
   c. Resolve the `UserService` for this proxy request (by slug or catalog_service_id)
   d. **Service check:** If `!group.allow_all_services` AND `user_service.id` is NOT in `group.allowed_service_ids`: reject with 403 "Agent group does not have access to this service"
   e. **Node check:** If `user_service.node_id` is set AND `!group.allow_all_nodes` AND `user_service.node_id` is NOT in `group.allowed_node_ids`: reject with 403 "Agent group does not have access to this node"
   f. If `api_key.agent_group_id` is `None`: no scope restriction (existing behavior)

### Implementation Location

Add the check in `proxy_service.rs` or `handlers/proxy.rs`, after the user is authenticated and before the proxy target is resolved.

**New function in `agent_group_service.rs`:**

```rust
/// Check if an agent group key is authorized to access a given service and node.
/// Returns Ok(()) if authorized, Err(Forbidden) if not.
///
/// Called only when the authenticating API key has an agent_group_id.
pub async fn check_agent_access(
    db: &Database,
    agent_group_id: &str,
    user_service_id: &str,
    node_id: Option<&str>,
) -> AppResult<()>
```

### Changes to Auth Middleware

The auth middleware (`mw/auth.rs`) already extracts the `ApiKey` when authenticating via API key. The `AuthUser` struct needs to carry the `agent_group_id` so the proxy handler can use it:

```rust
// In mw/auth.rs, AuthUser struct -- add:
    pub agent_group_id: Option<String>,
```

When authenticating via API key, populate `agent_group_id` from `api_key.agent_group_id`.

### Error Codes

Add new error variant to `AppError`:

```rust
    AgentGroupForbidden(String),  // -> 403, error_code: 9000
```

Error codes 9000-9002 reserved for agent group errors:
- 9000: `AgentGroupForbidden` -- agent group does not have access to the requested resource
- 9001: `AgentGroupInactive` -- agent group has been deactivated
- 9002: `AgentGroupNotFound` -- agent group referenced by API key no longer exists

---

## 7. Complete File List

### Files to CREATE

| File | Purpose |
|------|---------|
| `backend/src/models/agent_group.rs` | AgentGroup model struct |
| `backend/src/services/agent_group_service.rs` | CRUD + scope validation + proxy access check |
| `backend/src/handlers/agent_groups.rs` | HTTP handlers for agent group routes |
| `frontend/src/types/agent-groups.ts` | TypeScript types for agent groups |
| `frontend/src/hooks/use-agent-groups.ts` | TanStack Query hooks for agent groups |
| `frontend/src/components/dashboard/create-agent-group-dialog.tsx` | Dialog for creating agent groups with scope picker |
| `frontend/src/components/dashboard/agent-group-card.tsx` | Card component for agent group list |
| `frontend/src/pages/agent-group-detail.tsx` | Detail page for viewing/editing an agent group |

### Files to MODIFY

| File | Changes |
|------|---------|
| **Backend** | |
| `backend/src/models/mod.rs` | Add `pub mod agent_group;` |
| `backend/src/models/api_key.rs` | Add `agent_group_id: Option<String>` field |
| `backend/src/services/mod.rs` | Add `pub mod agent_group_service;` |
| `backend/src/services/key_service.rs` | Add `agent_group_id` parameter to `create_api_key`, support `nyxid_ag_` prefix |
| `backend/src/handlers/mod.rs` | Add `pub mod agent_groups;` |
| `backend/src/routes.rs` | Add agent group routes nest |
| `backend/src/db.rs` | Add `agent_groups` indexes in `ensure_indexes()` |
| `backend/src/mw/auth.rs` | Add `agent_group_id` field to `AuthUser`, populate from ApiKey |
| `backend/src/errors/mod.rs` | Add `AgentGroupForbidden` / `AgentGroupInactive` / `AgentGroupNotFound` error variants (9000-9002) |
| `backend/src/services/proxy_service.rs` | Add agent group scope check before proxy resolution |
| **Frontend** | |
| `frontend/src/pages/keys.tsx` | Add tabs (External Services / NyxID API Keys / Agent Groups), rename labels from "Keys" to "AI Services" |
| `frontend/src/pages/key-detail.tsx` | Update breadcrumb "Keys" -> "AI Services" |
| `frontend/src/pages/lazy.ts` | Remove `ApiKeysPage` export, add `AgentGroupDetailPage` export |
| `frontend/src/router.tsx` | Replace `apiKeysRoute` with redirect, add `agentGroupDetailRoute`, remove `ApiKeysPage` import |
| `frontend/src/components/dashboard/sidebar.tsx` | Rename "Keys" -> "AI Services", remove "API Keys" nav item |
| `frontend/src/components/dashboard/add-key-dialog.tsx` | Rename dialog titles from "Key" to "Service" |
| `frontend/src/pages/ai-setup.tsx` | Update quick prompt labels: "Keys" -> "AI Services", fix `/api-keys` links |
| `frontend/src/types/api.ts` | Add `agent_group_id?: string` to `ApiKey` interface |

### Files to DELETE

| File | Reason |
|------|--------|
| `frontend/src/pages/api-keys.tsx` | Merged into `/keys` page "NyxID API Keys" tab. Components (`ApiKeyTable`, `ApiKeyCreateDialog`) are kept and reused. |

---

## Migration Notes

- No data migration needed for the rename (UI-only change).
- No data migration needed for NyxID API key merge (frontend-only change).
- The `agent_group_id` field on `ApiKey` defaults to `None` via `#[serde(default)]`, so existing API keys are unaffected.
- The `agent_groups` collection is new -- no migration, just create indexes at startup.
- Existing API key validation in `key_service::validate_api_key` continues to work unchanged. The agent group scope check happens downstream in the proxy layer, not during key validation.
