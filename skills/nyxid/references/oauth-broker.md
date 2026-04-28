# OAuth broker bindings (Authorizations)

NyxID can act as an OAuth broker for third-party agent platforms (aevatar, custom apps): when a user authorizes such a platform via NyxID, the platform receives an opaque `binding_id` instead of a refresh_token. NyxID holds the refresh_token server-side, and the platform exchanges its `binding_id` for short-lived access tokens via standard RFC 8693 token exchange. Users can list and revoke these bindings at any time.

This is distinct from OAuth **consents** (`/settings/consents` in the web UI, `references/admin.md`): a consent is "I let this OAuth client authenticate as me"; a binding is "this client holds a server-side credential handle for me so it can act on my behalf via NyxID without ever holding my refresh_token". The two surfaces are adjacent in the user's settings.

## When the user might ask about this

- "What apps have access to my account?" → list both consents AND broker bindings; they're separate views.
- "Revoke aevatar's access" → check both pages; an integration may have a consent (for sign-in) and a binding (for delegated capability) and the user usually wants both gone.
- "Why does revoking show no effect immediately?" → access tokens already issued from a binding stay valid until they expire (5 min by default for broker-issued tokens). Revocation prevents new tokens from being minted; in-flight ones expire on their own.

## CLI

```bash
nyxid oauth bindings list                         # list user's active broker bindings
nyxid oauth bindings list --output json           # machine-readable

nyxid oauth bindings show <hash>                  # full SHA-256 hex or any unique 8+ char prefix
nyxid oauth bindings show abc12345 --output json

nyxid oauth bindings revoke <hash>                # prompts for confirmation
nyxid oauth bindings revoke <hash> --yes          # skip confirmation
```

The `<hash>` argument is the binding's SHA-256 hex (visible as `binding_hash` in `list` output). The raw `binding_id` (the `bnd_...` value) is only ever known to the holding OAuth client — the user never sees it. Always work from the hash on the user side.

Prefix matching follows the same UX as `git log <sha>` / `kubectl get pod <prefix>`: 8+ chars required, ambiguous prefix errors with "ambiguous prefix; provide more characters".

## Web UI

Same surface lives at `/settings/authorizations` (sidebar: **Authorizations**). Lists each active binding with its application, external account (if any — e.g. `lark · tenant_x · u_xxx` for a Lark-bound binding), scopes, created / last-used dates, and a revoke button. Sibling to **Authorized Apps** (`/settings/consents`).

## What revocation cascades to

- The targeted binding is marked revoked.
- The underlying NyxID refresh_token tied to that binding is also revoked (so the broker client can no longer mint access tokens from it).
- Other bindings the user holds — even with the same OAuth client — are NOT touched. Explicit revoke is per-binding.
- Reuse-detection (the broker client tries to use a stale refresh_token) DOES cascade-revoke all bindings for that `(client_id, user_id)` pair. That's a security signal, not a user action.

## Common error responses (RFC-aligned)

- `200 OK` on `/oauth/revoke` even for nonexistent / wrong-owner / already-revoked tokens (RFC 7009 §2.2 — never reveal whether a binding existed).
- `404 Not Found` on user-facing `DELETE /api/v1/users/me/broker-bindings/{hash}` if the binding doesn't belong to the current user. Same status whether it doesn't exist or belongs to someone else.
- `400 Bad Request` on `GET /oauth/bindings` (reverse-lookup) with no external-subject criteria — that endpoint is filtered-by-criteria, not list-all.

## Related skills

- For OAuth consent / "Authorized Apps", see `references/admin.md`.
- For end users granting / linking external accounts (Lark, Telegram, Discord) into agent integrations, the binding is created by the agent platform itself during its OAuth flow — the user just sees the result on `/settings/authorizations` after consenting.
