> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Proposal: Streamline Services, Connections, and Providers

**STATUS: Implemented.** The 3 new user collections (`user_endpoints`, `user_api_keys`, `user_services`) and the unified `/api/v1/keys` orchestration endpoint are live. The frontend "AI Services" page (`/keys`) replaces the old Services/Connections/Providers pages for normal users. AgentGroup model was absorbed into ApiKey scope fields. Old collections are retained for backward-compatible migration. See `IMPLEMENTATION_SPEC.md`, `IMPLEMENTATION_SPEC_V2.md`, and `IMPLEMENTATION_SPEC_V3.md` for implementation details.

---

## Problem

The current architecture has **6 user-relevant models** scattered across **3 pages** and **4 concepts** that should be clearly separated but are tangled together:

| Concept | What it is | Currently scattered across |
|---------|-----------|---------------------------|
| **Endpoint** (where) | Target URL | `DownstreamService.base_url` + `UserProviderToken.gateway_url` |
| **API Key** (auth) | Credential | `UserProviderToken` OR `UserServiceConnection` (depends on category) |
| **Service** (path) | Proxy routing | `DownstreamService.slug` + `ServiceProviderRequirement` (admin-only) |
| **Node Route** (local) | Route via agent | `NodeServiceBinding` (hidden on node detail page) |

---

## Current Architecture: Why It's Confusing

### The OpenClaw Problem

```mermaid
sequenceDiagram
    participant User
    participant ProvPage as Providers Page
    participant SvcPage as Services Page
    participant NodePage as Node Page
    participant DB

    Note over User: User wants to use their local OpenClaw

    User->>ProvPage: 1. Go to Providers page
    ProvPage->>DB: POST /providers/openclaw/connect/api-key<br/>{ api_key: "tok-...", gateway_url: "http://localhost:18789" }
    Note over DB: Stores credential AND url<br/>in UserProviderToken (mixed!)

    User->>SvcPage: 2. Go to Services page to verify
    Note over SvcPage: Shows "llm-openclaw" service<br/>base_url: "https://openclaw-gateway.invalid" (FAKE)<br/>User can't change this

    User->>NodePage: 3. If routing through node, go to Node page
    NodePage->>DB: POST /nodes/{id}/bindings<br/>{ service_id: "llm-openclaw" }
    Note over NodePage: Binding hidden in node detail page<br/>Not visible from service context

    Note over User: 3 pages touched, 2 concepts mixed (url + key),<br/>node binding disconnected from service
```

### Where Each Concept Lives Today

```mermaid
graph TB
    subgraph "Page: Services /services"
        SVC_LIST["Service List<br/>19 seeded services"]
        SVC_DETAIL["Service Detail<br/>Shows base_url (read-only)<br/>Shows auth_method (read-only)"]
    end

    subgraph "Page: Connections /connections"
        CONN_GRID["Connection Grid<br/>Add API key for 'connection' services"]
    end

    subgraph "Page: Providers /providers"
        PROV_GRID["Provider Grid<br/>Add API key for 'provider' services<br/>+ gateway_url for OpenClaw<br/>+ OAuth flows"]
    end

    subgraph "Page: Node Detail /nodes/:id"
        NODE_BIND["Service Bindings tab<br/>Bind services to this node<br/>Set priority for failover"]
    end

    EP["Endpoint (URL)"] -->|"base_url on"| SVC_DETAIL
    EP -->|"gateway_url on"| PROV_GRID
    KEY["API Key (credential)"] -->|"for 'connection' services"| CONN_GRID
    KEY -->|"for 'provider' services"| PROV_GRID
    SVC_PATH["Service (path)"] -->|"slug on"| SVC_LIST
    NODE["Node Route"] -->|"binding on"| NODE_BIND

    style EP fill:#f96,stroke:#333
    style KEY fill:#f96,stroke:#333
    style SVC_PATH fill:#ff9,stroke:#333
    style NODE fill:#ff9,stroke:#333
```

**Problems:**
1. **Endpoint URL** is split: `base_url` on service (admin-only), `gateway_url` on provider token (user can set but only for OpenClaw)
2. **API Key** stored in different collections depending on whether it's a "connection" service or a "provider" service
3. **Service path** is admin-only -- users can't customize or create their own
4. **Node routing** is buried in the node detail page, disconnected from the service it routes

---

## The 4 Concepts

These are the fundamental primitives. They should be clearly separated and manageable from one place.

```mermaid
graph TB
    subgraph "1. Endpoint"
        EP["WHERE the request goes<br/>A target URL"]
        EP_EX1["api.openai.com/v1"]
        EP_EX2["http://localhost:18789"]
        EP_EX3["https://my-corp.internal/api"]
        EP --> EP_EX1 & EP_EX2 & EP_EX3
    end

    subgraph "2. API Key"
        AK["HOW to authenticate<br/>A credential"]
        AK_EX1["sk-proj-abc... (API key)"]
        AK_EX2["OAuth access token"]
        AK_EX3["bearer-token-xyz"]
        AK --> AK_EX1 & AK_EX2 & AK_EX3
    end

    subgraph "3. Service"
        SV["WHAT proxy path to use<br/>Binds endpoint + key + auth config"]
        SV_EX1["/proxy/s/openai/* -> endpoint + key"]
        SV_EX2["/proxy/s/my-api/* -> endpoint + key"]
        SV --> SV_EX1 & SV_EX2
    end

    subgraph "4. Node Route"
        NR["HOW to reach the endpoint<br/>Direct or via local node agent"]
        NR_EX1["Direct (default)"]
        NR_EX2["Via node 'my-laptop'"]
        NR --> NR_EX1 & NR_EX2
    end

    SV -.->|"uses"| EP
    SV -.->|"authenticates with"| AK
    SV -.->|"optionally routes via"| NR
```

---

## Proposed Architecture

### Design Principle

**Keep admin catalog intact. Give users 4 clear concepts they control in one place.**

Admin-managed catalog (stays as-is):
- `ProviderConfig` -- OAuth/device-code plumbing (authorization_url, token_url, etc.)
- `DownstreamService` -- seeded catalog of known services (provides defaults)
- `ServiceProviderRequirement` -- links catalog services to providers

User-managed (new):
- `user_endpoints` -- target URLs (from catalog defaults or custom)
- `user_api_keys` -- credentials (API keys, OAuth tokens, bearer tokens)
- `user_services` -- proxy routing config (binds endpoint + key + auth + node)

`NodeServiceBinding` is **absorbed into `user_services`** as `node_id` + `node_priority` fields.

### Entity Relationship

```mermaid
erDiagram
    DownstreamService {
        string id PK
        string slug UK
        string name
        string base_url "default URL"
        string default_auth_method
        string default_auth_key_name
        string provider_config_id FK
    }

    ProviderConfig {
        string id PK
        string slug UK
        string provider_type "oauth2 | api_key | device_code"
        string authorization_url
        string token_url
    }

    UserEndpoint {
        string id PK
        string user_id
        string label "My OpenAI, Local OpenClaw"
        string url "https://api.openai.com/v1"
        string catalog_service_id FK "optional: from catalog"
        datetime created_at
        datetime updated_at
    }

    UserApiKey {
        string id PK
        string user_id
        string label "Production Key"
        string credential_type "api_key | oauth2 | bearer | basic"
        bytes credential_encrypted "the API key or bearer token"
        bytes access_token_encrypted "OAuth only"
        bytes refresh_token_encrypted "OAuth only"
        string token_scopes "OAuth only"
        datetime expires_at "OAuth only"
        string provider_config_id FK "optional: for OAuth refresh"
        bytes user_oauth_client_id_encrypted "user-owned OAuth app"
        bytes user_oauth_client_secret_encrypted "user-owned OAuth app"
        string status "active | expired | revoked"
        datetime last_used_at
        string error_message
        datetime created_at
        datetime updated_at
    }

    UserService {
        string id PK
        string user_id
        string slug "proxy path slug"
        string endpoint_id FK "which endpoint"
        string api_key_id FK "which credential"
        string auth_method "bearer | header | query | basic"
        string auth_key_name "Authorization | x-api-key | key"
        string catalog_service_id FK "optional: from catalog"
        string node_id FK "optional: route via node"
        int node_priority "failover priority"
        bool is_active
        datetime created_at
        datetime updated_at
    }

    DownstreamService ||--o{ UserEndpoint : "catalog_service_id"
    DownstreamService ||--o{ UserService : "catalog_service_id"
    ProviderConfig ||--o{ UserApiKey : "provider_config_id"
    UserEndpoint ||--o{ UserService : "endpoint_id"
    UserApiKey ||--o{ UserService : "api_key_id"
```

### How the 4 Concepts Map to Collections

```mermaid
graph TB
    subgraph "Admin Catalog (unchanged)"
        DS["DownstreamService<br/>(19 seeded)"]
        PC["ProviderConfig<br/>(19 seeded)"]
        SPR["ServiceProviderRequirement<br/>(19 seeded)"]
    end

    subgraph "User Collections (new)"
        UE["user_endpoints<br/>Target URLs"]
        UAK["user_api_keys<br/>Credentials"]
        US["user_services<br/>Proxy routing"]
    end

    subgraph "Absorbed"
        NSB["NodeServiceBinding<br/>(old, absorbed into user_services)"]
        USC["UserServiceConnection<br/>(old, migrated to user_api_keys + user_services)"]
        UPT["UserProviderToken<br/>(old, migrated to user_api_keys)"]
        UPC["UserProviderCredentials<br/>(old, merged into user_api_keys)"]
    end

    DS -->|"provides defaults"| UE
    DS -->|"provides defaults"| US
    PC -->|"OAuth config"| UAK

    NSB -.->|"node_id field on"| US
    USC -.->|"credential -> "| UAK
    USC -.->|"connection -> "| US
    UPT -.->|"token/key -> "| UAK
    UPC -.->|"merged into"| UAK

    style UE fill:#4af,stroke:#333,stroke-width:2px
    style UAK fill:#4f8,stroke:#333,stroke-width:2px
    style US fill:#fa4,stroke:#333,stroke-width:2px
    style NSB fill:#ddd,stroke:#999,stroke-dasharray: 5
    style USC fill:#ddd,stroke:#999,stroke-dasharray: 5
    style UPT fill:#ddd,stroke:#999,stroke-dasharray: 5
    style UPC fill:#ddd,stroke:#999,stroke-dasharray: 5
```

---

## User Flows

### Flow 1: Add OpenAI API Key (from catalog)

```mermaid
sequenceDiagram
    participant User
    participant UI as Keys Page
    participant API
    participant DB

    User->>UI: Click "+ Add Key"
    UI->>UI: Show catalog (OpenAI, Anthropic, ...)
    User->>UI: Select "OpenAI"
    UI->>UI: Show form: API key input + label

    User->>API: POST /api/v1/keys<br/>{ service_slug: "llm-openai",<br/>  credential: "sk-proj-...",<br/>  label: "Production" }

    Note over API: Auto-provision from catalog:

    API->>DB: 1. Create UserEndpoint<br/>{ url: "https://api.openai.com/v1",<br/>  catalog_service_id: llm-openai,<br/>  label: "OpenAI" }

    API->>DB: 2. Create UserApiKey<br/>{ credential_encrypted: encrypt("sk-proj-..."),<br/>  credential_type: "api_key",<br/>  label: "Production" }

    API->>DB: 3. Create UserService<br/>{ slug: "llm-openai",<br/>  endpoint_id: [from step 1],<br/>  api_key_id: [from step 2],<br/>  auth_method: "bearer",<br/>  auth_key_name: "Authorization" }

    API-->>User: Done! Ready to proxy.

    User->>API: POST /proxy/s/llm-openai/chat/completions
    Note over API: Resolves: UserService -> endpoint + key -> forward
```

**User did 1 action** (paste key). System auto-provisioned all 3 records from catalog defaults.

### Flow 2: Add OpenClaw (endpoint required)

```mermaid
sequenceDiagram
    participant User
    participant UI as Keys Page
    participant API
    participant DB

    User->>UI: Click "+ Add Key"
    User->>UI: Select "OpenClaw" from catalog
    UI->>UI: Show form: API key + Endpoint URL (required)

    User->>API: POST /api/v1/keys<br/>{ service_slug: "llm-openclaw",<br/>  credential: "my-token",<br/>  endpoint_url: "http://localhost:18789",<br/>  label: "Local OpenClaw" }

    API->>DB: 1. Create UserEndpoint<br/>{ url: "http://localhost:18789",<br/>  label: "Local OpenClaw" }

    API->>DB: 2. Create UserApiKey<br/>{ credential_encrypted: encrypt("my-token"),<br/>  credential_type: "api_key" }

    API->>DB: 3. Create UserService<br/>{ slug: "llm-openclaw",<br/>  endpoint_id: ..., api_key_id: ...,<br/>  auth_method: "bearer" }

    Note over API: No fake URL. No gateway_url override.<br/>Endpoint stores the real URL directly.
```

### Flow 3: Change Endpoint URL (e.g., proxy OpenAI through local gateway)

```mermaid
sequenceDiagram
    participant User
    participant UI as Keys Page
    participant API
    participant DB

    User->>UI: Click on "OpenAI" key card
    UI->>UI: Show detail: endpoint, key, routing
    User->>UI: Edit endpoint URL

    User->>API: PUT /api/v1/endpoints/{endpoint_id}<br/>{ url: "http://localhost:8080/openai" }

    API->>DB: Update UserEndpoint.url

    Note over User: Now /proxy/s/llm-openai/*<br/>goes to localhost:8080 instead of api.openai.com
    Note over User: Same API key, just different target
```

### Flow 4: Add Node Routing

```mermaid
sequenceDiagram
    participant User
    participant UI as Keys Page
    participant API
    participant DB

    User->>UI: Click on "OpenAI" key card
    UI->>UI: Show detail view
    User->>UI: Under "Routing" section, click "Route via Node"
    UI->>UI: Show node picker (lists user's online nodes)
    User->>UI: Select "my-laptop" node

    User->>API: PUT /api/v1/services/{service_id}<br/>{ node_id: "my-laptop-id", node_priority: 0 }

    API->>DB: Update UserService.node_id

    Note over User: Now configured on the SAME page<br/>as the endpoint and key.<br/>Not buried in node detail page.
```

### Flow 5: Fully Custom Endpoint (no catalog)

```mermaid
sequenceDiagram
    participant User
    participant UI as Keys Page
    participant API

    User->>UI: Click "+ Add Key"
    User->>UI: Select "Custom Endpoint"
    UI->>UI: Show full form:<br/>URL + API key + auth method + auth key name

    User->>API: POST /api/v1/keys<br/>{ label: "Internal API",<br/>  endpoint_url: "https://internal.corp.com/api",<br/>  credential: "secret-token",<br/>  auth_method: "header",<br/>  auth_key_name: "X-API-Key" }

    Note over API: No catalog entry needed.<br/>User-defined endpoint + key + auth config.
```

### Flow 6: Multiple Keys for Same Endpoint

```mermaid
graph TB
    EP["UserEndpoint<br/>url: api.openai.com/v1"]

    KEY1["UserApiKey<br/>label: Dev Key<br/>sk-dev-..."]
    KEY2["UserApiKey<br/>label: Prod Key<br/>sk-prod-..."]

    SVC1["UserService<br/>slug: openai-dev<br/>endpoint + dev key"]
    SVC2["UserService<br/>slug: openai-prod<br/>endpoint + prod key"]

    EP --> SVC1
    EP --> SVC2
    KEY1 --> SVC1
    KEY2 --> SVC2

    PROXY1["/proxy/s/openai-dev/*<br/>Uses dev key"]
    PROXY2["/proxy/s/openai-prod/*<br/>Uses prod key"]

    SVC1 --> PROXY1
    SVC2 --> PROXY2
```

---

## Proxy Resolution: New vs Old

### New Path (clean)

```mermaid
flowchart TD
    REQ["POST /proxy/s/{slug}/path<br/>or /proxy/{service_id}/path"] --> FIND_SVC

    FIND_SVC{"Find UserService<br/>by slug + user_id<br/>or by catalog_service_id + user_id"}

    FIND_SVC -->|Found| LOAD["Load UserEndpoint + UserApiKey"]
    FIND_SVC -->|Not found| FALLBACK["Fallback: old model resolution<br/>(migration period)"]

    LOAD --> NODE{"UserService.node_id<br/>set?"}
    NODE -->|Yes| NODE_ROUTE["Route via node WebSocket<br/>(credential on node agent)"]
    NODE -->|No| DIRECT["Direct proxy"]

    DIRECT --> BUILD["Build request:<br/>URL = UserEndpoint.url + path<br/>Auth = decrypt(UserApiKey.credential)<br/>Method = UserService.auth_method"]

    BUILD --> FORWARD["Forward request"]
    NODE_ROUTE --> RESPONSE["Return response"]
    FORWARD --> RESPONSE

    FALLBACK --> OLD_PROVIDER["Old: UserProviderToken path"]
    FALLBACK --> OLD_CONN["Old: UserServiceConnection path"]
    OLD_PROVIDER --> FORWARD
    OLD_CONN --> FORWARD

    style FIND_SVC fill:#4f8,stroke:#333,stroke-width:2px
    style FALLBACK fill:#ff9,stroke:#999,stroke-dasharray: 5
```

### Backward Compatibility: Proxy Slug Resolution

```mermaid
flowchart TD
    SLUG["/proxy/s/llm-openai/chat/completions"] --> CHECK_NEW{"UserService exists<br/>with slug 'llm-openai'<br/>for this user?"}

    CHECK_NEW -->|Yes| USE_NEW["Use new path:<br/>UserService -> UserEndpoint + UserApiKey"]
    CHECK_NEW -->|No| CHECK_OLD{"DownstreamService exists<br/>with slug 'llm-openai'?"}

    CHECK_OLD -->|Yes| USE_OLD["Use old path:<br/>DownstreamService + UserProviderToken"]
    CHECK_OLD -->|No| ERR["404: Service not found"]

    Note over USE_NEW: New users and migrated users
    Note over USE_OLD: Existing users not yet migrated
```

---

## Frontend: One Page, 4 Sections

```mermaid
graph TB
    subgraph "Keys Page /keys"
        HEADER["My Keys + Add Key"]

        subgraph "Key Card (expanded)"
            SECTION1["ENDPOINT<br/>URL: api.openai.com/v1<br/>[Edit URL]"]
            SECTION2["API KEY<br/>sk-proj-...abc (masked)<br/>[Rotate] [Reveal]"]
            SECTION3["SERVICE<br/>Slug: llm-openai<br/>Auth: Bearer / Authorization<br/>Status: Active"]
            SECTION4["ROUTING<br/>Direct (no node)<br/>[Route via Node]"]
        end

        CARD1["OpenAI API -- Active"]
        CARD2["OpenClaw -- Active"]
        CARD3["My Internal API -- Active"]
        CARD4["GitHub OAuth -- Expires 2h"]
    end

    CARD1 -->|"expand"| SECTION1
    HEADER -->|"+ Add Key"| WIZARD

    subgraph "Add Key Wizard"
        W1["Pick from catalog<br/>or 'Custom Endpoint'"]
        W2A["Catalog: show API key input<br/>+ optional endpoint override"]
        W2B["Custom: show URL + key<br/>+ auth method config"]
        W1 -->|catalog| W2A
        W1 -->|custom| W2B
    end

    style SECTION1 fill:#4af,stroke:#333
    style SECTION2 fill:#4f8,stroke:#333
    style SECTION3 fill:#fa4,stroke:#333
    style SECTION4 fill:#a4f,stroke:#333
```

---

## API Design

### New Routes

```
# Keys (convenience: auto-provisions endpoint + api_key + service)
POST   /api/v1/keys                  Create from catalog or custom
GET    /api/v1/keys                  List all (endpoint + key + service combined view)
GET    /api/v1/keys/:id              Get combined view
DELETE /api/v1/keys/:id              Revoke (deactivates service + key)

# Endpoints (user-managed target URLs)
GET    /api/v1/endpoints             List user's endpoints
PUT    /api/v1/endpoints/:id         Update URL
DELETE /api/v1/endpoints/:id         Delete endpoint

# API Keys (credentials, like NyxID API keys)
GET    /api/v1/api-keys/external     List user's external API keys
PUT    /api/v1/api-keys/external/:id Update label, rotate credential
DELETE /api/v1/api-keys/external/:id Revoke key

# Services (proxy routing config)
GET    /api/v1/user-services         List user's service bindings
PUT    /api/v1/user-services/:id     Update auth config, node routing
DELETE /api/v1/user-services/:id     Deactivate service

# OAuth (for OAuth-type keys)
POST   /api/v1/keys/oauth/authorize  Start OAuth flow for a provider
GET    /api/v1/keys/oauth/callback   OAuth callback
POST   /api/v1/keys/:id/refresh      Force token refresh

# Catalog (read-only for users)
GET    /api/v1/catalog               List available service templates
GET    /api/v1/catalog/:slug         Get template details

# Old routes (kept as thin wrappers during migration)
*      /api/v1/connections/*         -> writes to new collections
*      /api/v1/providers/*/connect/* -> writes to new collections
```

### POST /api/v1/keys -- The Main Entry Point

This is the convenience endpoint that auto-provisions all 3 records:

```json
// From catalog (most common)
POST /api/v1/keys
{
  "service_slug": "llm-openai",
  "credential": "sk-proj-abc123",
  "label": "Production"
}
// -> Creates: UserEndpoint + UserApiKey + UserService (all defaults from catalog)

// From catalog with endpoint override (OpenClaw, or pointing OpenAI to local proxy)
POST /api/v1/keys
{
  "service_slug": "llm-openclaw",
  "credential": "my-bearer-token",
  "endpoint_url": "http://localhost:18789",
  "label": "Local OpenClaw"
}
// -> Creates: UserEndpoint (custom URL) + UserApiKey + UserService

// Fully custom (no catalog)
POST /api/v1/keys
{
  "label": "Internal API",
  "endpoint_url": "https://internal.corp.com/api",
  "credential": "secret-token",
  "auth_method": "header",
  "auth_key_name": "X-API-Key"
}
// -> Creates: UserEndpoint + UserApiKey + UserService (all user-defined)
```

---

## Migration Plan

### Phase 0: Add New Collections (no breaking changes)

```mermaid
graph TD
    P0A["Add UserEndpoint, UserApiKey, UserService models"] --> P0B["Add indexes on new collections"]
    P0B --> P0C["Add /api/v1/keys, /endpoints, /api-keys/external, /user-services routes"]
    P0C --> P0D["Add /api/v1/catalog route (reads DownstreamService)"]
```

### Phase 1: Dual-Write + Migration Script

```mermaid
graph TD
    P1A["Migration script runs at startup:<br/>UserProviderToken -> UserApiKey + UserEndpoint + UserService<br/>UserServiceConnection -> UserApiKey + UserService<br/>NodeServiceBinding -> node_id on UserService"] --> P1B["Old routes dual-write:<br/>/connections and /providers write to BOTH old and new collections"]
    P1B --> P1C["Proxy checks new collections FIRST,<br/>falls back to old"]
```

**Migration mapping:**

| Old Record | New Records |
|-----------|------------|
| `UserProviderToken` (api_key) | `UserApiKey` (credential) + `UserEndpoint` (from catalog or gateway_url) + `UserService` (binding) |
| `UserProviderToken` (oauth2) | `UserApiKey` (tokens) + `UserEndpoint` (from catalog) + `UserService` (binding) |
| `UserProviderToken` + `UserProviderCredentials` | `UserApiKey` (tokens + user OAuth app creds merged) + `UserEndpoint` + `UserService` |
| `UserServiceConnection` | `UserApiKey` (credential) + `UserService` (binding, endpoint from DownstreamService) |
| `NodeServiceBinding` | `node_id` + `node_priority` fields on `UserService` |

### Phase 2: Frontend

```mermaid
graph TD
    P2A["Build /keys page with catalog browser"] --> P2B["Build key detail view with 4 sections"]
    P2B --> P2C["Build Add Key wizard"]
    P2C --> P2D["Add node routing picker to service section"]
    P2D --> P2E["Sidebar: 'Keys' replaces 'Connections'"]
    P2E --> P2F["Old pages redirect to /keys"]
```

### Phase 3: Cleanup

```mermaid
graph TD
    P3A["Remove proxy fallback to old collections"] --> P3B["Old routes become thin wrappers over new collections"]
    P3B --> P3C["Remove NodeServiceBinding (absorbed)"]
    P3C --> P3D["Archive old user collections"]
```

---

## What Does NOT Change

| Component | Status |
|-----------|--------|
| Proxy URL paths (`/proxy/{service_id}/*`, `/proxy/s/{slug}/*`) | Unchanged |
| DownstreamService (catalog) | Unchanged (read as catalog) |
| ProviderConfig (OAuth/device-code config) | Unchanged |
| ServiceProviderRequirement | Unchanged |
| Admin pages | Unchanged |
| Node agent WebSocket protocol | Unchanged |
| Node registration + heartbeat | Unchanged |
| SSH tunneling | Unchanged |
| MCP proxy + delegation | Unchanged |
| OAuth/device-code flow logic | Same, writes to UserApiKey instead of UserProviderToken |

## What Changes

| Component | Change |
|-----------|--------|
| User credential storage | 3 old collections -> 3 new collections (cleaner separation) |
| Node service bindings | Separate collection -> field on UserService |
| Proxy credential resolution | 2 branching paths -> 1 path (UserService -> endpoint + key) |
| `resolve_gateway_url_override()` | Eliminated (endpoint_url on UserEndpoint) |
| Frontend | 3 pages -> 1 page with 4 sections |
| API routes | New `/keys` + `/endpoints` + `/api-keys/external` + `/user-services` + `/catalog` |
| Old routes | Kept as wrappers during migration |

---

## Summary

```mermaid
graph TB
    subgraph "BEFORE: 4 concepts scattered across 4 pages"
        direction LR
        B1["Services page<br/>(endpoint URL locked)"]
        B2["Connections page<br/>(API key for connections)"]
        B3["Providers page<br/>(API key for providers<br/>+ gateway_url mixed in)"]
        B4["Node page<br/>(service bindings buried)"]
    end

    subgraph "AFTER: 4 concepts, 1 page, clearly separated"
        direction LR
        A1["Endpoint<br/>User controls the URL"]
        A2["API Key<br/>Like NyxID API keys"]
        A3["Service<br/>Proxy path + auth config"]
        A4["Node Route<br/>Optional, right here"]
    end

    B1 & B2 & B3 & B4 -.->|"unified into"| KEYS["Keys Page /keys"]
    KEYS --> A1 & A2 & A3 & A4

    style KEYS fill:#4f8,stroke:#333,stroke-width:3px
    style B1 fill:#ddd,stroke:#999
    style B2 fill:#ddd,stroke:#999
    style B3 fill:#ddd,stroke:#999
    style B4 fill:#ddd,stroke:#999
```

**One page. Four clear concepts. Paste a key, use it. Change the URL, add a node, rotate the key -- all in the same place.**
