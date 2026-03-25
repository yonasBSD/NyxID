> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# Implementation Spec V7: Final Fixes

Focused fixes for keyring spam, migration gaps, SSH creation, and node CLI interactivity.

---

## Issue 1: Keyring Spam Every 60 Seconds

**Root cause:** `oauth_refresh_loop` (node-agent/src/main.rs:990) calls `SecretBackend::from_config()` on every iteration (every 60s). For keychain backend, this calls `KeychainVault::load()` which reads from the OS keychain, producing DEBUG-level logs and potentially triggering macOS keychain access prompts.

**Why we can't just pass the backend in:** `KeychainVault` uses `RefCell<VaultData>`, making `SecretBackend` `!Send`. Tokio's `spawn` requires `Send` futures, so we can't hold `SecretBackend` across `.await` points in spawned tasks.

### Fix: Replace `RefCell` with `Mutex` in `KeychainVault`

**File: `node-agent/src/keychain.rs`**

1. Change `KeychainVault.vault` from `RefCell<VaultData>` to `std::sync::Mutex<VaultData>`
2. Update all `.borrow()` calls to `.lock().unwrap()`
3. Update all `.borrow_mut()` calls to `.lock().unwrap()`

```rust
// BEFORE
pub struct KeychainVault {
    backend: KeychainBackend,
    vault: RefCell<VaultData>,
}

// AFTER
pub struct KeychainVault {
    backend: KeychainBackend,
    vault: std::sync::Mutex<VaultData>,
}
```

Remove `use std::cell::RefCell;` import, add `use std::sync::Mutex;`.

Every method that calls `.borrow()` or `.borrow_mut()` changes to `.lock().unwrap()`:
- `set_auth_token`: `self.vault.borrow_mut()` -> `self.vault.lock().unwrap()`
- `get_auth_token`: `self.vault.borrow()` -> `self.vault.lock().unwrap()`
- `set_signing_secret`, `get_signing_secret`: same pattern
- `set_credential`, `get_credential`, `delete_credential`: same pattern
- `delete_auth_token`, `delete_signing_secret`: same pattern
- `flush`: `self.vault.borrow()` -> `self.vault.lock().unwrap()`
- Constructor methods: `RefCell::new(...)` -> `Mutex::new(...)`

**File: `node-agent/src/main.rs`**

4. Change `oauth_refresh_loop` signature to accept `backend: SecretBackend`:

```rust
async fn oauth_refresh_loop(
    config_file: std::path::PathBuf,
    config_dir: std::path::PathBuf,  // can remove this param now
    interval: Duration,
    backend: SecretBackend,           // NEW
) {
    loop {
        tokio::time::sleep(interval).await;
        let config = match NodeConfig::load(&config_file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // REMOVED: SecretBackend::from_config() call
        // Use `backend` directly for all credential operations
        // ... rest unchanged ...
    }
}
```

5. Update `cmd_start` to pass the backend into the loop:

```rust
// In cmd_start(), line ~158:
let refresh_handle = tokio::spawn(oauth_refresh_loop(
    config_file.clone(),
    config_dir.clone(),  // can remove
    Duration::from_secs(60),
    backend,             // pass the backend created at line 132
));
```

Wait -- `backend` is also passed to `ws_client::run_with_shutdown`. Since `SecretBackend` is now `Send`, we can clone it or restructure ownership. But `SecretBackend` doesn't implement `Clone`.

Better approach: wrap the backend in `Arc`:

```rust
// In cmd_start():
let backend = SecretBackend::from_config(&config, &config_dir)?;
let backend = std::sync::Arc::new(backend);

// Pass Arc<SecretBackend> to both loops and ws_client
let refresh_handle = tokio::spawn(oauth_refresh_loop(
    config_file.clone(),
    Duration::from_secs(60),
    Arc::clone(&backend),
));

ws_client::run_with_shutdown(
    config,
    config_file,
    auth_token,
    signing_secret,
    shared_creds,
    cred_sender,
    backend,  // Arc<SecretBackend>
).await;
```

This requires updating:
- `oauth_refresh_loop` to take `Arc<SecretBackend>`
- `credential_reload_loop` to take `Arc<SecretBackend>` (also benefits from not recreating)
- `ws_client::run_with_shutdown` to take `Arc<SecretBackend>`
- All `SecretBackend` method calls use `&*backend` (auto-deref through Arc)

6. Similarly update `credential_reload_loop` to accept `Arc<SecretBackend>` and stop recreating it:

```rust
async fn credential_reload_loop(
    config_file: std::path::PathBuf,
    sender: std::sync::Arc<SharedCredentialsSender>,
    interval: Duration,
    backend: std::sync::Arc<SecretBackend>,  // NEW
) {
    // ... mtime check logic unchanged ...
    // REMOVED: SecretBackend::from_config() call
    // Use &*backend directly
}
```

### Summary of changes

| File | Change |
|------|--------|
| `node-agent/src/keychain.rs` | `RefCell<VaultData>` -> `Mutex<VaultData>`, update all borrow/borrow_mut calls |
| `node-agent/src/main.rs` | Wrap `SecretBackend` in `Arc`, pass to both loops, remove per-iteration recreation |
| `node-agent/src/ws_client.rs` | Update `run_with_shutdown` signature to `Arc<SecretBackend>` if it uses the backend |

**Test:** Run `cargo test -p nyxid-node`. Existing keychain vault tests should pass with Mutex. Start the node agent with `--log-level debug` and verify no keychain log lines appear every 60s.

---

## Issue 2: Migration of Existing Connected Services

### Bug 2a: `service_type` hardcoded to `"http"` in migrations

**File: `backend/src/db.rs`**

Both `migrate_provider_tokens` (line 973) and `migrate_service_connections` (line 1132) hardcode `service_type: "http".to_string()`. SSH services migrated through either path get the wrong type.

**Fix in `migrate_service_connections` (line ~1121):**
```rust
// BEFORE (line 1132):
service_type: "http".to_string(),

// AFTER:
service_type: service.service_type.clone(),
```

**Fix in `migrate_provider_tokens` (line ~962):**
```rust
// BEFORE (line 973):
service_type: "http".to_string(),

// AFTER:
service_type: if let Some(ref svc) = service {
    svc.service_type.clone()
} else {
    "http".to_string()
},
```

### Bug 2b: Node-bound SSH services with no UserServiceConnection are not migrated

SSH services configured purely through `NodeServiceBinding` (no `UserProviderToken` or `UserServiceConnection`) are missed by all three migration steps:
- `migrate_provider_tokens`: no provider token for SSH
- `migrate_service_connections`: no connection record for node-only SSH
- `migrate_node_service_bindings`: only updates existing `UserService` records, doesn't create new ones

**Fix: Expand `migrate_node_service_bindings` to create missing records**

In `migrate_node_service_bindings`, after the existing `update_one` logic, add a fallback that creates the full record set if no `UserService` matched:

```rust
async fn migrate_node_service_bindings(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let bindings: Vec<NodeServiceBinding> = db
        .collection::<NodeServiceBinding>(NODE_SERVICE_BINDINGS)
        .find(doc! { "is_active": true })
        .await?
        .try_collect()
        .await?;

    let mut migrated = 0u64;
    let mut created = 0u64;
    for binding in &bindings {
        // Try to update existing UserService (created by provider_token or connection migration)
        let result = db
            .collection::<Document>(USER_SERVICES)
            .update_one(
                doc! {
                    "user_id": &binding.user_id,
                    "catalog_service_id": &binding.service_id,
                    "is_active": true,
                },
                doc! {
                    "$set": {
                        "node_id": &binding.node_id,
                        "node_priority": binding.priority,
                        "updated_at": bson::DateTime::from_chrono(Utc::now()),
                    }
                },
            )
            .await?;

        if result.modified_count > 0 {
            migrated += 1;
            continue;
        }

        // No existing UserService found -- check if we need to create one
        // (covers SSH services and other node-only bindings)
        let already_exists = db
            .collection::<UserService>(USER_SERVICES)
            .find_one(doc! {
                "user_id": &binding.user_id,
                "catalog_service_id": &binding.service_id,
                "is_active": true,
            })
            .await?;
        if already_exists.is_some() {
            // Already exists but node_id was already set -- skip
            continue;
        }

        // Check idempotency by source
        let migrated_before = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .find_one(doc! {
                "source": "migration_node_binding",
                "source_id": &binding.id,
            })
            .await?;
        if migrated_before.is_some() {
            continue;
        }

        // Load DownstreamService
        let service = match db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "_id": &binding.service_id })
            .await?
        {
            Some(s) => s,
            None => continue,
        };

        let now = Utc::now();
        let endpoint_id = uuid::Uuid::new_v4().to_string();
        let api_key_id = uuid::Uuid::new_v4().to_string();
        let service_id = uuid::Uuid::new_v4().to_string();

        let is_ssh = service.service_type == "ssh";
        let ep_url = if is_ssh {
            service.ssh_config.as_ref()
                .map(|c| format!("ssh://{}:{}", c.host, c.port))
                .unwrap_or_default()
        } else {
            service.base_url.clone()
        };

        let credential_type = if is_ssh {
            "ssh_certificate".to_string()
        } else {
            "node_managed".to_string()
        };

        // Create UserEndpoint
        let endpoint = UserEndpoint {
            id: endpoint_id.clone(),
            user_id: binding.user_id.clone(),
            label: service.name.clone(),
            url: ep_url,
            catalog_service_id: Some(service.id.clone()),
            created_at: now,
            updated_at: now,
        };
        db.collection::<UserEndpoint>(USER_ENDPOINTS)
            .insert_one(&endpoint)
            .await?;

        // Create UserApiKey (placeholder -- node-managed or SSH certificate)
        let api_key = UserApiKey {
            id: api_key_id.clone(),
            user_id: binding.user_id.clone(),
            label: service.name.clone(),
            credential_type,
            credential_encrypted: None,
            access_token_encrypted: None,
            refresh_token_encrypted: None,
            token_scopes: None,
            expires_at: None,
            provider_config_id: None,
            user_oauth_client_id_encrypted: None,
            user_oauth_client_secret_encrypted: None,
            status: "active".to_string(),
            last_used_at: None,
            error_message: None,
            source: Some("migration_node_binding".to_string()),
            source_id: Some(binding.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .insert_one(&api_key)
            .await
        {
            let _ = db.collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id }).await;
            return Err(e.into());
        }

        // Create UserService with node routing
        let user_service = UserService {
            id: service_id,
            user_id: binding.user_id.clone(),
            slug: service.slug.clone(),
            endpoint_id: endpoint_id.clone(),
            api_key_id: api_key_id.clone(),
            auth_method: service.auth_method.clone(),
            auth_key_name: service.auth_key_name.clone(),
            catalog_service_id: Some(service.id.clone()),
            node_id: Some(binding.node_id.clone()),
            node_priority: binding.priority,
            service_type: service.service_type.clone(),
            is_active: true,
            source: Some("migration_node_binding".to_string()),
            source_id: Some(binding.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserService>(USER_SERVICES)
            .insert_one(&user_service)
            .await
        {
            let _ = db.collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id }).await;
            let _ = db.collection::<UserApiKey>(USER_API_KEYS)
                .delete_one(doc! { "_id": &api_key_id }).await;
            return Err(e.into());
        }

        created += 1;
    }

    if migrated > 0 || created > 0 {
        tracing::info!(
            updated = migrated,
            created = created,
            "Migrated node service bindings to unified collections"
        );
    }
    Ok(())
}
```

### Summary of changes

| File | Change |
|------|--------|
| `backend/src/db.rs` line ~973 | `service_type` from catalog in `migrate_provider_tokens` |
| `backend/src/db.rs` line ~1132 | `service_type` from catalog in `migrate_service_connections` |
| `backend/src/db.rs` `migrate_node_service_bindings` | Create full record set (endpoint + api_key + service) for unmatched bindings |

**Test:** Create a `NodeServiceBinding` for an SSH `DownstreamService` without any `UserServiceConnection`. Run `migrate_to_unified_collections`. Verify `UserService` with `service_type: "ssh"` and `node_id` set appears in the `user_services` collection.

---

## Issue 3: SSH Service Creation Flow

### Status: Already working

The SSH creation flow works end-to-end:

1. **Catalog shows SSH services** -- `CatalogGrid` renders SSH services with an "SSH" badge (add-key-dialog.tsx:165-169).
2. **Routing forces "Via Node"** -- `RoutingStep` detects `isSshOnly` and locks routing to node (add-key-dialog.tsx:211-213, 285-289).
3. **Node setup shows SSH instructions** -- `NodeSetupStep` has SSH-specific instructions: allow target, trust CA, sshd_config (add-key-dialog.tsx:621-648).
4. **Backend handles SSH correctly** -- `unified_key_service::create_key` detects SSH, enforces `node_id`, derives `ssh://host:port` endpoint, sets `credential_type: "ssh_certificate"` (unified_key_service.rs:106-123).
5. **SSH config shown in response** -- `build_key_view` populates SSH fields from catalog's `SshServiceConfig` (unified_key_service.rs:390-409).

### Optional improvement: `nyxid-node ssh allow` CLI command

Not blocking, but a convenience command to add SSH targets to the node config:

```
nyxid-node ssh allow --host <host> --port <port>
```

This would append to `config.toml`'s `[ssh] allowed_targets` array. The node config already has `SshTargetConfig` (config.rs:73-78) and `SshConfig.allowed_targets` (config.rs:60).

**Deferred to a future spec** -- the manual config edit is documented in the frontend instructions and is sufficient.

### One small fix: NodeSetupStep missing Label input for catalog entries

The `NodeSetupStep` only shows a Label input for custom endpoints (`isCustom`). For catalog SSH services, the label defaults to `entry.name` but the user cannot customize it.

**File: `frontend/src/components/dashboard/add-key-dialog.tsx`**

Add a Label input at the top of `NodeSetupStep`, before the SSH instructions block:

```tsx
// Inside NodeSetupStep, after the Back button and before the isCustom block:
<div className="space-y-1.5">
  <Label htmlFor="node-label">Label</Label>
  <Input
    id="node-label"
    placeholder={catalogEntry?.name ?? "My Service"}
    value={form.label}
    onChange={(e) => onChange({ label: e.target.value })}
  />
</div>
```

This applies to all node-setup services (SSH and non-SSH catalog entries), giving the user a chance to customize the display name.

---

## Issue 4: Custom Endpoint Via Node -- CLI Command Accuracy

### Status: Already working

The `NodeSetupStep` for custom endpoints already shows:
- Slug input (required)
- Endpoint URL input
- Auth Method + Auth Key Name dropdowns
- Generated CLI command via `buildCredentialCommand()` that includes all fields

The generated command correctly maps auth methods:
- `bearer` -> `--header Authorization --secret-format bearer`
- `header` -> `--header <key_name>`
- `query` -> `--query-param <key_name>`
- `basic` -> `--header <key_name> --secret-format basic`
- `none` -> `--header <key_name>` (fallback; credential is optional for node-managed)

The `handleNodeSetupSubmit` for custom endpoints sends all fields: `label`, `endpoint_url`, `slug`, `auth_method`, `auth_key_name`, `node_id`.

### No changes needed for this issue.

---

## Issue 5: Node CLI Interactive Mode for `credentials add`

**Current behavior:** `credentials add` requires `--header <name>` or `--query-param <name>` as flags. If neither is provided, it fails with a validation error.

**Desired behavior:** If flags are omitted, prompt interactively.

### Changes

**File: `node-agent/src/cli.rs`**

No changes to CLI struct. All existing flags remain optional. Interactive mode activates when flags are missing.

**File: `node-agent/src/main.rs`**

Replace the else-branch in `cmd_credentials` -> `CredentialCommands::Add` (line ~467-471) with interactive prompts:

```rust
CredentialCommands::Add {
    service,
    url,
    header,
    query_param,
    secret_format,
    value,
} => {
    let mut config = NodeConfig::load(&config_file)?;
    let backend = SecretBackend::from_config(&config, &config_dir)?;

    // Determine injection method and key name
    let (injection_method, key_name) = if let Some(h) = header {
        ("header", h)
    } else if let Some(q) = query_param {
        ("query_param", q)
    } else {
        // Interactive: prompt for injection method
        let method = prompt_choice(
            "Auth method",
            &["header", "query_param"],
            "header",
        )?;
        let default_name = if method == "header" { "Authorization" } else { "api_key" };
        let name = prompt_string(
            &format!("{} name", if method == "header" { "Header" } else { "Query param" }),
            default_name,
        )?;
        (method.as_str().to_string(), name)
        // Actually, let's keep the variable types consistent.
    };

    // Determine endpoint URL
    let effective_url = if url.is_some() {
        url
    } else {
        // Interactive: prompt for URL (optional)
        let input = prompt_string_optional("Endpoint URL (optional, press Enter to skip)")?;
        if input.is_empty() { None } else { Some(input) }
    };

    // ... rest of credential storage logic (existing code) ...
}
```

Add two helper functions to `main.rs`:

```rust
/// Prompt for a string value with a default.
fn prompt_string(label: &str, default: &str) -> Result<String> {
    use std::io::Write;
    print!("{label} [{default}]: ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    Ok(if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() })
}

/// Prompt for an optional string value (empty = None).
fn prompt_string_optional(label: &str) -> Result<String> {
    use std::io::Write;
    print!("{label}: ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Prompt to choose from a set of options.
fn prompt_choice(label: &str, options: &[&str], default: &str) -> Result<String> {
    use std::io::Write;
    let options_str = options.join("/");
    print!("{label} ({options_str}) [{default}]: ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default.to_string());
    }
    if options.contains(&trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(crate::error::Error::Validation(
            format!("Invalid choice '{}', expected one of: {}", trimmed, options_str),
        ))
    }
}
```

### Full interactive flow when no flags provided

```
$ nyxid-node credentials add --service my-api

Auth method (header/query_param) [header]: header
Header name [Authorization]: X-Api-Key
Secret format (raw/bearer/basic) [raw]: bearer
Endpoint URL (optional, press Enter to skip): https://api.example.com/v1
Enter value for header 'X-Api-Key': ****

Credential added for service 'my-api'.
```

### Detailed change to `CredentialCommands::Add` handler

The existing code at lines 395-475 handles three cases:
1. `--header` provided (with or without `:`-separated value)
2. `--query-param` provided (with or without `=`-separated value)
3. Neither provided -> error

Change case 3 to interactive mode:

```rust
} else {
    // Interactive mode: prompt for all values
    let method = prompt_choice("Auth method", &["header", "query_param"], "header")?;

    if method == "header" {
        let header_name = prompt_string("Header name", "Authorization")?;

        // Prompt for secret format
        let fmt_str = prompt_choice("Secret format", &["raw", "bearer", "basic"], "raw")?;
        let fmt = match fmt_str.as_str() {
            "bearer" => CredentialSecretFormat::Bearer,
            "basic" => CredentialSecretFormat::Basic,
            _ => CredentialSecretFormat::Raw,
        };

        let secret = read_secret_value(
            value,
            &format!("Enter value for header '{header_name}'"),
        )?;
        let secret = format_secret_value(secret, fmt)?;

        // Prompt for URL if not provided
        let effective_url = match url {
            Some(u) => Some(u),
            None => {
                let input = prompt_string_optional("Endpoint URL (optional, press Enter to skip)")?;
                if input.is_empty() { None } else { Some(input) }
            }
        };

        config.add_header_credential_via(
            &service, &header_name, &secret, effective_url.as_deref(), &backend,
        )?;
    } else {
        let param_name = prompt_string("Query param name", "api_key")?;

        let secret = read_secret_value(
            value,
            &format!("Enter value for query param '{param_name}'"),
        )?;

        let effective_url = match url {
            Some(u) => Some(u),
            None => {
                let input = prompt_string_optional("Endpoint URL (optional, press Enter to skip)")?;
                if input.is_empty() { None } else { Some(input) }
            }
        };

        config.add_query_param_credential_via(
            &service, &param_name, &secret, effective_url.as_deref(), &backend,
        )?;
    }
}
```

### Summary of changes

| File | Change |
|------|--------|
| `node-agent/src/main.rs` | Add `prompt_string`, `prompt_string_optional`, `prompt_choice` helpers |
| `node-agent/src/main.rs` | Replace error branch in `CredentialCommands::Add` with interactive prompts |

**Test:** Run `cargo build -p nyxid-node`. Test interactively: `nyxid-node credentials add --service test-svc` should prompt for auth method, key name, secret format, URL, and secret value.

---

## Change Summary

| # | Issue | Files | Priority |
|---|-------|-------|----------|
| 1 | Keyring spam | `keychain.rs`, `main.rs`, `ws_client.rs` | High -- user-visible annoyance |
| 2a | `service_type` hardcoded in migration | `db.rs` (2 lines) | High -- data correctness |
| 2b | Node-only bindings not migrated | `db.rs` (`migrate_node_service_bindings`) | High -- SSH services invisible |
| 3 | SSH creation flow | Already working; optional label input in `add-key-dialog.tsx` | Low |
| 4 | Custom via node CLI command | Already working | None |
| 5 | Interactive `credentials add` | `main.rs` (add prompts + helpers) | Medium -- UX improvement |

## Implementation Order

1. **Issue 2a** (2 lines, highest risk) -- fix `service_type` in migrations
2. **Issue 2b** (expand `migrate_node_service_bindings`) -- create missing records
3. **Issue 1** (`RefCell` -> `Mutex`, `Arc<SecretBackend>`) -- keyring spam
4. **Issue 5** (interactive prompts) -- CLI UX
5. **Issue 3** (optional label input) -- minor frontend polish
