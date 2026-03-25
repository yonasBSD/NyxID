> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec: Streamline Services

This is the detailed implementation spec for the streamline services refactoring described in `docs/STREAMLINE_SERVICES_PROPOSAL.md`. Backend and frontend agents implement directly from this spec.

---

## Table of Contents

1. [New Rust Models](#1-new-rust-models)
2. [New Indexes](#2-new-indexes)
3. [New Service Functions](#3-new-service-functions)
4. [New Handlers + Routes](#4-new-handlers--routes)
5. [Old Route Wrappers](#5-old-route-wrappers)
6. [Migration Logic](#6-migration-logic)
7. [Frontend Spec](#7-frontend-spec)
8. [Error Codes](#8-error-codes)
9. [Complete File List](#9-complete-file-list)

---

## 1. New Rust Models

### 1.1 `backend/src/models/user_endpoint.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;

pub const COLLECTION_NAME: &str = "user_endpoints";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserEndpoint {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    pub label: String,
    /// Target URL (e.g., "https://api.openai.com/v1" or "http://localhost:18789")
    pub url: String,
    /// Optional: populated when auto-provisioned from catalog
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}
```

### 1.2 `backend/src/models/user_api_key.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::bson_datetime;

pub const COLLECTION_NAME: &str = "user_api_keys";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserApiKey {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    pub label: String,

    /// "api_key" | "oauth2" | "bearer" | "basic"
    pub credential_type: String,

    // --- Primary credential (encrypted) ---
    /// For api_key/bearer/basic: the raw credential
    #[serde(default, with = "crate::models::bson_bytes::optional")]
    pub credential_encrypted: Option<Vec<u8>>,

    // --- OAuth2 tokens (encrypted) ---
    #[serde(default, with = "crate::models::bson_bytes::optional")]
    pub access_token_encrypted: Option<Vec<u8>>,
    #[serde(default, with = "crate::models::bson_bytes::optional")]
    pub refresh_token_encrypted: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_scopes: Option<String>,
    #[serde(default, with = "bson_datetime::optional")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Optional: link to ProviderConfig for OAuth refresh
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<String>,

    // --- User-owned OAuth app credentials (merged from UserProviderCredentials) ---
    #[serde(default, with = "crate::models::bson_bytes::optional")]
    pub user_oauth_client_id_encrypted: Option<Vec<u8>>,
    #[serde(default, with = "crate::models::bson_bytes::optional")]
    pub user_oauth_client_secret_encrypted: Option<Vec<u8>>,

    /// "active" | "expired" | "revoked" | "refresh_failed"
    pub status: String,
    #[serde(default, with = "bson_datetime::optional")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Source tracking for migration: "migration_provider_token" | "migration_connection" | "user_created"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Original record ID from migration (for idempotency)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}
```

### 1.3 `backend/src/models/user_service.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const COLLECTION_NAME: &str = "user_services";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserService {
    #[serde(rename = "_id")]
    pub id: String,
    pub user_id: String,
    /// Proxy path slug (e.g., "llm-openai", "my-custom-api")
    pub slug: String,
    /// FK to UserEndpoint
    pub endpoint_id: String,
    /// FK to UserApiKey
    pub api_key_id: String,
    /// "bearer" | "header" | "query" | "basic" | "none"
    pub auth_method: String,
    /// Header name or query param name (e.g., "Authorization", "x-api-key", "key")
    pub auth_key_name: String,
    /// Optional: populated when auto-provisioned from catalog
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    /// Optional: route requests through this node agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Failover priority (lower = higher priority, default 0)
    #[serde(default)]
    pub node_priority: i32,
    pub is_active: bool,

    /// Source tracking for migration idempotency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}
```

### 1.4 Module Registration in `backend/src/models/mod.rs`

Add these lines (sorted alphabetically with existing modules):

```rust
pub mod user_api_key;
pub mod user_endpoint;
pub mod user_service;
```

---

## 2. New Indexes

Add to `ensure_indexes()` in `backend/src/db.rs`, after the existing `node_registration_tokens` indexes:

```rust
    // -- user_endpoints --
    let user_endpoints = db.collection::<mongodb::bson::Document>("user_endpoints");
    user_endpoints
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "catalog_service_id": 1 })
                .build(),
        )
        .await?;
    user_endpoints
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .build(),
        )
        .await?;

    // -- user_api_keys --
    let user_api_keys = db.collection::<mongodb::bson::Document>("user_api_keys");
    user_api_keys
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .build(),
        )
        .await?;
    user_api_keys
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "provider_config_id": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;
    user_api_keys
        .create_index(
            IndexModel::builder()
                .keys(doc! { "source": 1, "source_id": 1 })
                .options(
                    IndexOptions::builder()
                        .sparse(true)
                        .unique(true)
                        .build(),
                )
                .build(),
        )
        .await?;

    // -- user_services --
    let user_services = db.collection::<mongodb::bson::Document>("user_services");
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "slug": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .partial_filter_expression(doc! { "is_active": true })
                        .build(),
                )
                .build(),
        )
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "catalog_service_id": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "is_active": 1 })
                .build(),
        )
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "endpoint_id": 1 })
                .build(),
        )
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "api_key_id": 1 })
                .build(),
        )
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "source": 1, "source_id": 1 })
                .options(
                    IndexOptions::builder()
                        .sparse(true)
                        .unique(true)
                        .build(),
                )
                .build(),
        )
        .await?;
```

---

## 3. New Service Functions

### 3.1 `backend/src/services/user_endpoint_service.rs`

```rust
// All functions take &mongodb::Database as first arg.
// IDs are &str (per project convention).

/// List all endpoints for a user.
pub async fn list_endpoints(db: &Database, user_id: &str) -> AppResult<Vec<UserEndpoint>>
// Query: { user_id, } sorted by created_at desc

/// Get single endpoint by ID, verifying ownership.
pub async fn get_endpoint(db: &Database, user_id: &str, endpoint_id: &str) -> AppResult<UserEndpoint>
// Query: { _id: endpoint_id, user_id }

/// Create a new endpoint.
pub async fn create_endpoint(
    db: &Database,
    user_id: &str,
    label: &str,
    url: &str,
    catalog_service_id: Option<&str>,
) -> AppResult<UserEndpoint>
// Validates: label 1-200 chars, url is valid (reuse validate_base_url from handlers::services_helpers)
// Generates UUID v4 for id

/// Update endpoint URL and/or label.
pub async fn update_endpoint(
    db: &Database,
    user_id: &str,
    endpoint_id: &str,
    url: Option<&str>,
    label: Option<&str>,
) -> AppResult<()>
// $set only provided fields + updated_at
// Validates url if provided

/// Delete endpoint. Fails if any active UserService references it.
pub async fn delete_endpoint(db: &Database, user_id: &str, endpoint_id: &str) -> AppResult<()>
// Check: count user_services where endpoint_id = this and is_active = true
// If > 0, return AppError::Conflict("Endpoint is in use by active services")
// Otherwise: delete the document
```

### 3.2 `backend/src/services/user_api_key_service.rs`

```rust
/// List all API keys for a user (summary only, no decrypted values).
pub async fn list_api_keys(db: &Database, user_id: &str) -> AppResult<Vec<UserApiKey>>
// Query: { user_id } sorted by created_at desc

/// Get single API key by ID, verifying ownership.
pub async fn get_api_key(db: &Database, user_id: &str, key_id: &str) -> AppResult<UserApiKey>

/// Create a new API key with an encrypted credential.
pub async fn create_api_key(
    db: &Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    label: &str,
    credential_type: &str,
    credential: &str,
    provider_config_id: Option<&str>,
) -> AppResult<UserApiKey>
// Validates: label 1-200 chars, credential non-empty, credential.len() <= 8192
// Encrypts credential, generates UUID v4

/// Update label or rotate credential.
pub async fn update_api_key(
    db: &Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    key_id: &str,
    label: Option<&str>,
    credential: Option<&str>,
) -> AppResult<()>
// If credential provided: encrypt and $set credential_encrypted + updated_at
// If label provided: $set label + updated_at

/// Revoke an API key (sets status = "revoked").
/// Does NOT delete -- UserService references remain valid but credential won't decrypt.
pub async fn revoke_api_key(db: &Database, user_id: &str, key_id: &str) -> AppResult<()>
// $set status = "revoked", credential_encrypted = null, updated_at

/// Delete an API key. Fails if any active UserService references it.
pub async fn delete_api_key(db: &Database, user_id: &str, key_id: &str) -> AppResult<()>
// Check: count user_services where api_key_id = this and is_active = true
// If > 0, return Conflict
// Otherwise: delete

/// Update last_used_at timestamp (fire-and-forget, called from proxy).
pub async fn touch_last_used(db: &Database, key_id: &str)
// $set last_used_at = now, no error propagation
```

### 3.3 `backend/src/services/user_service_service.rs`

```rust
/// List all active user services for a user.
pub async fn list_user_services(db: &Database, user_id: &str) -> AppResult<Vec<UserService>>
// Query: { user_id, is_active: true } sorted by created_at desc

/// Get single user service by ID, verifying ownership.
pub async fn get_user_service(db: &Database, user_id: &str, service_id: &str) -> AppResult<UserService>

/// Find a user service by slug for a given user.
pub async fn find_by_slug(db: &Database, user_id: &str, slug: &str) -> AppResult<Option<UserService>>
// Query: { user_id, slug, is_active: true }

/// Find a user service by catalog_service_id for a given user.
pub async fn find_by_catalog_service_id(db: &Database, user_id: &str, catalog_service_id: &str) -> AppResult<Option<UserService>>
// Query: { user_id, catalog_service_id, is_active: true }

/// Create a new user service.
pub async fn create_user_service(
    db: &Database,
    user_id: &str,
    slug: &str,
    endpoint_id: &str,
    api_key_id: &str,
    auth_method: &str,
    auth_key_name: &str,
    catalog_service_id: Option<&str>,
    node_id: Option<&str>,
    node_priority: i32,
) -> AppResult<UserService>
// Validates: slug must be 1-64 chars, lowercase alphanumeric + hyphens
// Validates: auth_method is one of "bearer", "header", "query", "basic", "none"
// Validates: endpoint_id and api_key_id exist and belong to user_id
// Check uniqueness: no active service with same (user_id, slug)

/// Update service config (auth method, node routing, etc.).
pub async fn update_user_service(
    db: &Database,
    user_id: &str,
    service_id: &str,
    auth_method: Option<&str>,
    auth_key_name: Option<&str>,
    node_id: Option<&str>,      // Some("") clears, Some(id) sets, None leaves unchanged
    node_priority: Option<i32>,
    is_active: Option<bool>,
) -> AppResult<()>
// $set only provided fields + updated_at

/// Deactivate a user service (soft delete).
pub async fn deactivate_user_service(db: &Database, user_id: &str, service_id: &str) -> AppResult<()>
// $set is_active = false, updated_at
```

### 3.4 `backend/src/services/key_service.rs`

This is the **convenience orchestration service** that auto-provisions all 3 records.

```rust
use crate::crypto::aes::EncryptionKeys;
use crate::errors::{AppError, AppResult};

/// Result of creating a key (all 3 records).
pub struct CreateKeyResult {
    pub endpoint: UserEndpoint,
    pub api_key: UserApiKey,
    pub service: UserService,
}

/// Combined view for GET /keys and GET /keys/:id.
pub struct KeyView {
    pub id: String,              // UserService.id (the primary concept)
    pub label: String,           // UserApiKey.label
    pub slug: String,            // UserService.slug
    pub endpoint_url: String,    // UserEndpoint.url
    pub endpoint_id: String,     // UserEndpoint.id
    pub api_key_id: String,      // UserApiKey.id
    pub credential_type: String, // UserApiKey.credential_type
    pub auth_method: String,     // UserService.auth_method
    pub auth_key_name: String,   // UserService.auth_key_name
    pub status: String,          // UserApiKey.status
    pub catalog_service_id: Option<String>,
    pub catalog_service_name: Option<String>,
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub is_active: bool,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

/// POST /api/v1/keys -- auto-provision endpoint + api_key + service from catalog or custom.
///
/// From catalog (service_slug provided):
///   1. Look up DownstreamService by slug
///   2. Create UserEndpoint with url = endpoint_url override OR service.base_url
///   3. Create UserApiKey with encrypted credential
///   4. Create UserService with slug = service.slug, auth from service defaults
///
/// Custom (no service_slug):
///   1. Create UserEndpoint with provided endpoint_url
///   2. Create UserApiKey with encrypted credential
///   3. Create UserService with user-provided slug, auth_method, auth_key_name
pub async fn create_key(
    db: &Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    // From catalog
    service_slug: Option<&str>,
    // Custom or override
    endpoint_url: Option<&str>,
    credential: &str,
    label: &str,
    // Custom auth config (ignored when catalog provides defaults)
    slug_override: Option<&str>,
    auth_method: Option<&str>,
    auth_key_name: Option<&str>,
) -> AppResult<CreateKeyResult>
```

**Logic for `create_key`:**

```
if service_slug is provided:
    1. Find DownstreamService by slug (must exist and be active)
    2. Check if user already has a UserService with this catalog_service_id
       -> if so, return Conflict("You already have a key for this service")
    3. Determine endpoint URL:
       - if endpoint_url provided: use it
       - else if service.provider_config_id is set AND provider.requires_gateway_url:
            return BadRequest("This service requires an endpoint URL")
       - else: use service.base_url
    4. Determine credential_type:
       - Look up provider via service.provider_config_id
       - If provider exists: use provider.provider_type (api_key -> "api_key")
       - Else: map from service.auth_type or default to "api_key"
    5. Create UserEndpoint { url, label: service.name, catalog_service_id: service.id }
    6. Create UserApiKey { credential_encrypted, credential_type, label, provider_config_id }
    7. Create UserService {
         slug: service.slug,
         endpoint_id, api_key_id,
         auth_method: service.auth_method (map "bearer" correctly),
         auth_key_name: service.auth_key_name,
         catalog_service_id: service.id,
       }
else (custom):
    1. endpoint_url is required -> BadRequest if missing
    2. slug_override is required -> BadRequest if missing ("Slug is required for custom endpoints")
    3. auth_method defaults to "bearer", auth_key_name defaults to "Authorization"
    4. Create UserEndpoint { url: endpoint_url, label }
    5. Create UserApiKey { credential_encrypted, credential_type: "api_key", label }
    6. Create UserService {
         slug: slug_override,
         endpoint_id, api_key_id,
         auth_method, auth_key_name,
       }
```

```rust
/// GET /api/v1/keys -- list all keys as combined views.
pub async fn list_keys(db: &Database, user_id: &str) -> AppResult<Vec<KeyView>>
// 1. Load all active UserService for user
// 2. Batch-load all referenced UserEndpoint and UserApiKey by ID
// 3. Optionally batch-load DownstreamService for catalog_service_id -> name
// 4. Assemble KeyView for each

/// GET /api/v1/keys/:id -- get single combined view.
pub async fn get_key(db: &Database, user_id: &str, service_id: &str) -> AppResult<KeyView>
// Load UserService, then its endpoint and api_key, assemble KeyView

/// DELETE /api/v1/keys/:id -- revoke key (deactivate service + revoke api_key).
pub async fn revoke_key(db: &Database, user_id: &str, service_id: &str) -> AppResult<()>
// 1. Load UserService (verify ownership)
// 2. Deactivate UserService (is_active = false)
// 3. Revoke UserApiKey (status = "revoked", clear credential)
// Note: does NOT delete UserEndpoint (may be shared)
```

### 3.5 Proxy Service Changes (`backend/src/services/proxy_service.rs`)

Add a new function alongside existing `resolve_proxy_target`:

```rust
/// Resolve proxy target from the new UserService model.
///
/// Returns Ok(Some(ProxyTarget)) if a UserService exists for this user+slug/service_id.
/// Returns Ok(None) to signal the caller should fall back to old resolution.
pub async fn resolve_proxy_target_from_user_service(
    db: &Database,
    encryption_keys: &EncryptionKeys,
    user_id: &str,
    slug: Option<&str>,
    catalog_service_id: Option<&str>,
) -> AppResult<Option<(ProxyTarget, Option<String>)>>
// Returns (ProxyTarget, Option<node_id>)
```

**Logic:**

```
1. Find UserService:
   - If slug provided: find by (user_id, slug, is_active: true)
   - Elif catalog_service_id provided: find by (user_id, catalog_service_id, is_active: true)
   - If not found: return Ok(None) (fall back to old path)

2. Load UserEndpoint by user_service.endpoint_id
   - If not found: return Internal error (data integrity issue)

3. Load UserApiKey by user_service.api_key_id
   - If not found: return Internal error
   - If status != "active": return BadRequest("API key is {status}")

4. If user_service.auth_method == "none":
   - Return ProxyTarget with empty credential

5. Decrypt credential:
   - For credential_type "api_key" | "bearer" | "basic": decrypt credential_encrypted
   - For credential_type "oauth2": decrypt access_token_encrypted

6. Fire-and-forget: update UserApiKey.last_used_at

7. Build ProxyTarget:
   - base_url = UserEndpoint.url
   - auth_method = UserService.auth_method
   - auth_key_name = UserService.auth_key_name
   - credential = decrypted string
   - service = construct a minimal DownstreamService for compatibility
     (or refactor ProxyTarget to not require DownstreamService -- preferred)

8. Return Ok(Some((target, user_service.node_id)))
```

**Integration into `execute_proxy` (in `handlers/proxy.rs`):**

Modify the proxy handler to try new path first, fall back to old:

```
// In proxy_request_by_slug:
// 1. Try resolve_proxy_target_from_user_service(db, keys, user_id, slug=Some(slug), catalog_service_id=None)
// 2. If Some((target, node_id)):
//      - If node_id is set: use it for node routing (skip resolve_node_route)
//      - Else: proceed with direct proxy using target
// 3. If None: fall back to existing resolve_service_by_slug + execute_proxy flow

// In proxy_request (by UUID service_id):
// 1. Try resolve_proxy_target_from_user_service(db, keys, user_id, slug=None, catalog_service_id=Some(service_id))
// 2. If Some: use new path
// 3. If None: fall back to existing resolve_proxy_target flow
```

### 3.6 Catalog Service (`backend/src/services/catalog_service.rs`)

```rust
/// A catalog entry combining DownstreamService + ProviderConfig info.
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub base_url: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub provider_type: Option<String>,       // "api_key" | "oauth2" | "device_code"
    pub requires_gateway_url: bool,
    pub api_key_instructions: Option<String>,
    pub api_key_url: Option<String>,
    pub icon_url: Option<String>,
    pub documentation_url: Option<String>,
}

/// List catalog entries available for user key creation.
/// Filters to connection-category + provider-linked services.
pub async fn list_catalog(db: &Database) -> AppResult<Vec<CatalogEntry>>
// 1. Load all active DownstreamService where:
//    service_type = "http" AND service_category IN ("connection", "internal")
//    AND (requires_user_credential = true OR provider_config_id IS NOT NULL)
// 2. For each, optionally join ProviderConfig by provider_config_id
// 3. Build CatalogEntry

/// Get single catalog entry by slug.
pub async fn get_catalog_entry(db: &Database, slug: &str) -> AppResult<CatalogEntry>
```

### 3.7 Module Registration in `backend/src/services/mod.rs`

Add:

```rust
pub mod catalog_service;
pub mod key_service;
pub mod user_api_key_service;
pub mod user_endpoint_service;
pub mod user_service_service;
```

---

## 4. New Handlers + Routes

### 4.1 `backend/src/handlers/keys.rs`

**Request/Response types:**

```rust
#[derive(Deserialize)]
pub struct CreateKeyRequest {
    /// Catalog service slug (e.g., "llm-openai"). Mutually exclusive with full custom config.
    pub service_slug: Option<String>,
    /// The credential value (API key, bearer token, etc.)
    pub credential: String,
    /// User-facing label
    pub label: String,
    /// Endpoint URL override (required for self-hosted providers and custom endpoints)
    pub endpoint_url: Option<String>,
    /// Custom slug (required when service_slug is None)
    pub slug: Option<String>,
    /// Custom auth method (default: "bearer")
    pub auth_method: Option<String>,
    /// Custom auth key name (default: "Authorization")
    pub auth_key_name: Option<String>,
}

// Custom Debug impl: redact credential field
impl std::fmt::Debug for CreateKeyRequest { ... }

#[derive(Debug, Serialize)]
pub struct KeyResponse {
    pub id: String,
    pub label: String,
    pub slug: String,
    pub endpoint_url: String,
    pub endpoint_id: String,
    pub api_key_id: String,
    pub credential_type: String,
    pub auth_method: String,
    pub auth_key_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct KeyListResponse {
    pub keys: Vec<KeyResponse>,
}

#[derive(Debug, Serialize)]
pub struct DeleteKeyResponse {
    pub message: String,
}
```

**Handler functions:**

```rust
/// POST /api/v1/keys
pub async fn create_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateKeyRequest>,
) -> AppResult<Json<KeyResponse>>

/// GET /api/v1/keys
pub async fn list_keys(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> AppResult<Json<KeyListResponse>>

/// GET /api/v1/keys/{key_id}
pub async fn get_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<KeyResponse>>

/// DELETE /api/v1/keys/{key_id}
pub async fn delete_key(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> AppResult<Json<DeleteKeyResponse>>
```

### 4.2 `backend/src/handlers/user_endpoints.rs`

```rust
#[derive(Deserialize)]
pub struct UpdateEndpointRequest {
    pub url: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EndpointResponse {
    pub id: String,
    pub label: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct EndpointListResponse {
    pub endpoints: Vec<EndpointResponse>,
}

/// GET /api/v1/endpoints
pub async fn list_endpoints(State, AuthUser) -> AppResult<Json<EndpointListResponse>>

/// PUT /api/v1/endpoints/{endpoint_id}
pub async fn update_endpoint(State, AuthUser, Path, Json) -> AppResult<Json<EndpointResponse>>

/// DELETE /api/v1/endpoints/{endpoint_id}
pub async fn delete_endpoint(State, AuthUser, Path) -> AppResult<impl IntoResponse>
// Returns 204 No Content
```

### 4.3 `backend/src/handlers/user_api_keys_external.rs`

```rust
#[derive(Deserialize)]
pub struct UpdateExternalApiKeyRequest {
    pub label: Option<String>,
    pub credential: Option<String>,
}

// Custom Debug impl for credential redaction

#[derive(Debug, Serialize)]
pub struct ExternalApiKeyResponse {
    pub id: String,
    pub label: String,
    pub credential_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ExternalApiKeyListResponse {
    pub api_keys: Vec<ExternalApiKeyResponse>,
}

/// GET /api/v1/api-keys/external
pub async fn list_external_api_keys(State, AuthUser) -> AppResult<Json<ExternalApiKeyListResponse>>

/// PUT /api/v1/api-keys/external/{key_id}
pub async fn update_external_api_key(State, AuthUser, Path, Json) -> AppResult<Json<ExternalApiKeyResponse>>

/// DELETE /api/v1/api-keys/external/{key_id}
pub async fn delete_external_api_key(State, AuthUser, Path) -> AppResult<impl IntoResponse>
```

### 4.4 `backend/src/handlers/user_services_handler.rs`

```rust
#[derive(Deserialize)]
pub struct UpdateUserServiceRequest {
    pub auth_method: Option<String>,
    pub auth_key_name: Option<String>,
    pub node_id: Option<String>,          // "" to clear
    pub node_priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UserServiceResponse {
    pub id: String,
    pub slug: String,
    pub endpoint_id: String,
    pub api_key_id: String,
    pub auth_method: String,
    pub auth_key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub node_priority: i32,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct UserServiceListResponse {
    pub services: Vec<UserServiceResponse>,
}

/// GET /api/v1/user-services
pub async fn list_user_services(State, AuthUser) -> AppResult<Json<UserServiceListResponse>>

/// PUT /api/v1/user-services/{service_id}
pub async fn update_user_service(State, AuthUser, Path, Json) -> AppResult<Json<UserServiceResponse>>

/// DELETE /api/v1/user-services/{service_id}
pub async fn delete_user_service(State, AuthUser, Path) -> AppResult<impl IntoResponse>
```

### 4.5 `backend/src/handlers/catalog.rs`

```rust
#[derive(Debug, Serialize)]
pub struct CatalogEntryResponse {
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub base_url: String,
    pub auth_method: String,
    pub auth_key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<String>,
    pub requires_gateway_url: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogListResponse {
    pub entries: Vec<CatalogEntryResponse>,
}

/// GET /api/v1/catalog
pub async fn list_catalog(State, AuthUser) -> AppResult<Json<CatalogListResponse>>

/// GET /api/v1/catalog/{slug}
pub async fn get_catalog_entry(State, AuthUser, Path) -> AppResult<Json<CatalogEntryResponse>>
```

### 4.6 Handler Module Registration (`backend/src/handlers/mod.rs`)

Add:

```rust
pub mod catalog;
pub mod keys;
pub mod user_api_keys_external;
pub mod user_endpoints;
pub mod user_services_handler;
```

### 4.7 Route Definitions (`backend/src/routes.rs`)

Add these route groups to `build_router()`:

```rust
    let key_routes = Router::new()
        .route("/", get(handlers::keys::list_keys).post(handlers::keys::create_key))
        .route("/{key_id}", get(handlers::keys::get_key).delete(handlers::keys::delete_key));

    let user_endpoint_routes = Router::new()
        .route("/", get(handlers::user_endpoints::list_endpoints))
        .route(
            "/{endpoint_id}",
            put(handlers::user_endpoints::update_endpoint)
                .delete(handlers::user_endpoints::delete_endpoint),
        );

    let external_api_key_routes = Router::new()
        .route("/", get(handlers::user_api_keys_external::list_external_api_keys))
        .route(
            "/{key_id}",
            put(handlers::user_api_keys_external::update_external_api_key)
                .delete(handlers::user_api_keys_external::delete_external_api_key),
        );

    let user_service_routes = Router::new()
        .route("/", get(handlers::user_services_handler::list_user_services))
        .route(
            "/{service_id}",
            put(handlers::user_services_handler::update_user_service)
                .delete(handlers::user_services_handler::delete_user_service),
        );

    let catalog_routes = Router::new()
        .route("/", get(handlers::catalog::list_catalog))
        .route("/{slug}", get(handlers::catalog::get_catalog_entry));
```

Add these nests inside `api_v1_human_only` (after `.nest("/nodes", node_routes)`):

```rust
        .nest("/keys", key_routes)
        .nest("/endpoints", user_endpoint_routes)
        .nest("/api-keys/external", external_api_key_routes)
        .nest("/user-services", user_service_routes)
        .nest("/catalog", catalog_routes)
```

**Note:** `/api-keys/external` does NOT conflict with existing `/api-keys` (NyxID API keys). The `external` sub-path differentiates them.

---

## 5. Old Route Wrappers

During migration, existing routes continue to work. The old handlers are NOT modified in Phase 0. In Phase 1, dual-write is added:

### Phase 1 Dual-Write (for a future PR, not in this initial implementation)

When `/connections/{service_id}` `POST` is called:
1. Execute existing `connection_service::connect_user` (old path)
2. Additionally call `key_service::create_key` with service_slug from the DownstreamService

When `/providers/{provider_id}/connect/api-key` `POST` is called:
1. Execute existing `user_token_service::store_api_key` (old path)
2. Additionally call `key_service::create_key` with the provider's linked service slug

**For Phase 0 (this spec):** No changes to old handlers. The migration script (Section 6) handles backfilling existing data.

---

## 6. Migration Logic

Add to `backend/src/db.rs` (called from `create_connection` after `ensure_indexes`):

```rust
/// Migrate existing user data to the new unified collections.
/// Idempotent: uses source + source_id to skip already-migrated records.
pub async fn migrate_to_unified_collections(
    db: &Database,
    encryption_keys: &EncryptionKeys,
) -> Result<(), Box<dyn std::error::Error>> {
    migrate_provider_tokens(db, encryption_keys).await?;
    migrate_service_connections(db, encryption_keys).await?;
    migrate_node_service_bindings(db).await?;
    Ok(())
}
```

### 6.1 `migrate_provider_tokens`

```
For each UserProviderToken:
  1. Check if UserApiKey exists with source="migration_provider_token", source_id=token.id
     -> If yes, skip (idempotent)
  2. Find the DownstreamService linked to this provider (via provider_config_id)
  3. Create UserEndpoint:
     - url: token.gateway_url OR service.base_url (if service exists)
     - label: provider.name (from ProviderConfig lookup)
     - catalog_service_id: service.id (if service exists)
  4. Create UserApiKey:
     - credential_type: token.token_type
     - credential_encrypted: token.api_key_encrypted (for api_key) or None (for oauth2)
     - access_token_encrypted: token.access_token_encrypted
     - refresh_token_encrypted: token.refresh_token_encrypted
     - token_scopes: token.token_scopes
     - expires_at: token.expires_at
     - provider_config_id: token.provider_config_id
     - status: token.status
     - label: token.label OR provider.name
     - last_used_at: token.last_used_at
     - error_message: token.error_message
     - source: "migration_provider_token"
     - source_id: token.id
  5. Merge UserProviderCredentials (if exists for same user_id + provider_config_id):
     - user_oauth_client_id_encrypted: cred.client_id_encrypted
     - user_oauth_client_secret_encrypted: cred.client_secret_encrypted
  6. Create UserService:
     - slug: service.slug (if service exists, else provider.slug)
     - endpoint_id, api_key_id: from steps 3-4
     - auth_method: service.auth_method (or "bearer" default)
     - auth_key_name: service.auth_key_name (or "Authorization" default)
     - catalog_service_id: service.id (if exists)
     - source: "migration_provider_token"
     - source_id: token.id
```

### 6.2 `migrate_service_connections`

```
For each UserServiceConnection where is_active = true:
  1. Check if UserApiKey exists with source="migration_connection", source_id=conn.id
     -> If yes, skip
  2. Load DownstreamService by conn.service_id
  3. Check if a UserService already exists for this user + catalog_service_id
     -> If yes, skip (already migrated via provider token path)
  4. Create UserEndpoint:
     - url: service.base_url
     - label: service.name
     - catalog_service_id: service.id
  5. Create UserApiKey:
     - credential_type: conn.credential_type OR service.auth_type OR "api_key"
     - credential_encrypted: conn.credential_encrypted
     - status: "active"
     - label: conn.credential_label OR service.name
     - source: "migration_connection"
     - source_id: conn.id
  6. Create UserService:
     - slug: service.slug
     - endpoint_id, api_key_id: from above
     - auth_method: service.auth_method
     - auth_key_name: service.auth_key_name
     - catalog_service_id: service.id
     - source: "migration_connection"
     - source_id: conn.id
```

### 6.3 `migrate_node_service_bindings`

```
For each NodeServiceBinding where is_active = true:
  1. Find the UserService for this user + service_id (via catalog_service_id)
     -> If not found, skip (the service connection wasn't migrated)
  2. Update UserService:
     - $set node_id = binding.node_id
     - $set node_priority = binding.priority
```

### 6.4 Startup Integration

In `backend/src/db.rs`, `create_connection` function, after `ensure_indexes(&db).await?;`:

```rust
    // Run unified collection migration (idempotent)
    if let Err(e) = migrate_to_unified_collections(&db, encryption_keys).await {
        tracing::warn!("Unified collection migration encountered errors: {e}");
        // Non-fatal: don't block startup
    }
```

**Note:** `create_connection` needs `encryption_keys: &EncryptionKeys` added to its signature. Update `main.rs` to pass it after EncryptionKeys construction.

---

## 7. Frontend Spec

### 7.1 New TypeScript Types (`frontend/src/types/keys.ts`)

```typescript
export interface KeyInfo {
  readonly id: string;
  readonly label: string;
  readonly slug: string;
  readonly endpoint_url: string;
  readonly endpoint_id: string;
  readonly api_key_id: string;
  readonly credential_type: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly status: string;
  readonly catalog_service_id: string | null;
  readonly catalog_service_name: string | null;
  readonly node_id: string | null;
  readonly node_priority: number;
  readonly is_active: boolean;
  readonly expires_at: string | null;
  readonly last_used_at: string | null;
  readonly error_message: string | null;
  readonly created_at: string;
}

export interface KeyListResponse {
  readonly keys: readonly KeyInfo[];
}

export interface CatalogEntry {
  readonly slug: string;
  readonly name: string;
  readonly description: string | null;
  readonly base_url: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly provider_type: string | null;
  readonly requires_gateway_url: boolean;
  readonly api_key_instructions: string | null;
  readonly api_key_url: string | null;
  readonly icon_url: string | null;
  readonly documentation_url: string | null;
}

export interface CatalogListResponse {
  readonly entries: readonly CatalogEntry[];
}

export interface UserEndpointInfo {
  readonly id: string;
  readonly label: string;
  readonly url: string;
  readonly catalog_service_id: string | null;
  readonly created_at: string;
  readonly updated_at: string;
}

export interface UserServiceInfo {
  readonly id: string;
  readonly slug: string;
  readonly endpoint_id: string;
  readonly api_key_id: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly catalog_service_id: string | null;
  readonly node_id: string | null;
  readonly node_priority: number;
  readonly is_active: boolean;
  readonly created_at: string;
  readonly updated_at: string;
}
```

### 7.2 New Hooks (`frontend/src/hooks/use-keys.ts`)

```typescript
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type { KeyInfo, KeyListResponse, CatalogEntry, CatalogListResponse } from "@/types/keys";

// -- Queries --

export function useKeys() {
  return useQuery({
    queryKey: ["keys"],
    queryFn: async (): Promise<readonly KeyInfo[]> => {
      const res = await api.get<KeyListResponse>("/keys");
      return res.keys;
    },
  });
}

export function useKey(keyId: string) {
  return useQuery({
    queryKey: ["keys", keyId],
    queryFn: async (): Promise<KeyInfo> => {
      return api.get<KeyInfo>(`/keys/${keyId}`);
    },
    enabled: Boolean(keyId),
  });
}

export function useCatalog() {
  return useQuery({
    queryKey: ["catalog"],
    queryFn: async (): Promise<readonly CatalogEntry[]> => {
      const res = await api.get<CatalogListResponse>("/catalog");
      return res.entries;
    },
  });
}

// -- Mutations --

interface CreateKeyParams {
  readonly service_slug?: string;
  readonly credential: string;
  readonly label: string;
  readonly endpoint_url?: string;
  readonly slug?: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
}

export function useCreateKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: CreateKeyParams): Promise<KeyInfo> => {
      return api.post<KeyInfo>("/keys", params);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({ queryKey: ["llm-status"] });
    },
  });
}

export function useDeleteKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (keyId: string): Promise<void> => {
      return api.delete<void>(`/keys/${keyId}`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({ queryKey: ["llm-status"] });
    },
  });
}

interface UpdateEndpointParams {
  readonly endpointId: string;
  readonly url?: string;
  readonly label?: string;
}

export function useUpdateEndpoint() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateEndpointParams): Promise<void> => {
      return api.put<void>(`/endpoints/${params.endpointId}`, {
        url: params.url,
        label: params.label,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}

interface UpdateUserServiceParams {
  readonly serviceId: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
  readonly node_id?: string;
  readonly node_priority?: number;
  readonly is_active?: boolean;
}

export function useUpdateUserService() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateUserServiceParams): Promise<void> => {
      const { serviceId, ...body } = params;
      return api.put<void>(`/user-services/${serviceId}`, body);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}

interface UpdateExternalApiKeyParams {
  readonly keyId: string;
  readonly label?: string;
  readonly credential?: string;
}

export function useUpdateExternalApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateExternalApiKeyParams): Promise<void> => {
      const { keyId, ...body } = params;
      return api.put<void>(`/api-keys/external/${keyId}`, body);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}
```

### 7.3 New Pages

#### `frontend/src/pages/keys.tsx` -- Key List Page

- Page header: "Keys" with "+ Add Key" button
- Lists all keys from `useKeys()` as cards
- Each card shows: label, slug, status badge, endpoint URL (truncated), credential type
- Card click navigates to `/keys/{id}`
- "+ Add Key" opens the Add Key Wizard (inline or dialog)

**Add Key Wizard** (can be a dialog or inline section):

1. Step 1: Pick from catalog (grid of CatalogEntry cards from `useCatalog()`) or "Custom Endpoint"
2. Step 2a (catalog): Show API key input field + optional endpoint URL override (shown if `requires_gateway_url`) + label field
3. Step 2b (custom): Show all fields: URL, API key, slug, auth method dropdown, auth key name
4. Submit calls `useCreateKey()`

#### `frontend/src/pages/key-detail.tsx` -- Key Detail Page

- Route: `/keys/$keyId`
- Loads key via `useKey(keyId)`
- Four sections in a card layout:

**Section 1: Endpoint**
- Shows URL with inline edit button
- Edit calls `useUpdateEndpoint({ endpointId, url })`

**Section 2: API Key**
- Shows masked credential type and status
- "Rotate" button calls `useUpdateExternalApiKey({ keyId, credential: newValue })`
- Status badge (active/expired/revoked)

**Section 3: Service**
- Shows slug (read-only for catalog-provisioned, editable for custom)
- Auth method and auth key name
- Active/inactive toggle

**Section 4: Routing**
- Shows "Direct" if no node_id, or node name if set
- "Route via Node" button opens node picker (list from `useNodes()`)
- Calls `useUpdateUserService({ serviceId, node_id })`

**Delete button** at bottom: calls `useDeleteKey()`

### 7.4 Lazy Page Exports (`frontend/src/pages/lazy.ts`)

Add:

```typescript
export const KeysPage = lazy(() =>
  import("./keys").then((m) => ({ default: m.KeysPage })),
);
export const KeyDetailPage = lazy(() =>
  import("./key-detail").then((m) => ({ default: m.KeyDetailPage })),
);
```

### 7.5 Router Changes (`frontend/src/router.tsx`)

Add imports:

```typescript
import { KeysPage, KeyDetailPage } from "@/pages/lazy";
```

Add route definitions:

```typescript
const keysRoute = createRoute({
  path: "/keys",
  getParentRoute: () => dashboardLayout,
  component: KeysPage,
});

const keyDetailRoute = createRoute({
  path: "/keys/$keyId",
  getParentRoute: () => dashboardLayout,
  component: KeyDetailPage,
});
```

Add to `dashboardLayout.addChildren([...])`:

```typescript
    keysRoute,
    keyDetailRoute,
```

### 7.6 Sidebar Navigation Changes (`frontend/src/components/dashboard/sidebar.tsx`)

Update `NAV_ITEMS` to add Keys and reorder:

```typescript
const NAV_ITEMS = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
  { to: "/keys", icon: KeyRound, label: "Keys" },           // NEW
  { to: "/api-keys", icon: Key, label: "API Keys" },
  { to: "/services", icon: Server, label: "Services" },
  { to: "/connections", icon: Link2, label: "Connections" },  // Keep during migration
  { to: "/providers", icon: Plug, label: "Providers" },       // Keep during migration
  { to: "/nodes", icon: HardDrive, label: "Nodes" },
  { to: "/settings", icon: Settings, label: "Settings" },
  { to: "/settings/consents", icon: KeyRound, label: "Authorized Apps" },
  { to: "/guide", icon: BookOpen, label: "Guide" },
] as const;
```

**Note:** `KeyRound` import already exists. Use a different icon for "Keys" to distinguish from "Authorized Apps". Consider using `Cable` or `Unplug` from lucide-react. Final icon choice is up to the frontend implementer, but `KeyRound` is a reasonable default.

### 7.7 AI Setup Page Updates (`frontend/src/pages/ai-setup.tsx`)

Update the setup instructions to reference the new Keys page for adding API keys. Add a prominent link/button:

```
"Go to Keys page to add your API keys" -> Link to /keys
```

The exact text and placement is at the frontend implementer's discretion. The key change is pointing users to `/keys` instead of `/providers`.

### 7.8 Old Pages: Keep During Migration

- `/connections` -- keep as-is (reads from old collections)
- `/providers` -- keep as-is (reads from old collections)
- No redirects yet. Phase 3 (cleanup) will handle redirects.

---

## 8. Error Codes

No new `AppError` variants are needed. The existing variants cover all cases:

| Scenario | Existing Variant | Code |
|----------|-----------------|------|
| Key/endpoint/service not found | `NotFound` | 1003 |
| Duplicate slug | `DuplicateSlug` | 4006 |
| Already have key for service | `Conflict` | 1004 |
| Endpoint in use (can't delete) | `Conflict` | 1004 |
| Credential validation failed | `ValidationError` | 1008 |
| Invalid auth method | `BadRequest` | 1000 |
| Missing required field | `BadRequest` | 1000 |

---

## 9. Complete File List

### Files to CREATE

| Path | Description |
|------|-------------|
| `backend/src/models/user_endpoint.rs` | UserEndpoint model (Section 1.1) |
| `backend/src/models/user_api_key.rs` | UserApiKey model (Section 1.2) |
| `backend/src/models/user_service.rs` | UserService model (Section 1.3) |
| `backend/src/services/user_endpoint_service.rs` | Endpoint CRUD service (Section 3.1) |
| `backend/src/services/user_api_key_service.rs` | API key CRUD service (Section 3.2) |
| `backend/src/services/user_service_service.rs` | User service CRUD (Section 3.3) |
| `backend/src/services/key_service.rs` | Convenience orchestration service (Section 3.4) |
| `backend/src/services/catalog_service.rs` | Catalog listing service (Section 3.6) |
| `backend/src/handlers/keys.rs` | Keys handler (Section 4.1) |
| `backend/src/handlers/user_endpoints.rs` | Endpoints handler (Section 4.2) |
| `backend/src/handlers/user_api_keys_external.rs` | External API keys handler (Section 4.3) |
| `backend/src/handlers/user_services_handler.rs` | User services handler (Section 4.4) |
| `backend/src/handlers/catalog.rs` | Catalog handler (Section 4.5) |
| `frontend/src/types/keys.ts` | TypeScript types (Section 7.1) |
| `frontend/src/hooks/use-keys.ts` | TanStack Query hooks (Section 7.2) |
| `frontend/src/pages/keys.tsx` | Keys list page (Section 7.3) |
| `frontend/src/pages/key-detail.tsx` | Key detail page (Section 7.3) |

### Files to MODIFY

| Path | Changes |
|------|---------|
| `backend/src/models/mod.rs` | Add `pub mod user_endpoint`, `user_api_key`, `user_service` |
| `backend/src/services/mod.rs` | Add `pub mod` for 5 new services |
| `backend/src/handlers/mod.rs` | Add `pub mod` for 5 new handlers |
| `backend/src/db.rs` | Add indexes (Section 2), add migration function (Section 6), update `create_connection` signature |
| `backend/src/routes.rs` | Add route groups for keys, endpoints, external API keys, user-services, catalog (Section 4.7) |
| `backend/src/main.rs` | Pass `encryption_keys` to `create_connection` (Section 6.4) |
| `backend/src/services/proxy_service.rs` | Add `resolve_proxy_target_from_user_service` (Section 3.5) |
| `backend/src/handlers/proxy.rs` | Try new resolution first, fall back to old (Section 3.5) |
| `frontend/src/pages/lazy.ts` | Add lazy exports for KeysPage, KeyDetailPage |
| `frontend/src/router.tsx` | Add /keys and /keys/$keyId routes |
| `frontend/src/components/dashboard/sidebar.tsx` | Add "Keys" nav item |

### Files NOT Modified (but referenced for conventions)

All existing model, service, handler, and frontend files remain untouched. The old collections (`user_provider_tokens`, `user_service_connections`, `user_provider_credentials`, `node_service_bindings`) continue to function. Cleanup is Phase 3 (future PR).

---

## Implementation Order

1. **Backend Models** -- Create 3 model files + register in mod.rs
2. **Backend Indexes** -- Add to db.rs
3. **Backend Services** -- Create 5 service files + register in mod.rs
4. **Backend Handlers** -- Create 5 handler files + register in mod.rs
5. **Backend Routes** -- Wire up in routes.rs
6. **Backend Proxy Integration** -- Add new resolution to proxy_service + proxy handler
7. **Backend Migration** -- Add migration logic to db.rs + update main.rs
8. **Frontend Types** -- Create types/keys.ts
9. **Frontend Hooks** -- Create hooks/use-keys.ts
10. **Frontend Pages** -- Create keys.tsx + key-detail.tsx
11. **Frontend Router** -- Add routes + sidebar nav
12. **Tests** -- Unit tests for models, services; integration tests for handlers
