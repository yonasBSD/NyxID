use std::time::Duration;

use chrono::Utc;
use futures::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Database, IndexModel};

use crate::config::AppConfig;
use crate::models::downstream_service::{
    COLLECTION_NAME as DOWNSTREAM_SERVICES, DownstreamService,
};
use crate::models::node_service_binding::{
    COLLECTION_NAME as NODE_SERVICE_BINDINGS, NodeServiceBinding,
};
use crate::models::provider_config::{COLLECTION_NAME as PROVIDER_CONFIGS, ProviderConfig};
use crate::models::user_api_key::{COLLECTION_NAME as USER_API_KEYS, UserApiKey};
use crate::models::user_endpoint::{COLLECTION_NAME as USER_ENDPOINTS, UserEndpoint};
use crate::models::user_provider_credentials::{
    COLLECTION_NAME as USER_PROVIDER_CREDENTIALS, UserProviderCredentials,
};
use crate::models::user_provider_token::{
    COLLECTION_NAME as USER_PROVIDER_TOKENS, UserProviderToken,
};
use crate::models::user_service::{COLLECTION_NAME as USER_SERVICES, UserService};
use crate::models::user_service_connection::{
    COLLECTION_NAME as USER_SERVICE_CONNECTIONS, UserServiceConnection,
};

/// Type alias for the MongoDB database handle used throughout the application.
pub type DbHandle = Database;

/// Create a configured MongoDB connection and return the database handle.
///
/// Parses the connection string, configures the connection pool, verifies
/// connectivity with a ping, and ensures all required indexes exist.
pub async fn create_connection(config: &AppConfig) -> Result<DbHandle, mongodb::error::Error> {
    let mut client_options = ClientOptions::parse(&config.database_url).await?;

    client_options.max_pool_size = Some(config.database_max_connections);
    client_options.min_pool_size = Some(2);
    client_options.connect_timeout = Some(Duration::from_secs(10));
    client_options.server_selection_timeout = Some(Duration::from_secs(10));
    client_options.max_idle_time = Some(Duration::from_secs(600));

    let client = Client::with_options(client_options)?;

    // Extract database name from the connection string, default to "nyxid"
    let db_name = client
        .default_database()
        .map(|db| db.name().to_string())
        .unwrap_or_else(|| "nyxid".to_string());

    let db = client.database(&db_name);

    // Verify connectivity
    db.run_command(doc! { "ping": 1 }).await?;
    tracing::info!("MongoDB connection established");

    ensure_indexes(&db).await?;
    tracing::info!("MongoDB indexes verified");

    backfill_downstream_service_types(&db).await?;

    Ok(db)
}

/// Create all required indexes for every collection.
///
/// Uses `create_index` which is idempotent -- if the index already exists
/// with the same specification it is a no-op.
pub async fn ensure_indexes(db: &Database) -> Result<(), mongodb::error::Error> {
    // ── users ──
    let users = db.collection::<mongodb::bson::Document>("users");
    users
        .create_index(
            IndexModel::builder()
                .keys(doc! { "email": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    users
        .create_index(
            IndexModel::builder()
                .keys(doc! { "email_verification_token": 1 })
                .build(),
        )
        .await?;
    users
        .create_index(
            IndexModel::builder()
                .keys(doc! { "password_reset_token": 1 })
                .build(),
        )
        .await?;
    // Social login lookup: find user by (provider, provider_id)
    // Drop old sparse index if it exists (sparse doesn't work with null values from serde)
    let _ = users
        .drop_index("social_provider_1_social_provider_id_1")
        .await;
    users
        .create_index(
            IndexModel::builder()
                .keys(doc! { "social_provider": 1, "social_provider_id": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .partial_filter_expression(doc! {
                            "social_provider": { "$type": "string" },
                            "social_provider_id": { "$type": "string" },
                        })
                        .build(),
                )
                .build(),
        )
        .await?;

    // ── sessions ──
    let sessions = db.collection::<mongodb::bson::Document>("sessions");
    sessions
        .create_index(IndexModel::builder().keys(doc! { "token_hash": 1 }).build())
        .await?;
    sessions
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;
    sessions
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;

    // ── authorization_codes ──
    let auth_codes = db.collection::<mongodb::bson::Document>("authorization_codes");
    auth_codes
        .create_index(IndexModel::builder().keys(doc! { "code_hash": 1 }).build())
        .await?;
    auth_codes
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;

    // ── refresh_tokens ──
    let refresh_tokens = db.collection::<mongodb::bson::Document>("refresh_tokens");
    refresh_tokens
        .create_index(
            IndexModel::builder()
                .keys(doc! { "jti": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    refresh_tokens
        .create_index(IndexModel::builder().keys(doc! { "session_id": 1 }).build())
        .await?;
    refresh_tokens
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;

    // ── api_keys ──
    let api_keys = db.collection::<mongodb::bson::Document>("api_keys");
    api_keys
        .create_index(IndexModel::builder().keys(doc! { "key_hash": 1 }).build())
        .await?;
    api_keys
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── mfa_factors ──
    let mfa = db.collection::<mongodb::bson::Document>("mfa_factors");
    mfa.create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── downstream_services ──
    let services = db.collection::<mongodb::bson::Document>("downstream_services");
    services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "slug": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "service_category": 1, "is_active": 1 })
                .build(),
        )
        .await?;
    services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "service_type": 1, "service_category": 1, "is_active": 1 })
                .build(),
        )
        .await?;

    services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "provider_config_id": 1 })
                .options(IndexOptions::builder().sparse(true).unique(true).build())
                .build(),
        )
        .await?;

    // ── user_service_connections ──
    let usc = db.collection::<mongodb::bson::Document>("user_service_connections");
    usc.create_index(
        IndexModel::builder()
            .keys(doc! { "user_id": 1, "service_id": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    )
    .await?;

    // ── audit_log ──
    let audit = db.collection::<mongodb::bson::Document>("audit_log");
    audit
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "created_at": -1 })
                .build(),
        )
        .await?;
    audit
        .create_index(
            IndexModel::builder()
                .keys(doc! { "event_type": 1, "created_at": -1 })
                .build(),
        )
        .await?;

    // ── oauth_clients ── (no special indexes beyond _id)

    // ── service_endpoints ──
    let endpoints = db.collection::<mongodb::bson::Document>("service_endpoints");
    endpoints
        .create_index(
            IndexModel::builder()
                .keys(doc! { "service_id": 1, "is_active": 1 })
                .build(),
        )
        .await?;
    endpoints
        .create_index(
            IndexModel::builder()
                .keys(doc! { "service_id": 1, "name": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;

    // ── provider_configs ──
    let provider_configs = db.collection::<mongodb::bson::Document>("provider_configs");
    provider_configs
        .create_index(
            IndexModel::builder()
                .keys(doc! { "slug": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    provider_configs
        .create_index(
            IndexModel::builder()
                .keys(doc! { "provider_type": 1, "is_active": 1 })
                .build(),
        )
        .await?;

    // ── user_provider_tokens ──
    let user_tokens = db.collection::<mongodb::bson::Document>("user_provider_tokens");
    user_tokens
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "provider_config_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    user_tokens
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "status": 1 })
                .build(),
        )
        .await?;
    user_tokens
        .create_index(
            IndexModel::builder()
                .keys(doc! { "status": 1, "expires_at": 1 })
                .build(),
        )
        .await?;

    // ── service_provider_requirements ──
    let spr = db.collection::<mongodb::bson::Document>("service_provider_requirements");
    spr.create_index(
        IndexModel::builder()
            .keys(doc! { "service_id": 1, "provider_config_id": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    )
    .await?;
    spr.create_index(IndexModel::builder().keys(doc! { "service_id": 1 }).build())
        .await?;
    spr.create_index(
        IndexModel::builder()
            .keys(doc! { "provider_config_id": 1 })
            .build(),
    )
    .await?;

    // ── oauth_states ──
    let oauth_states = db.collection::<mongodb::bson::Document>("oauth_states");
    oauth_states
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;
    oauth_states
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── roles ──
    let roles = db.collection::<mongodb::bson::Document>("roles");
    roles
        .create_index(
            IndexModel::builder()
                .keys(doc! { "slug": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    roles
        .create_index(
            IndexModel::builder()
                .keys(doc! { "client_id": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;

    // ── groups ──
    let groups = db.collection::<mongodb::bson::Document>("groups");
    groups
        .create_index(
            IndexModel::builder()
                .keys(doc! { "slug": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    groups
        .create_index(
            IndexModel::builder()
                .keys(doc! { "parent_group_id": 1 })
                .build(),
        )
        .await?;

    // ── consents ──
    let consents = db.collection::<mongodb::bson::Document>("consents");
    consents
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "client_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    consents
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── service_accounts ──
    let sa = db.collection::<mongodb::bson::Document>("service_accounts");
    sa.create_index(
        IndexModel::builder()
            .keys(doc! { "client_id": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    )
    .await?;
    sa.create_index(IndexModel::builder().keys(doc! { "is_active": 1 }).build())
        .await?;
    sa.create_index(IndexModel::builder().keys(doc! { "created_by": 1 }).build())
        .await?;

    // ── service_account_tokens ──
    let sat = db.collection::<mongodb::bson::Document>("service_account_tokens");
    sat.create_index(
        IndexModel::builder()
            .keys(doc! { "jti": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    )
    .await?;
    sat.create_index(
        IndexModel::builder()
            .keys(doc! { "service_account_id": 1 })
            .build(),
    )
    .await?;
    sat.create_index(
        IndexModel::builder()
            .keys(doc! { "expires_at": 1 })
            .options(
                IndexOptions::builder()
                    .expire_after(Duration::from_secs(0))
                    .build(),
            )
            .build(),
    )
    .await?;

    // ── mcp_sessions ──
    let mcp_sessions = db.collection::<mongodb::bson::Document>("mcp_sessions");
    mcp_sessions
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;
    mcp_sessions
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── approval_requests ──
    let approval_requests = db.collection::<mongodb::bson::Document>("approval_requests");
    approval_requests
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "status": 1 })
                .build(),
        )
        .await?;
    // Migration: drop the legacy auto-named index on idempotency_key if it exists.
    // The current index uses a stable explicit name to avoid accidental drop/recreate
    // loops on startup.
    let _ = approval_requests.drop_index("idempotency_key_1").await;
    approval_requests
        .create_index(
            IndexModel::builder()
                .keys(doc! { "idempotency_key": 1 })
                .options(
                    IndexOptions::builder()
                        .name("idempotency_key_pending_unique".to_string())
                        .unique(true)
                        .partial_filter_expression(doc! { "status": "pending" })
                        .build(),
                )
                .build(),
        )
        .await?;
    approval_requests
        .create_index(
            IndexModel::builder()
                .keys(doc! { "created_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(90 * 24 * 60 * 60))
                        .build(),
                )
                .build(),
        )
        .await?;

    // ── approval_grants ──
    let approval_grants = db.collection::<mongodb::bson::Document>("approval_grants");
    approval_grants
        .create_index(
            IndexModel::builder()
                .keys(doc! {
                    "user_id": 1,
                    "service_id": 1,
                    "requester_type": 1,
                    "requester_id": 1,
                })
                .build(),
        )
        .await?;
    approval_grants
        .create_index(
            IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(Duration::from_secs(0))
                        .build(),
                )
                .build(),
        )
        .await?;
    approval_grants
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "granted_at": -1 })
                .build(),
        )
        .await?;

    // ── service_approval_configs ──
    let sac = db.collection::<mongodb::bson::Document>("service_approval_configs");
    sac.create_index(
        IndexModel::builder()
            .keys(doc! { "user_id": 1, "service_id": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    )
    .await?;
    sac.create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // ── user_provider_credentials ──
    let user_creds = db.collection::<mongodb::bson::Document>("user_provider_credentials");
    user_creds
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "provider_config_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;

    // ── notification_channels ──
    let notification_channels = db.collection::<mongodb::bson::Document>("notification_channels");
    notification_channels
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;
    notification_channels
        .create_index(
            IndexModel::builder()
                .keys(doc! { "telegram_link_code": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;
    notification_channels
        .create_index(
            IndexModel::builder()
                .keys(doc! { "telegram_chat_id": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;
    // Supports token-based cleanup paths (e.g. account switching and logout detach).
    notification_channels
        .create_index(
            IndexModel::builder()
                .keys(doc! { "push_devices.token": 1 })
                .options(IndexOptions::builder().sparse(true).build())
                .build(),
        )
        .await?;

    // ── nodes ──
    let nodes = db.collection::<mongodb::bson::Document>("nodes");
    // Drop legacy index without partial filter (if it exists) to replace with soft-delete-safe version
    let _ = nodes.drop_index("user_id_1_name_1").await;
    nodes
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "name": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .partial_filter_expression(doc! { "is_active": true })
                        .build(),
                )
                .build(),
        )
        .await?;
    nodes
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "is_active": 1 })
                .build(),
        )
        .await?;
    nodes
        .create_index(
            IndexModel::builder()
                .keys(doc! { "auth_token_hash": 1 })
                .build(),
        )
        .await?;

    // ── node_service_bindings ──
    let nsb = db.collection::<mongodb::bson::Document>("node_service_bindings");
    // Drop legacy index without partial filter (if it exists) to replace with soft-delete-safe version
    let _ = nsb.drop_index("node_id_1_service_id_1").await;
    nsb.create_index(
        IndexModel::builder()
            .keys(doc! { "node_id": 1, "service_id": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .partial_filter_expression(doc! { "is_active": true })
                    .build(),
            )
            .build(),
    )
    .await?;
    nsb.create_index(
        IndexModel::builder()
            .keys(doc! { "user_id": 1, "service_id": 1, "is_active": 1 })
            .build(),
    )
    .await?;
    nsb.create_index(
        IndexModel::builder()
            .keys(doc! { "node_id": 1, "is_active": 1 })
            .build(),
    )
    .await?;

    // ── node_registration_tokens ──
    let nrt = db.collection::<mongodb::bson::Document>("node_registration_tokens");
    nrt.create_index(IndexModel::builder().keys(doc! { "token_hash": 1 }).build())
        .await?;
    nrt.create_index(
        IndexModel::builder()
            .keys(doc! { "expires_at": 1 })
            .options(
                IndexOptions::builder()
                    .expire_after(Duration::from_secs(0))
                    .build(),
            )
            .build(),
    )
    .await?;

    // Drop old sparse unique indexes that conflict with partial filter indexes
    // (MongoDB won't replace an index with different options on the same keys)
    let _ = db
        .collection::<mongodb::bson::Document>("user_api_keys")
        .drop_index("source_1_source_id_1")
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("user_services")
        .drop_index("source_1_source_id_1")
        .await;

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
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
        .await?;

    // -- user_api_keys --
    let user_api_keys = db.collection::<mongodb::bson::Document>("user_api_keys");
    user_api_keys
        .create_index(IndexModel::builder().keys(doc! { "user_id": 1 }).build())
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
                        .unique(true)
                        .partial_filter_expression(doc! {
                            "source_id": { "$type": "string" }
                        })
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
        .create_index(IndexModel::builder().keys(doc! { "api_key_id": 1 }).build())
        .await?;
    user_services
        .create_index(
            IndexModel::builder()
                .keys(doc! { "source": 1, "source_id": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .partial_filter_expression(doc! {
                            "source_id": { "$type": "string" }
                        })
                        .build(),
                )
                .build(),
        )
        .await?;

    backfill_downstream_service_types(db).await?;

    Ok(())
}

async fn backfill_downstream_service_types(db: &Database) -> Result<(), mongodb::error::Error> {
    let services = db.collection::<Document>("downstream_services");
    let migration = services
        .update_many(
            doc! { "service_type": { "$exists": false } },
            doc! { "$set": { "service_type": "http" } },
        )
        .await?;

    if migration.modified_count > 0 {
        tracing::info!(
            count = migration.modified_count,
            "Backfilled missing downstream service_type to http"
        );
    }

    Ok(())
}

/// Migrate existing user data to the new unified collections.
/// Idempotent: uses source + source_id to skip already-migrated records.
pub async fn migrate_to_unified_collections(
    db: &Database,
) -> Result<(), Box<dyn std::error::Error>> {
    migrate_provider_tokens(db).await?;
    migrate_service_connections(db).await?;
    migrate_node_service_bindings(db).await?;
    tracing::info!("Unified collection migration complete");
    Ok(())
}

/// Migrate UserProviderTokens to the unified UserEndpoint + UserApiKey + UserService model.
async fn migrate_provider_tokens(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let tokens: Vec<UserProviderToken> = db
        .collection::<UserProviderToken>(USER_PROVIDER_TOKENS)
        .find(doc! {})
        .await?
        .try_collect()
        .await?;

    let mut migrated = 0u64;
    for token in &tokens {
        // Check idempotency: skip if already migrated
        let existing = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .find_one(doc! {
                "source": "migration_provider_token",
                "source_id": &token.id,
            })
            .await?;
        if existing.is_some() {
            continue;
        }

        // Find the DownstreamService linked to this provider
        let service = db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "provider_config_id": &token.provider_config_id, "is_active": true })
            .await?;

        // Load ProviderConfig for name
        let provider = db
            .collection::<ProviderConfig>(PROVIDER_CONFIGS)
            .find_one(doc! { "_id": &token.provider_config_id })
            .await?;

        let provider_name = provider
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Unknown Provider".to_string());
        let provider_slug = provider
            .as_ref()
            .map(|p| p.slug.clone())
            .unwrap_or_else(|| format!("provider-{}", &token.provider_config_id));

        let now = Utc::now();
        let endpoint_id = uuid::Uuid::new_v4().to_string();
        let api_key_id = uuid::Uuid::new_v4().to_string();
        let service_id = uuid::Uuid::new_v4().to_string();

        // Determine endpoint URL and catalog info
        let (endpoint_url, catalog_service_id, slug, auth_method, auth_key_name) =
            if let Some(ref svc) = service {
                let url = token
                    .gateway_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .unwrap_or(&svc.base_url);
                (
                    url.to_string(),
                    Some(svc.id.clone()),
                    svc.slug.clone(),
                    svc.auth_method.clone(),
                    svc.auth_key_name.clone(),
                )
            } else {
                let url = token
                    .gateway_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .unwrap_or("https://placeholder.invalid");
                (
                    url.to_string(),
                    None,
                    provider_slug,
                    "bearer".to_string(),
                    "Authorization".to_string(),
                )
            };

        // Create UserEndpoint
        let endpoint = UserEndpoint {
            id: endpoint_id.clone(),
            user_id: token.user_id.clone(),
            label: provider_name.clone(),
            url: endpoint_url,
            catalog_service_id: catalog_service_id.clone(),
            created_at: now,
            updated_at: now,
        };
        db.collection::<UserEndpoint>(USER_ENDPOINTS)
            .insert_one(&endpoint)
            .await?;

        // Look up user provider credentials for OAuth app credentials
        let user_creds = db
            .collection::<UserProviderCredentials>(USER_PROVIDER_CREDENTIALS)
            .find_one(doc! {
                "user_id": &token.user_id,
                "provider_config_id": &token.provider_config_id,
            })
            .await?;

        // Create UserApiKey -- clean up endpoint on failure
        let api_key = UserApiKey {
            id: api_key_id.clone(),
            user_id: token.user_id.clone(),
            label: token.label.clone().unwrap_or(provider_name),
            credential_type: token.token_type.clone(),
            credential_encrypted: token.api_key_encrypted.clone(),
            access_token_encrypted: token.access_token_encrypted.clone(),
            refresh_token_encrypted: token.refresh_token_encrypted.clone(),
            token_scopes: token.token_scopes.clone(),
            expires_at: token.expires_at,
            provider_config_id: Some(token.provider_config_id.clone()),
            user_oauth_client_id_encrypted: user_creds
                .as_ref()
                .and_then(|c| c.client_id_encrypted.clone()),
            user_oauth_client_secret_encrypted: user_creds
                .as_ref()
                .and_then(|c| c.client_secret_encrypted.clone()),
            status: token.status.clone(),
            last_used_at: token.last_used_at,
            error_message: token.error_message.clone(),
            source: Some("migration_provider_token".to_string()),
            source_id: Some(token.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .insert_one(&api_key)
            .await
        {
            // Clean up orphaned endpoint
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
            return Err(e.into());
        }

        // Create UserService -- clean up endpoint + api_key on failure
        let user_service = UserService {
            id: service_id,
            user_id: token.user_id.clone(),
            slug,
            endpoint_id: endpoint_id.clone(),
            api_key_id: api_key_id.clone(),
            auth_method,
            auth_key_name,
            catalog_service_id,
            node_id: None,
            node_priority: 0,
            service_type: if let Some(ref svc) = service {
                svc.service_type.clone()
            } else {
                "http".to_string()
            },
            is_active: true,
            source: Some("migration_provider_token".to_string()),
            source_id: Some(token.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserService>(USER_SERVICES)
            .insert_one(&user_service)
            .await
        {
            // Clean up orphaned endpoint and api_key
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
            let _ = db
                .collection::<UserApiKey>(USER_API_KEYS)
                .delete_one(doc! { "_id": &api_key_id })
                .await;
            return Err(e.into());
        }

        migrated += 1;
    }

    if migrated > 0 {
        tracing::info!(
            count = migrated,
            "Migrated provider tokens to unified collections"
        );
    }
    Ok(())
}

/// Migrate UserServiceConnections to the unified model.
async fn migrate_service_connections(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let connections: Vec<UserServiceConnection> = db
        .collection::<UserServiceConnection>(USER_SERVICE_CONNECTIONS)
        .find(doc! { "is_active": true })
        .await?
        .try_collect()
        .await?;

    let mut migrated = 0u64;
    for conn in &connections {
        // Check idempotency
        let existing = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .find_one(doc! {
                "source": "migration_connection",
                "source_id": &conn.id,
            })
            .await?;
        if existing.is_some() {
            continue;
        }

        // Load the downstream service
        let service = match db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "_id": &conn.service_id })
            .await?
        {
            Some(s) => s,
            None => {
                tracing::warn!(
                    conn_id = %conn.id,
                    service_id = %conn.service_id,
                    "Skipping connection migration: downstream service not found"
                );
                continue;
            }
        };

        // Check if already migrated via provider token path
        let already_has_service = db
            .collection::<UserService>(USER_SERVICES)
            .find_one(doc! {
                "user_id": &conn.user_id,
                "catalog_service_id": &service.id,
                "is_active": true,
            })
            .await?;
        if already_has_service.is_some() {
            continue;
        }

        let now = Utc::now();
        let endpoint_id = uuid::Uuid::new_v4().to_string();
        let api_key_id = uuid::Uuid::new_v4().to_string();
        let service_id = uuid::Uuid::new_v4().to_string();

        // Create UserEndpoint
        let endpoint = UserEndpoint {
            id: endpoint_id.clone(),
            user_id: conn.user_id.clone(),
            label: service.name.clone(),
            url: service.base_url.clone(),
            catalog_service_id: Some(service.id.clone()),
            created_at: now,
            updated_at: now,
        };
        db.collection::<UserEndpoint>(USER_ENDPOINTS)
            .insert_one(&endpoint)
            .await?;

        // Create UserApiKey -- clean up endpoint on failure
        let cred_type = conn
            .credential_type
            .clone()
            .or_else(|| service.auth_type.clone())
            .unwrap_or_else(|| "api_key".to_string());
        let api_key = UserApiKey {
            id: api_key_id.clone(),
            user_id: conn.user_id.clone(),
            label: conn
                .credential_label
                .clone()
                .unwrap_or_else(|| service.name.clone()),
            credential_type: cred_type,
            credential_encrypted: conn.credential_encrypted.clone(),
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
            source: Some("migration_connection".to_string()),
            source_id: Some(conn.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserApiKey>(USER_API_KEYS)
            .insert_one(&api_key)
            .await
        {
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
            return Err(e.into());
        }

        // Create UserService -- clean up endpoint + api_key on failure
        let user_service = UserService {
            id: service_id,
            user_id: conn.user_id.clone(),
            slug: service.slug.clone(),
            endpoint_id: endpoint_id.clone(),
            api_key_id: api_key_id.clone(),
            auth_method: service.auth_method.clone(),
            auth_key_name: service.auth_key_name.clone(),
            catalog_service_id: Some(service.id.clone()),
            node_id: None,
            node_priority: 0,
            service_type: service.service_type.clone(),
            is_active: true,
            source: Some("migration_connection".to_string()),
            source_id: Some(conn.id.clone()),
            created_at: now,
            updated_at: now,
        };
        if let Err(e) = db
            .collection::<UserService>(USER_SERVICES)
            .insert_one(&user_service)
            .await
        {
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
            let _ = db
                .collection::<UserApiKey>(USER_API_KEYS)
                .delete_one(doc! { "_id": &api_key_id })
                .await;
            return Err(e.into());
        }

        migrated += 1;
    }

    if migrated > 0 {
        tracing::info!(
            count = migrated,
            "Migrated service connections to unified collections"
        );
    }
    Ok(())
}

/// Migrate NodeServiceBindings: attach node_id + priority to matching UserService records.
/// For bindings with no existing UserService (e.g. SSH-only services configured purely
/// through NodeServiceBinding), create the full record set (endpoint + api_key + service).
async fn migrate_node_service_bindings(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let bindings: Vec<NodeServiceBinding> = db
        .collection::<NodeServiceBinding>(NODE_SERVICE_BINDINGS)
        .find(doc! { "is_active": true })
        .await?
        .try_collect()
        .await?;

    let mut updated = 0u64;
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
            updated += 1;
            continue;
        }

        // No existing UserService was updated -- check if one already exists
        let already_exists = db
            .collection::<UserService>(USER_SERVICES)
            .find_one(doc! {
                "user_id": &binding.user_id,
                "catalog_service_id": &binding.service_id,
                "is_active": true,
            })
            .await?;
        if already_exists.is_some() {
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

        // Load DownstreamService for this binding
        let service = match db
            .collection::<DownstreamService>(DOWNSTREAM_SERVICES)
            .find_one(doc! { "_id": &binding.service_id })
            .await?
        {
            Some(s) => s,
            None => {
                tracing::warn!(
                    binding_id = %binding.id,
                    service_id = %binding.service_id,
                    "Skipping node binding migration: downstream service not found"
                );
                continue;
            }
        };

        let now = Utc::now();
        let endpoint_id = uuid::Uuid::new_v4().to_string();
        let api_key_id = uuid::Uuid::new_v4().to_string();
        let service_id = uuid::Uuid::new_v4().to_string();

        let is_ssh = service.service_type == "ssh";
        let ep_url = if is_ssh {
            service
                .ssh_config
                .as_ref()
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
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
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
            let _ = db
                .collection::<UserEndpoint>(USER_ENDPOINTS)
                .delete_one(doc! { "_id": &endpoint_id })
                .await;
            let _ = db
                .collection::<UserApiKey>(USER_API_KEYS)
                .delete_one(doc! { "_id": &api_key_id })
                .await;
            return Err(e.into());
        }

        created += 1;
    }

    if updated > 0 || created > 0 {
        tracing::info!(
            updated,
            created,
            "Migrated node service bindings to unified collections"
        );
    }
    Ok(())
}
