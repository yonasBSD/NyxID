# Node Management and SSH Remote Access

## Table of contents

- [Node Management](#node-management)
  - [Setting up a new node](#setting-up-a-new-node)
  - [Managing the node service](#managing-the-node-service)
  - [Managing nodes](#managing-nodes)
- [SSH Remote Access](#ssh-remote-access)

## Node Management

Nodes are for users who do not want their credentials stored on the NyxID server. Instead, credentials stay encrypted on the user's own machine (the node). When a proxy request comes in, NyxID passes it through to the node agent via WebSocket, the node injects the credential locally and forwards the request to the downstream service. The credential never leaves the node.

### Setting up a new node

Registration must happen before installing the daemon. Credentials can be added before or after starting -- the agent reloads them automatically within 5 seconds.

```bash
# Step 1: Generate a registration token (on any machine with nyxid CLI)
nyxid node register-token

# Step 2: Install nyxid CLI on the target machine
bash -c "$(curl -fsSL https://raw.githubusercontent.com/ChronoAIProject/NyxID/main/skills/nyxid/scripts/install.sh)"

# Step 3: Register the node (--keychain recommended for secure storage)
nyxid node register \
  --token "nyx_nreg_..." \
  --url "wss://<server>/api/v1/nodes/ws" \
  --keychain

# Step 4: Install and start as a background service (recommended)
nyxid node daemon install                              # install as system service
nyxid node daemon start                                # start the service

# Step 5: Add credentials (auto-registers catalog services in the backend)
nyxid node credentials setup --service llm-openai      # agent picks up new credentials automatically

# For custom endpoints: register first, then add credentials locally
nyxid service add --custom --via-node my-node           # creates backend record (prompts for URL, auth, etc.)
nyxid node credentials add --service my-api --header Authorization --secret-format bearer

# Or run in foreground (for debugging)
nyxid node start

# Or run via Docker
docker build -f cli/Dockerfile.node -t nyxid-node .    # build image (once)

# Option A: auto-register + start (no host setup needed)
docker run --user "$(id -u):$(id -g)" \
  -v ~/.nyxid-node:/app/config \
  -e NYXID_NODE_TOKEN=nyx_nreg_... \
  -e NYXID_NODE_URL=wss://<server>/api/v1/nodes/ws \
  nyxid-node

# Option B: mount existing config (registered on host)
docker run --user "$(id -u):$(id -g)" \
  -v ~/.nyxid-node:/app/config \
  nyxid-node
```

> `credentials setup` works for **catalog services only** -- it fetches config from the catalog and automatically registers the service in the backend with the node's ID.
> For **custom endpoints**, use `nyxid service add --custom --via-node <node-name>` first to create the backend record, then `nyxid node credentials add` to store the credential locally on the node.
> Credentials can be added, updated, or removed while the agent is running. The agent watches the config file and reloads credentials automatically (no restart needed). This works for both native daemons and Docker containers (config is mounted as a volume).
> Docker containers use the file backend (AES-GCM encrypted) -- OS keychain is not available in containers.

### Managing the node service

```bash
# Background service lifecycle (launchd on macOS, systemd on Linux)
nyxid node daemon install                              # install as system service (auto-starts on login)
nyxid node daemon install --force                      # reinstall / update service config
nyxid node daemon start                                # start the service
nyxid node daemon stop                                 # stop the service
nyxid node daemon restart                              # restart (picks up config changes)
nyxid node daemon status                               # check if installed and running
nyxid node daemon logs                                 # show recent logs (last 50 lines)
nyxid node daemon logs --follow                        # tail logs in real time
nyxid node daemon uninstall                             # remove service (stops first)
```

### Managing nodes

```bash
# nyxid CLI (manage nodes from user side)
nyxid node list --output json                          # list nodes (includes IDs)
nyxid node show <ID_OR_NAME> --output json             # show node details + metrics
nyxid node register-token                              # interactive: opens browser wizard (v3.1)
nyxid node register-token --name "edge-tokyo" --output json  # scripted: prints raw nyx_nreg_... (legacy shape)
nyxid node delete <ID_OR_NAME> --yes                   # delete node
nyxid node rotate-token <ID_OR_NAME>                   # interactive: opens browser wizard (shows new auth_token + signing_secret)
nyxid node rotate-token <ID_OR_NAME> --output json     # scripted: prints raw secret to stdout (legacy shape)

# nyxid node CLI (run on the node machine)
nyxid node credentials setup --service <SLUG>          # auto-detect and setup (recommended)
nyxid node credentials add --service <SLUG> --header Authorization --secret-format bearer
nyxid node credentials add-oauth --service <SLUG> --from-catalog  # OAuth from node
nyxid node credentials list                            # list configured credentials
nyxid node credentials remove --service <SLUG>         # remove credential
```

> `credentials setup` works for **catalog services**: it auto-detects whether the service needs an API key, OAuth, or gateway URL, guides the user through the right flow, and auto-registers the service in the backend with the node's ID. For **custom endpoints**, use `nyxid service add --custom --via-node <node>` first, then `nyxid node credentials add`.

## SSH Remote Access

All SSH commands accept service ID, slug, or name (auto-resolves). SSH slugs are scoped per-user -- two users can each have an SSH service with the same slug without conflict. MCP SSH tools (`ssh_exec`, `ssh_list`) only see the caller's own services.

```bash
nyxid ssh exec <SERVICE> --principal ubuntu -- uptime
nyxid ssh exec <SERVICE> --principal ubuntu -- ls -la /var/log
nyxid ssh terminal <SERVICE>                           # auto-resolves principal
nyxid ssh terminal <SERVICE> --principal ubuntu
nyxid ssh issue-cert <SERVICE> --public-key-file ~/.ssh/id_ed25519.pub --principal ubuntu --certificate-file ~/.ssh/id_ed25519-cert.pub
nyxid ssh proxy <SERVICE>                              # ProxyCommand for OpenSSH

# List SSH services
nyxid service list --output json | jq '.keys[] | select(.service_type == "ssh")'
```
