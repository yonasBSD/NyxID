use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "nyxid-node", about = "NyxID credential node agent")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, global = true)]
    pub log_level: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Register this node with a NyxID server
    Register {
        /// One-time registration token (nyx_nreg_...)
        #[arg(long)]
        token: String,

        /// WebSocket URL of the NyxID server
        #[arg(long)]
        url: Option<String>,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,

        /// Store secrets in the OS keychain instead of encrypted file
        #[arg(long)]
        keychain: bool,
    },

    /// Start the node agent (connect and serve)
    Start {
        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Show node connection status
    Status {
        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Update the node's auth token and signing secret after a server-side rotation
    Rekey {
        /// Replacement auth token (nyx_nauth_...)
        #[arg(long)]
        auth_token: String,

        /// Replacement HMAC signing secret (64 hex chars)
        #[arg(long)]
        signing_secret: String,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Manage local credentials
    Credentials {
        #[command(subcommand)]
        command: CredentialCommands,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Migrate secret storage from file to OS keychain (or vice versa)
    Migrate {
        /// Target backend: "keychain" or "file"
        #[arg(long)]
        to: String,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Manage OpenClaw integration (connect, status, disconnect)
    Openclaw {
        #[command(subcommand)]
        command: OpenClawCommands,

        /// Path to config directory
        #[arg(long)]
        config: Option<String>,
    },

    /// Show version information
    Version,
}

#[derive(Subcommand)]
pub enum OpenClawCommands {
    /// Connect to a local or remote OpenClaw gateway.
    /// Stores credentials locally, registers provider connection with NyxID,
    /// and creates the node service binding automatically.
    Connect {
        /// OpenClaw gateway URL (e.g., http://localhost:18789)
        #[arg(long)]
        url: String,

        /// OpenClaw gateway bearer token (OPENCLAW_GATEWAY_TOKEN).
        /// If omitted, will prompt securely.
        #[arg(long)]
        token: Option<String>,

        /// NyxID API base URL (e.g., http://localhost:3001). Defaults to server URL from config.
        #[arg(long)]
        api_url: Option<String>,

        /// NyxID access token for API calls. If omitted, uses NYXID_ACCESS_TOKEN env var.
        #[arg(long)]
        access_token: Option<String>,
    },

    /// Show OpenClaw connection status
    Status,

    /// Disconnect from OpenClaw: removes local credentials and the node binding
    Disconnect,
}

#[derive(Subcommand)]
pub enum CredentialCommands {
    /// Add a credential for a service (prompts for the secret value securely)
    Add {
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

        /// Query parameter name to inject (e.g., "api_key"). The value will be prompted securely.
        #[arg(long)]
        query_param: Option<String>,

        /// How to format the prompted secret before storing it.
        #[arg(long, value_enum, default_value_t = CredentialSecretFormat::Raw)]
        secret_format: CredentialSecretFormat,

        /// Inline secret value (skips interactive prompt; NOT recommended -- visible in shell history)
        #[arg(long, hide = true)]
        value: Option<String>,
    },

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
    },

    /// Auto-setup credentials for a service (fetches requirements from catalog)
    Setup {
        /// Service slug (e.g., "llm-openai", "api-twitter")
        #[arg(long)]
        service: String,

        /// NyxID API base URL (defaults to server URL from config)
        #[arg(long)]
        api_url: Option<String>,

        /// NyxID access token (defaults to NYXID_ACCESS_TOKEN env var)
        #[arg(long)]
        access_token: Option<String>,
    },

    /// List configured credentials
    List,

    /// Remove a credential for a service
    Remove {
        /// Service slug to remove
        #[arg(long)]
        service: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CredentialSecretFormat {
    /// Store the secret exactly as entered.
    Raw,
    /// Prefix the secret with "Bearer ".
    Bearer,
    /// Base64-encode "username:password" and prefix it with "Basic ".
    Basic,
}
