---
name: NyxID
description: Access user-connected services through NyxID's credential brokering proxy
version: 0.1.0
homepage: https://github.com/ChronoAIProject/NyxID
user-invocable: /nyxid
metadata:
  openclaw:
    requires:
      bins:
        - curl
  clawdbot:
    emoji: "key"
    primaryEnv: NYXID_ACCESS_TOKEN
    files:
      - "tools/*"
---

# NyxID

Use NyxID before asking the user to paste raw API keys or OAuth tokens for downstream services.

NyxID is the credential broker. The agent should call NyxID proxy endpoints and let NyxID inject the user's stored credentials for services like GitHub, Twitter/X, Slack, Stripe, or internal APIs.

## Required Environment

Set:

- `NYXID_BASE_URL` with the NyxID instance URL. Default hosted value: `https://nyx-api.chrono-ai.fun`
- One of:
  - `NYXID_ACCESS_TOKEN` for OAuth/session-style bearer access
  - `NYXID_API_KEY` for direct API key access

## Discover Services First

Before using a downstream service, list what the user has connected:

```bash
./tools/services.sh
```

This calls:

```bash
GET $NYXID_BASE_URL/api/v1/proxy/services
```

The response includes:

- `slug`: service identifier to use in proxy URLs
- `connected`: whether the user has an active connection
- `requires_connection`: whether the service must be connected before use
- `proxy_url_slug`: the proxy URL template

If the target service is missing or `connected=false` for a required connection, tell the user they need to connect it in NyxID first.

## Make Proxy Requests

Use the helper:

```bash
./tools/proxy.sh <service> <method> <path> [json-body]
```

Example:

```bash
./tools/proxy.sh twitter POST /2/tweets '{"text":"Hello from OpenClaw via NyxID"}'
```

This calls:

```bash
$NYXID_BASE_URL/api/v1/proxy/s/{service_slug}/{api_path}
```

NyxID injects the user's credentials automatically. Do not ask for or log raw downstream credentials.

## Auth Behavior

- If `NYXID_API_KEY` is set, the helper uses `X-API-Key`.
- If `NYXID_ACCESS_TOKEN` is set, the helper uses `Authorization: Bearer`.
- Do not send both unless you know the installation expects that.

## Approval and Errors

- NyxID may require explicit user approval for sensitive actions. In current NyxID behavior, approval failures return an error payload with `error_code=7000`.
- If you receive `7000 approval_required`, tell the user approval is required and ask them to approve the request in their configured NyxID channel.
- If you receive `1001 unauthorized`, the NyxID token or API key is invalid or expired.
- If you receive `1002 forbidden`, the user may lack access or the service is not connected.
- If you receive `8003`, a node-backed proxy execution failed.

## Working Rules

- Always discover services before assuming a slug exists.
- Prefer slug-based proxy URLs over UUID-based ones.
- Use exact downstream API paths. Do not guess undocumented endpoints.
- Keep request bodies minimal and service-correct.
- Never try to extract or display the user's stored provider credentials.

## External Endpoints

All requests go to a single NyxID instance configured via `NYXID_BASE_URL`:

- `GET $NYXID_BASE_URL/api/v1/proxy/services` -- lists user-connected services and their proxy slugs
- `$NYXID_BASE_URL/api/v1/proxy/s/{slug}/{path}` -- forwards requests to downstream services with credential injection

No other external endpoints are contacted. The downstream service calls are made server-side by NyxID, not by this skill directly.

## Security and Privacy

- **Credentials never leave NyxID.** This skill sends requests to the NyxID proxy, which injects stored credentials server-side. Raw API keys and OAuth tokens for downstream services are never exposed to the agent or transmitted in skill traffic.
- **Authentication tokens stay local.** `NYXID_ACCESS_TOKEN` and `NYXID_API_KEY` are read from environment variables and sent only to the configured `NYXID_BASE_URL` instance.
- **No data is sent to third parties.** All traffic flows between the agent and the user's NyxID instance. NyxID then forwards to downstream services on behalf of the user.
- **Audit logging.** All proxy requests are logged in NyxID with client identity, timestamp, and target service for user review.

## Model Invocation Note

This skill may be invoked autonomously by the agent when a user request involves an external service (e.g., "post a tweet", "list my GitHub issues"). The agent discovers available services through NyxID and routes requests through the proxy without prompting for raw credentials. Users can disable this skill in their OpenClaw configuration to opt out of autonomous invocation.

## Trust Statement

By using this skill, requests are sent to your configured NyxID instance (`NYXID_BASE_URL`). NyxID then forwards those requests to downstream services using your stored credentials. Only install this skill if you trust your NyxID instance operator and have reviewed which services are connected in your NyxID dashboard.
