> **Internal implementation spec -- see [AI_AGENT_PLAYBOOK.md](AI_AGENT_PLAYBOOK.md) for user-facing documentation.**

# NyxID CLI Implementation Spec

**Status:** Draft
**Crate:** `cli/` (new workspace member, binary name: `nyxid`)
**Purpose:** Standalone CLI for users to manage their NyxID account. AI agents use it via skills/playbooks to configure everything without the web frontend.

---

## 0. Relationship to Existing CLI

The backend binary (`backend/src/main.rs`) already has `Login` and `Ssh` subcommands:
- `login_cli.rs` -- browser login + password login, stores token at `~/.nyxid/access_token`
- `ssh_cli.rs` -- SSH certificate issuance, WebSocket proxy, OpenSSH config generation

The new `cli/` crate is a **separate standalone binary** that:
- Reuses the **same token storage** (`~/.nyxid/access_token`) for compatibility
- Reuses the **same login flow patterns** (browser + password)
- Reuses the **same token resolution** (`--access-token` > `NYXID_ACCESS_TOKEN` env > `~/.nyxid/access_token`)
- Does NOT depend on the backend crate -- pure API client using `reqwest`
- Can be installed independently (like `nyxid-node`)

The existing `Login` and `Ssh` subcommands in the backend binary remain for backward compatibility. The new CLI is the primary user-facing tool going forward.

---

## 1. Project Structure

```
cli/
  Cargo.toml
  src/
    main.rs               # Entry point, clap dispatch
    cli.rs                # Clap Parser + Subcommand enums
    auth.rs               # Login flows (browser + password), token storage, token resolution
    api.rs                # HTTP client wrapper (reqwest + auth header injection)
    commands/
      mod.rs              # Re-exports
      whoami.rs            # whoami
      status.rs            # status overview
      catalog.rs           # catalog list, catalog show
      service.rs           # service add/list/show/delete/update
      api_key.rs           # api-key create/list/show/rotate/delete/update
      node.rs              # node list/show/register-token
      ssh.rs               # ssh issue-cert/proxy/config (mirrors backend ssh_cli.rs)
      openclaw.rs          # openclaw setup
      mcp.rs               # mcp config
```

Add `"cli"` to the workspace `members` array in the root `Cargo.toml`:
```toml
[workspace]
members = ["backend", "node-agent", "cli"]
```

---

## 2. Cargo.toml

```toml
[package]
name = "nyxid-cli"
version = "0.1.0"
edition = "2024"
license.workspace = true
repository.workspace = true

[[bin]]
name = "nyxid"
path = "src/main.rs"

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

anyhow = "1"                  # Error handling (matches existing CLI pattern)
clap = { version = "4.5", features = ["derive"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
dirs = "6"                    # Home directory (matches login_cli.rs)
open = "5"                    # Open browser (matches login_cli.rs)
rpassword = "7"               # Secure password prompts (matches login_cli.rs)
urlencoding = "2.1"           # URL decode callback params (matches login_cli.rs)
rand = "0.8"                  # State parameter generation (matches login_cli.rs)
hex = "0.4"                   # Hex encoding for state (matches login_cli.rs)
comfy-table = "7"             # Terminal table formatting
toml = "0.8"                  # Config file (server URL)
dialoguer = { version = "0.11", features = ["password"] }  # Interactive prompts
url = "2"                     # URL parsing (matches ssh_cli.rs)

# SSH subcommand dependencies
tokio-tungstenite = { version = "0.26", features = ["rustls-tls-native-roots"] }
futures = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

---

## 3. Clap Command Structure

```rust
// cli.rs

use clap::{Parser, Subcommand, Args};

#[derive(Parser)]
#[command(
    name = "nyxid",
    about = "NyxID CLI -- manage your NyxID account from the terminal",
    version,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Log in to NyxID (opens browser by default)
    Login(LoginArgs),

    /// Log out and clear stored token
    Logout(BaseUrlArgs),

    /// Show current user info
    Whoami(AuthArgs),

    /// Show account overview (services, keys, nodes)
    Status(AuthArgs),

    /// Browse the service catalog
    Catalog {
        #[command(subcommand)]
        command: CatalogCommands,
    },

    /// Manage AI services (external APIs)
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },

    /// Manage NyxID API keys
    ApiKey {
        #[command(subcommand)]
        command: ApiKeyCommands,
    },

    /// Manage credential nodes
    Node {
        #[command(subcommand)]
        command: NodeCommands,
    },

    /// SSH client helper commands
    Ssh(SshCli),

    /// Set up OpenClaw integration
    Openclaw {
        #[command(subcommand)]
        command: OpenClawCommands,
    },

    /// Generate MCP configuration for AI tools
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
}

// ---- Shared auth args (mirrors ssh_cli.rs AuthArgs) ----

#[derive(Args, Clone)]
pub struct AuthArgs {
    /// NyxID base URL, e.g. https://auth.nyxid.dev
    #[arg(long, env = "NYXID_URL")]
    pub base_url: String,

    /// Access token (overrides saved token)
    #[arg(long)]
    pub access_token: Option<String>,

    /// Environment variable to read the access token from
    #[arg(long, default_value = "NYXID_ACCESS_TOKEN")]
    pub access_token_env: String,

    /// Output format: table or json
    #[arg(long, default_value = "table")]
    pub output: OutputFormat,
}

#[derive(Args, Clone)]
pub struct BaseUrlArgs {
    /// NyxID base URL
    #[arg(long, env = "NYXID_URL")]
    pub base_url: String,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

// ---- Login ----

#[derive(Args)]
pub struct LoginArgs {
    /// NyxID base URL, e.g. https://auth.nyxid.dev
    #[arg(long, env = "NYXID_URL")]
    pub base_url: String,

    /// Use email/password login instead of opening the browser
    #[arg(long)]
    pub password: bool,

    /// Email address (only used with --password)
    #[arg(long)]
    pub email: Option<String>,
}

// ---- Catalog ----

#[derive(Subcommand)]
pub enum CatalogCommands {
    /// List available services from the catalog
    List {
        #[command(flatten)]
        auth: AuthArgs,
    },
    /// Show details for a catalog entry
    Show {
        /// Service slug (e.g., llm-openai)
        slug: String,
        #[command(flatten)]
        auth: AuthArgs,
    },
}

// ---- Service ----

#[derive(Subcommand)]
pub enum ServiceCommands {
    /// Add a service from catalog or custom endpoint
    Add {
        /// Catalog slug (e.g., llm-openai). Omit with --custom for a custom endpoint.
        slug: Option<String>,

        /// Add a fully custom endpoint (interactive prompts)
        #[arg(long)]
        custom: bool,

        /// Route traffic through a node
        #[arg(long)]
        via_node: Option<String>,

        /// Endpoint URL override
        #[arg(long)]
        endpoint_url: Option<String>,

        /// Label for this service
        #[arg(long)]
        label: Option<String>,

        /// Auth method (bearer, header, query, basic)
        #[arg(long)]
        auth_method: Option<String>,

        /// Auth key name (e.g., Authorization, X-API-Key)
        #[arg(long)]
        auth_key_name: Option<String>,

        #[command(flatten)]
        auth: AuthArgs,
    },

    /// List user's configured services
    List {
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Show service details
    Show {
        /// Service ID
        id: String,
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Delete a service
    Delete {
        /// Service ID
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Update service configuration
    Update {
        /// Service ID
        id: String,

        /// New endpoint URL
        #[arg(long)]
        endpoint_url: Option<String>,

        /// New node ID for routing
        #[arg(long)]
        node_id: Option<String>,

        /// Remove node routing (direct mode)
        #[arg(long)]
        no_node: bool,

        #[command(flatten)]
        auth: AuthArgs,
    },
}

// ---- API Key ----

#[derive(Subcommand)]
pub enum ApiKeyCommands {
    /// Create a new NyxID API key
    Create {
        /// Key name
        #[arg(long)]
        name: Option<String>,

        /// Scopes (space-separated: read write proxy)
        #[arg(long)]
        scopes: Option<String>,

        /// Expiry in days (0 = no expiry)
        #[arg(long)]
        expires_in_days: Option<u32>,

        /// Allowed service IDs (comma-separated)
        #[arg(long)]
        allowed_services: Option<String>,

        /// Allowed node IDs (comma-separated)
        #[arg(long)]
        allowed_nodes: Option<String>,

        /// Allow access to all services
        #[arg(long)]
        allow_all_services: bool,

        /// Allow access to all nodes
        #[arg(long)]
        allow_all_nodes: bool,

        #[command(flatten)]
        auth: AuthArgs,
    },

    /// List API keys
    List {
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Show key details
    Show {
        id: String,
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Rotate a key
    Rotate {
        id: String,
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Revoke a key
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        auth: AuthArgs,
    },

    /// Update key scope
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        scopes: Option<String>,
        #[arg(long)]
        allowed_services: Option<String>,
        #[arg(long)]
        allowed_nodes: Option<String>,
        #[arg(long)]
        allow_all_services: Option<bool>,
        #[arg(long)]
        allow_all_nodes: Option<bool>,
        #[command(flatten)]
        auth: AuthArgs,
    },
}

// ---- Node ----

#[derive(Subcommand)]
pub enum NodeCommands {
    /// List user's nodes
    List {
        #[command(flatten)]
        auth: AuthArgs,
    },
    /// Show node details
    Show {
        id: String,
        #[command(flatten)]
        auth: AuthArgs,
    },
    /// Generate a registration token
    RegisterToken {
        #[command(flatten)]
        auth: AuthArgs,
    },
}

// ---- SSH (mirrors backend ssh_cli.rs) ----

#[derive(Args)]
pub struct SshCli {
    #[command(subcommand)]
    pub command: SshCommand,
}

#[derive(Subcommand)]
pub enum SshCommand {
    /// Issue a short-lived SSH certificate
    IssueCert {
        #[command(flatten)]
        auth: AuthArgs,
        #[arg(long)]
        service_id: String,
        #[arg(long)]
        public_key_file: std::path::PathBuf,
        #[arg(long)]
        principal: String,
        #[arg(long)]
        certificate_file: std::path::PathBuf,
        #[arg(long)]
        ca_public_key_file: Option<std::path::PathBuf>,
    },
    /// Open an SSH-over-WebSocket tunnel (ProxyCommand)
    Proxy {
        #[command(flatten)]
        auth: AuthArgs,
        #[arg(long)]
        service_id: String,
        #[arg(long, default_value_t = false)]
        issue_certificate: bool,
        #[arg(long)]
        public_key_file: Option<std::path::PathBuf>,
        #[arg(long)]
        principal: Option<String>,
        #[arg(long)]
        certificate_file: Option<std::path::PathBuf>,
        #[arg(long)]
        ca_public_key_file: Option<std::path::PathBuf>,
    },
    /// Print an OpenSSH config stanza
    Config {
        #[arg(long)]
        host_alias: String,
        #[arg(long)]
        base_url: String,
        #[arg(long)]
        service_id: String,
        #[arg(long)]
        principal: String,
        #[arg(long)]
        identity_file: std::path::PathBuf,
        #[arg(long)]
        certificate_file: std::path::PathBuf,
        #[arg(long, default_value = "NYXID_ACCESS_TOKEN")]
        access_token_env: String,
        #[arg(long)]
        ca_public_key_file: Option<std::path::PathBuf>,
    },
}

// ---- OpenClaw ----

#[derive(Subcommand)]
pub enum OpenClawCommands {
    /// Interactive OpenClaw setup
    Setup {
        /// OpenClaw gateway URL
        #[arg(long)]
        url: Option<String>,
        #[command(flatten)]
        auth: AuthArgs,
    },
}

// ---- MCP ----

#[derive(Subcommand)]
pub enum McpCommands {
    /// Generate MCP configuration for AI tools
    Config {
        /// Target tool: cursor, claude-code, vscode, generic
        #[arg(long, default_value = "generic")]
        tool: String,
        #[command(flatten)]
        auth: AuthArgs,
    },
}
```

---

## 4. Token Storage and Resolution

### Storage

**Same as existing `login_cli.rs`:** plain text file at `~/.nyxid/access_token`.

```rust
// auth.rs (replicates login_cli.rs patterns exactly)

use std::path::PathBuf;
use anyhow::{Context, Result, bail};

const TOKEN_DIR_NAME: &str = ".nyxid";
const TOKEN_FILE_NAME: &str = "access_token";
const CALLBACK_TIMEOUT_SECS: u64 = 120;

fn token_file_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(TOKEN_DIR_NAME).join(TOKEN_FILE_NAME))
}

pub fn read_saved_token() -> Option<String> {
    let path = token_file_path().ok()?;
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn save_token(token: &str) -> Result<()> {
    let path = token_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, token)
        .with_context(|| format!("Failed to write token to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn clear_token() -> Result<()> {
    let path = token_file_path()?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}
```

### Resolution

**Same 3-step pattern as `ssh_cli.rs`:**

```rust
// auth.rs

use crate::cli::AuthArgs;

pub fn resolve_access_token(auth: &AuthArgs) -> Result<String> {
    // 1. Explicit --access-token flag
    if let Some(token) = &auth.access_token {
        return Ok(token.clone());
    }

    // 2. Environment variable (NYXID_ACCESS_TOKEN by default)
    if let Ok(token) = std::env::var(&auth.access_token_env) {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 3. Saved token from `nyxid login`
    if let Some(token) = read_saved_token() {
        return Ok(token);
    }

    bail!(
        "No access token found. Run `nyxid login --base-url <URL>`, \
         set {}, or pass --access-token",
        auth.access_token_env
    )
}
```

---

## 5. Login Flows

### Browser Login (default)

Copied from `login_cli.rs` with identical behavior:

```
1. Call GET {base_url}/api/v1/public/config to discover frontend_url
2. Bind TCP listener to 127.0.0.1:0 (random port)
3. Generate random state (16 bytes, hex-encoded)
4. Open browser to {frontend_url}/cli-auth?port={port}&state={state}
5. Wait for callback: GET /callback?access_token=...&state=...
6. Validate state, extract token
7. Return success HTML to browser
8. Save token to ~/.nyxid/access_token
```

### Password Login (`--password`)

Copied from `login_cli.rs` with identical behavior:

```
1. Prompt for email (if not --email)
2. Prompt for password via rpassword
3. POST {base_url}/api/v1/auth/login { email, password, client: "cli" }
4. Save access_token to ~/.nyxid/access_token
```

### Logout

```
1. Read saved token
2. Call POST {base_url}/api/v1/auth/logout with Bearer token (best-effort)
3. Delete ~/.nyxid/access_token
```

---

## 6. API Client

```rust
// api.rs

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct ApiClient {
    client: Client,
    base_url: String,
    access_token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, access_token: String) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: format!("{}/api/v1", base_url.trim_end_matches('/')),
            access_token,
        })
    }

    /// Convenience: resolve token from AuthArgs, build client.
    pub fn from_auth(auth: &crate::cli::AuthArgs) -> Result<Self> {
        let token = crate::auth::resolve_access_token(auth)?;
        Self::new(&auth.base_url, token)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.get(&url).bearer_auth(&self.access_token).send().await
            .with_context(|| format!("GET {path} failed"))?;
        Self::handle_response(resp, path).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.access_token).json(body).send().await
            .with_context(|| format!("POST {path} failed"))?;
        Self::handle_response(resp, path).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.put(&url).bearer_auth(&self.access_token).json(body).send().await
            .with_context(|| format!("PUT {path} failed"))?;
        Self::handle_response(resp, path).await
    }

    pub async fn delete_empty(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.delete(&url).bearer_auth(&self.access_token).send().await
            .with_context(|| format!("DELETE {path} failed"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("DELETE {path} failed (HTTP {status}): {body}");
        }
    }

    pub async fn post_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.post(&url).bearer_auth(&self.access_token).json(body).send().await
            .with_context(|| format!("POST {path} failed"))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("POST {path} failed (HTTP {status}): {body}");
        }
    }

    async fn handle_response<T: DeserializeOwned>(resp: reqwest::Response, path: &str) -> Result<T> {
        if resp.status().is_success() {
            resp.json().await
                .with_context(|| format!("Failed to parse response from {path}"))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("{path} failed (HTTP {status}): {body}");
        }
    }
}
```

---

## 7. Main Entry Point

```rust
// main.rs

mod auth;
mod api;
mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use crate::cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    // No structured logging for CLI -- just eprintln for errors
    if let Err(e) = run().await {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Login(args) => auth::run_login(args).await,
        Commands::Logout(args) => auth::run_logout(&args.base_url).await,

        Commands::Whoami(auth) => {
            let api = api::ApiClient::from_auth(&auth)?;
            commands::whoami::run(&api, auth.output).await
        }
        Commands::Status(auth) => {
            let api = api::ApiClient::from_auth(&auth)?;
            commands::status::run(&api, auth.output).await
        }
        Commands::Catalog { command } => commands::catalog::run(command).await,
        Commands::Service { command } => commands::service::run(command).await,
        Commands::ApiKey { command } => commands::api_key::run(command).await,
        Commands::Node { command } => commands::node::run(command).await,
        Commands::Ssh(ssh) => commands::ssh::run(ssh).await,
        Commands::Openclaw { command } => commands::openclaw::run(command).await,
        Commands::Mcp { command } => commands::mcp::run(command).await,
    }
}
```

Each command module extracts `AuthArgs` from its variant, calls `ApiClient::from_auth(&auth)`, and proceeds.

---

## 8. Command Implementations

### 8.1 `nyxid whoami`

```
API: GET /users/me
Display:
  User ID:  abc-def-123
  Email:    user@example.com
  Name:     Jane Doe
  Role:     user
  MFA:      enabled
  Verified: yes
```

### 8.2 `nyxid status`

```
APIs (parallel):
  GET /users/me
  GET /keys
  GET /api-keys
  GET /nodes

Display:
  Account: user@example.com (user)
  Server:  https://auth.nyxid.dev

  AI Services (3)
  +---------------+--------------------------+--------+--------+
  | Slug          | Endpoint                 | Status | Node   |
  +---------------+--------------------------+--------+--------+
  | llm-openai    | api.openai.com/v1        | active | --     |
  | llm-anthropic | api.anthropic.com/v1     | active | --     |
  | llm-openclaw  | localhost:18789          | active | laptop |
  +---------------+--------------------------+--------+--------+

  API Keys (2)
  +----------------+-----------+-----------+
  | Name           | Scopes    | Expires   |
  +----------------+-----------+-----------+
  | AI Agent Key   | read write| 2026-04-25|
  | CI/CD Key      | read      | never     |
  +----------------+-----------+-----------+

  Nodes (1)
  +------------+--------+------------------+
  | Name       | Status | Last Seen        |
  +------------+--------+------------------+
  | my-laptop  | online | 2 minutes ago    |
  +------------+--------+------------------+
```

### 8.3 `nyxid catalog list`

```
API: GET /catalog
Display:
  Available Services
  +------------------+--------------------+-------------------------------+
  | Slug             | Name               | Default URL                   |
  +------------------+--------------------+-------------------------------+
  | llm-openai       | OpenAI             | https://api.openai.com/v1     |
  | llm-anthropic    | Anthropic          | https://api.anthropic.com/v1  |
  | llm-openclaw     | OpenClaw           | (requires endpoint URL)       |
  +------------------+--------------------+-------------------------------+

  Use `nyxid service add <slug>` to add a service.
```

### 8.4 `nyxid catalog show <slug>`

```
API: GET /catalog/{slug}
Display:
  Service: OpenAI (llm-openai)

  Default URL:     https://api.openai.com/v1
  Auth Method:     bearer
  Auth Key Name:   Authorization
  Category:        llm

  Add this service:
    nyxid service add llm-openai --base-url <URL>
```

### 8.5 `nyxid service add <slug>`

```
API: POST /keys
Body: {
  "service_slug": "<slug>",
  "credential": "<prompted via rpassword>",
  "label": "<from --label or default>",
  "endpoint_url": "<if --endpoint-url>",
  "node_id": "<if --via-node>",
  "auth_method": "<if --auth-method>",
  "auth_key_name": "<if --auth-key-name>"
}

Flow:
1. If --via-node is set, skip credential prompt (node provides it)
2. Otherwise, prompt: "Enter API key/credential:" via rpassword (hidden)
3. Call POST /keys
4. Print result

Display:
  Service added successfully!

  Slug:      llm-openai
  Endpoint:  https://api.openai.com/v1
  Status:    active

  Proxy URL: https://auth.nyxid.dev/api/v1/proxy/s/llm-openai/
```

### 8.6 `nyxid service add --custom`

```
Interactive flow:
1. Prompt: Label?
2. Prompt: Endpoint URL?
3. Prompt: Auth method? [bearer/header/query/basic] (default: bearer)
4. Prompt: Auth key name? (default: Authorization)
5. Prompt: Credential (hidden via rpassword)
6. Optional: Route via node? [y/N] -> if yes, select from node list

API: POST /keys (fully custom, no service_slug)
```

### 8.7 `nyxid service list`

```
API: GET /keys
Display:
  +--------------+--------------------------+--------+--------+
  | Slug         | Endpoint                 | Status | Node   |
  +--------------+--------------------------+--------+--------+
  | llm-openai   | api.openai.com/v1        | active | --     |
  | llm-openclaw | localhost:18789          | active | laptop |
  +--------------+--------------------------+--------+--------+
```

### 8.8 `nyxid service show <id>`

```
API: GET /keys/{id}
Display:
  Service: OpenAI (llm-openai)
  ID:         abc-123
  Status:     active

  Endpoint:   https://api.openai.com/v1
  Auth:       bearer / Authorization
  Node:       -- (direct)

  Proxy URL:  https://auth.nyxid.dev/api/v1/proxy/s/llm-openai/
```

### 8.9 `nyxid service delete <id>`

```
API: DELETE /keys/{id}
Flow: Confirm prompt unless --yes. Print "Service deleted."
```

### 8.10 `nyxid service update <id>`

```
APIs:
  PUT /endpoints/{endpoint_id}         (if --endpoint-url)
  PUT /user-services/{service_id}      (if --node-id or --no-node)
Display: "Service updated."
```

### 8.11 `nyxid api-key create`

```
API: POST /api-keys
Flow:
1. Prompt for name if not --name
2. Default scopes: "read write"
3. Print created key (shown once)

Display:
  API key created!

  Name:    AI Agent Key
  Key:     nyxid_abc123xyz...  (save this -- shown only once)
  Scopes:  read write
  Expires: 2026-04-25

  Set as environment variable:
    export NYXID_API_KEY="nyxid_abc123xyz..."
```

### 8.12 `nyxid api-key list`

```
API: GET /api-keys
Display: Table with Name, Scopes, Expires, Last Used columns.
```

### 8.13 `nyxid api-key show <id>`

```
API: GET /api-keys/{id}
Display: Key details including scope restrictions.
```

### 8.14 `nyxid api-key rotate <id>`

```
API: POST /api-keys/{id}/rotate
Display:
  Key rotated!
  New Key: nyxid_newkey123...  (save this -- shown only once)
```

### 8.15 `nyxid api-key delete <id>`

```
API: DELETE /api-keys/{id}
Flow: Confirm prompt unless --yes. Print "API key revoked."
```

### 8.16 `nyxid api-key update <id>`

```
API: PUT /api-keys/{id}
Body: Only fields provided via flags.
Display: "API key updated."
```

### 8.17 `nyxid node list`

```
API: GET /nodes
Display: Table with Name, Status, Last Seen, Services columns.
```

### 8.18 `nyxid node show <id>`

```
API: GET /nodes/{id}
Display: Node details including metrics.
```

### 8.19 `nyxid node register-token`

```
API: POST /nodes/register-token
Display:
  Registration token created.

  Token:   nyx_nreg_abc123...
  Expires: in 1 hour

  Register a node:
    nyxid-node register --token nyx_nreg_abc123... --url ws://<server>/api/v1/nodes/ws
```

### 8.20 `nyxid ssh issue-cert / proxy / config`

**Direct port of `backend/src/ssh_cli.rs`.** Same flags, same behavior, same WebSocket proxy logic. The only difference is token resolution uses `auth.rs::resolve_access_token` instead of `crate::login_cli::read_saved_token` (identical logic, different import path).

### 8.21 `nyxid openclaw setup`

```
Interactive flow:
1. Prompt: OpenClaw gateway URL? (or --url)
2. Prompt: Bearer token? (hidden via rpassword)
3. POST /keys { service_slug: "llm-openclaw", credential, endpoint_url, label: "OpenClaw" }

Display:
  OpenClaw configured!

  Slug:      llm-openclaw
  Endpoint:  http://localhost:18789
  Status:    active

  Proxy URL: .../api/v1/proxy/s/llm-openclaw/

  Generate MCP config:
    nyxid mcp config --tool claude-code --base-url <URL>
```

### 8.22 `nyxid mcp config`

```
API: GET /mcp/config
Generates tool-specific config snippets.

--tool cursor:
  {
    "mcpServers": {
      "nyxid": {
        "url": "https://auth.nyxid.dev/mcp",
        "headers": { "Authorization": "Bearer ${NYXID_API_KEY}" }
      }
    }
  }

--tool claude-code:
  claude mcp add nyxid --transport streamable-http \
    --url "https://auth.nyxid.dev/mcp" \
    --header "Authorization: Bearer ${NYXID_API_KEY}"

--tool generic:
  MCP Server URL: https://auth.nyxid.dev/mcp
  Authorization: Bearer <your-api-key>
```

---

## 9. Output Formatting

**Table output** (default): `comfy-table` for terminal tables.

**JSON output** (`--output json`): Raw JSON from the API response, no decoration. Makes the CLI scriptable.

```rust
// In each command module:
match auth.output {
    OutputFormat::Json => {
        println!("{}", serde_json::to_string_pretty(&data)?);
    }
    OutputFormat::Table => {
        // comfy-table rendering
    }
}
```

---

## 10. Implementation Phases

### Phase 1 (MVP)

Everything needed for AI agent workflows:

1. `auth.rs` -- browser login, password login, logout, token resolution (port from `login_cli.rs`)
2. `api.rs` -- HTTP client with bearer auth
3. `cli.rs` -- full clap structure
4. Commands: `whoami`, `status`, `catalog list/show`, `service add/list/show/delete`, `api-key create/list/show/rotate/delete`, `node list/show/register-token`, `openclaw setup`, `mcp config`
5. Output: table + JSON formatting

### Phase 2

1. `ssh` subcommand (port from `ssh_cli.rs`)
2. `service update` (endpoint URL + node routing)
3. `api-key update` (scope changes)

### Phase 3

1. Shell completions (`nyxid completions bash/zsh/fish`)
2. Config file for default `--base-url` (`~/.nyxid/config.toml`)
3. Version check against server

---

## 11. Security

1. **Token file:** `~/.nyxid/access_token` with `0600` permissions (owner read/write only)
2. **No secrets in args:** Credentials prompted via `rpassword` (hidden), never as positional args
3. **State parameter:** Browser login validates `state` to prevent CSRF (same as `login_cli.rs`)
4. **Localhost callback:** Binds to `127.0.0.1` only (same as `login_cli.rs`)
5. **No shell history:** `rpassword` prevents secrets from appearing in shell history

---

## 12. AI Agent Integration

The CLI is designed for AI agents to invoke:

```bash
# Check if logged in
nyxid whoami --base-url $NYXID_URL --output json

# Browse catalog
nyxid catalog list --base-url $NYXID_URL --output json

# Add a service (credential prompted -- AI tells user to run this themselves)
nyxid service add llm-openai --base-url $NYXID_URL --output json

# Create an API key for proxy access
nyxid api-key create --name "Agent Key" --scopes "read write proxy" \
  --base-url $NYXID_URL --output json

# Generate MCP config
nyxid mcp config --tool claude-code --base-url $NYXID_URL
```

All commands support `--output json` for machine-readable output. Errors go to stderr, data to stdout.
