use mongodb::bson::doc;

use crate::config::AppConfig;
use crate::crypto::aes::EncryptionKeys;

/// Connect to a fresh per-test MongoDB database. Tries the docker-compose
/// instance first, then a plain local mongod. Returns `None` if neither is
/// reachable so integration tests can skip cleanly in environments without
/// a running MongoDB.
pub(crate) async fn connect_test_database(prefix: &str) -> Option<mongodb::Database> {
    let db_name = format!("nyxid_test_{prefix}_{}", uuid::Uuid::new_v4());
    let candidates = [
        format!("mongodb://nyxid:nyxid_dev_password@127.0.0.1:27018/{db_name}?authSource=admin"),
        format!("mongodb://127.0.0.1:27017/{db_name}"),
    ];

    for uri in candidates {
        let Ok(client) = mongodb::Client::with_uri_str(&uri).await else {
            continue;
        };
        let db = client.database(&db_name);
        if db.run_command(doc! { "ping": 1 }).await.is_ok() {
            return Some(db);
        }
    }

    None
}

/// Build an `AppConfig` suitable for unit tests that need access to the
/// config's side-effect-free fields (encryption key, limits, feature flags).
pub(crate) fn test_app_config() -> AppConfig {
    AppConfig {
        port: 3001,
        base_url: "http://localhost:3001".to_string(),
        frontend_url: "http://localhost:3000".to_string(),
        cors_allowed_origins: vec![],
        database_url: "mongodb://ignored-for-test".to_string(),
        database_max_connections: 10,
        environment: "test".to_string(),
        jwt_private_key_path: "keys/private.pem".to_string(),
        jwt_public_key_path: "keys/public.pem".to_string(),
        jwt_issuer: "nyxid".to_string(),
        jwt_access_ttl_secs: 900,
        jwt_refresh_ttl_secs: 604800,
        google_client_id: None,
        google_client_secret: None,
        github_client_id: None,
        github_client_secret: None,
        apple_client_id: None,
        apple_team_id: None,
        apple_key_id: None,
        apple_private_key_path: None,
        smtp_host: None,
        smtp_port: None,
        smtp_username: None,
        smtp_password: None,
        smtp_from_address: None,
        encryption_key: Some("11".repeat(32)),
        encryption_key_previous: None,
        rate_limit_per_second: 10,
        rate_limit_burst: 30,
        sa_token_ttl_secs: 3600,
        cookie_domain: None,
        telegram_bot_token: None,
        telegram_webhook_secret: None,
        telegram_webhook_url: None,
        telegram_bot_username: None,
        approval_expiry_interval_secs: 5,
        fcm_service_account_path: None,
        fcm_project_id: None,
        apns_key_path: None,
        apns_key_id: None,
        apns_team_id: None,
        apns_topic: None,
        apns_sandbox: true,
        key_provider: "local".to_string(),
        aws_kms_key_arn: None,
        aws_kms_key_arn_previous: None,
        gcp_kms_key_name: None,
        gcp_kms_key_name_previous: None,
        node_heartbeat_interval_secs: 30,
        node_heartbeat_timeout_secs: 90,
        node_proxy_timeout_secs: 30,
        node_registration_token_ttl_secs: 3600,
        node_max_per_user: 10,
        node_max_ws_connections: 100,
        node_max_stream_duration_secs: 300,
        node_hmac_signing_enabled: true,
        proxy_max_body_size: 100 * 1024 * 1024,
        proxy_stream_idle_timeout_secs: 60,
        ssh_max_sessions_per_user: 4,
        ssh_connect_timeout_secs: 10,
        ssh_max_tunnel_duration_secs: 3600,
        ws_passthrough_max_connections: 200,
        channel_relay_callback_timeout_secs: 30,
        channel_relay_max_bots_per_user: 5,
        channel_relay_message_ttl_days: 30,
        channel_event_rate_limit_per_second: 100,
        channel_event_rate_limit_burst: 200,
        channel_event_dedup_capacity: 32_768,
        channel_event_dedup_ttl_secs: 300,
        invite_code_required: false,
        email_auth_enabled: false,
        auto_verify_email: false,
    }
}

pub(crate) fn test_encryption_keys() -> EncryptionKeys {
    EncryptionKeys::from_config(&test_app_config())
}
