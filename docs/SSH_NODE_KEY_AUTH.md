# SSH Node-Key Auth Mode

NyxID SSH services support three auth modes:

| Mode | Value | Supported operations |
|---|---|---|
| Certificate | `cert` | `ssh proxy`, `ssh exec`, browser terminal, `ssh issue-cert` |
| Node key | `node_key` | `ssh exec`, browser terminal |
| Proxy only | `proxy_only` | `ssh proxy` transport only |

`node_key` keeps the target SSH private key on the node host. The NyxID server forwards only the service slug, SSH principal, command or shell request, and signed routing metadata over the node WebSocket. The node agent looks up a local encrypted entry keyed by `(service_slug, principal)` and authenticates to the downstream SSH target with `russh`.

## Create a Node-Key SSH Service

Create or convert the service from a logged-in operator/admin machine:

```bash
nyxid service add-ssh routeros \
  --host 10.0.0.1 \
  --port 22 \
  --via-node edge-node \
  --node-key \
  --principals nyxid-ro,nyxid-admin
```

Then provision credentials on the node machine:

```bash
nyxid node ssh-credentials add \
  --service routeros \
  --principal nyxid-ro \
  --key-file ~/.ssh/routeros_ro \
  --host 10.0.0.1 \
  --port 22
```

One service can hold multiple principals with separate private keys:

```bash
nyxid node ssh-credentials add --service routeros --principal nyxid-admin --key-file ~/.ssh/routeros_admin --host 10.0.0.1 --port 22
nyxid node ssh-credentials list --service routeros
```

The node config stores only non-secret metadata in a flat `ssh_keys` list. Private keys and optional passphrases are stored through the node secret backend using per-entry secret names. Existing API credential entries are unchanged; operators do not need to re-register nodes or re-enter existing node credentials.

## Execute Commands

When a service has one local principal, the CLI can infer it:

```bash
nyxid ssh exec routeros -- /system/resource/print
```

When two or more principals are registered for the same service, select one explicitly:

```bash
nyxid ssh exec routeros --principal nyxid-ro -- /system/resource/print
```

If no local credential exists for the selected principal, the backend returns `1011 SshNodeKeyMissing`. If more than one local principal exists and the caller omits `--principal`, the CLI returns `1014 SshPrincipalAmbiguous` and lists the available principals.

`nyxid ssh proxy` is intentionally unsupported for node-key services and returns `1015 SshAuthModeUnsupportedForOperation`. Use `nyxid ssh exec` or the browser terminal for node-key services.

## Convert Modes

Use the conversion helper for existing SSH services:

```bash
nyxid service convert-ssh routeros --to-node-key
nyxid service convert-ssh routeros --to-cert
nyxid service convert-ssh routeros --to-proxy-only
```

Converting to `node_key` does not check whether a node-local key has already been provisioned because the target node may be offline. `ssh exec` returns `1011 SshNodeKeyMissing` until an operator provisions at least one credential with `nyxid node ssh-credentials add`.

Converting away from `node_key` leaves local node-key entries in the node store. They are marked stale by server-side state and can be removed on the node with:

```bash
nyxid node ssh-credentials prune --stale
```

Every conversion emits audit event `service.ssh_auth_mode_changed` with `{from, to, actor}`.

## Host-Key Pinning

By default, `nyxid node ssh-credentials add` scans the target SSH host key and stores its SHA-256 fingerprint. The node agent verifies that fingerprint before authenticating and returns `1012 SshHostKeyMismatch` if the target presents a different host key.

Use `--no-pin-host-key` only when the operator accepts the risk of trusting any host key for that entry:

```bash
nyxid node ssh-credentials add --service routeros --principal nyxid-ro --key-file ~/.ssh/routeros_ro --host 10.0.0.1 --no-pin-host-key
```

## Node WebSocket Frames

Node-key command execution uses:

- `ssh_node_exec_open` from NyxID to node with `request_id`, `service_slug`, `principal`, `command`, and `timeout_secs`.
- `ssh_node_exec_data` from node to NyxID for stdout/stderr chunks.
- `ssh_node_exec_close` from node to NyxID for normal completion.
- `ssh_node_exec_error` from node to NyxID for missing keys, host-key mismatch, or SSH channel errors.

The browser terminal reuses `web_terminal_open` with `auth_mode: "node_key"` and the same `web_terminal_data`, `web_terminal_resize`, and `web_terminal_closed` stream frames as cert-mode terminals.
