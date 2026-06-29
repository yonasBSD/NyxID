---
name: aevatar-scheduler
description: Create and manage cron schedules that fire an Aevatar service on a recurring basis, authenticated as the scope owner via NyxID — over the REST API. Use when a user wants to "schedule", "run on a cron", "set up a recurring run", "run every day/hour/Monday", "automate this service on a timer", "preview a cron", "pause/resume/disable a schedule", or "run it now". It builds the schedule against a published service (identity + endpoint + payload + serving revision), uses scope-owner NyxID auth (which requires the owner's NyxID broker binding), and covers preview, enable/disable, run-now, update, and delete. Publish the service first with the service-publisher skill.
version: "1.3"
metadata:
  category: plain
  tag:
    - aevatar
    - schedule
    - cron
    - recurring
    - automation
    - nyxid
    - timer
---

# Schedule an Aevatar service on a cron

You create a **schedule** that fires a published service on a cron expression,
authenticated as **you** (the scope owner) through NyxID. Publish the service first
(`aevatar-service-publisher`) — you need its identity, an endpoint, and the payload type.

## Bootstrap

```bash
BASE=https://aevatar-console-backend-api.aevatar.ai
TOK=$(tr -d '\n' < ~/.nyxid/access_token)        # or the agent's own NyxID bearer
scopeId=$(curl -s -H "Authorization: Bearer $TOK" "$BASE/api/studio/context" | jq -r .scopeId)
auth=(-H "Authorization: Bearer $TOK" -H "Content-Type: application/json")
```

> **`jq` is only for convenience** — any JSON reader works (replace `| jq -r .scopeId` with
> `| python3 -c 'import sys,json;print(json.load(sys.stdin)["scopeId"])'`). Make these calls with
> the **`curl` binary**, not Python's `urllib`/`requests` (a WAF may 403 those). Reminder: the
> `scopeOwnerNyxId` precondition below cannot be satisfied by a bare NyxID **CLI** token — it needs
> the owner's interactive **console** NyxID login (broker binding), or creation 400s.

## Gather the target (one call: the scope services list)

`GET /api/scopes/{scopeId}/services` returns everything you need per service — copy it off
the entry for your service:
```bash
curl -s "${auth[@]}" "$BASE/api/scopes/$scopeId/services" \
  | jq '.[] | {tenantId, appId, namespace, serviceId, defaultServingRevisionId, invokeReady,
               endpoints: [.endpoints[] | {endpointId, requestTypeUrl}]}'
```
- **identity** — the 4-tuple `{tenantId, appId, namespace, serviceId}`. For a workflow
  member the `serviceId` is `member-<memberId>`.
- **endpointId** + **payloadTypeUrl** — from `endpoints[]` (`payloadTypeUrl` = the
  endpoint's `requestTypeUrl`). A workflow member's default endpoint is `chat` with
  `type.googleapis.com/aevatar.ai.ChatRequestEvent`.
- **revisionId** — use the service's `defaultServingRevisionId`. **Required** whenever you
  send `payloadJson` (see below).
- **payloadJson** — the request body as a JSON **string** (or `payloadBase64` for a packed
  proto). For a chat endpoint, `{"prompt":"…"}` is accepted.
- Confirm `invokeReady` is `true` before scheduling — a schedule against a not-yet-serving
  service will fire into nothing.

## Preview the cron first (no clock guessing)

```bash
curl -s "${auth[@]}" -X POST "$BASE/api/schedules/preview" \
  -d '{"cronExpression":"0 9 * * 1-5","timezone":"Asia/Shanghai","count":"5"}' | jq .
```
Returns the next N fire times so you can confirm the expression means what the user wants.
Use a real IANA `timezone`; the engine has no implicit local time.

## Precondition: the scope owner needs a NyxID owner (broker) binding

A scheduled service fire happens *later*, after your current token has expired, so the
platform must be able to **re-mint** the scope owner's NyxID credential at fire time. That
requires the scope owner to have an **authenticated NyxID owner binding** (the
`urn:nyxid:scope:broker_binding` granted when you sign in through the Aevatar console /
studio NyxID login). A plain NyxID-CLI token is **not** sufficient: creating a
`scopeOwnerNyxId` schedule without that binding fails fast with

> HTTP 400 — "Authenticated NyxID owner binding is required for scope owner schedule auth;
> complete or refresh NyxID login before creating a scope owner schedule."

If you hit this, tell the user to complete/refresh their NyxID login in the Aevatar console
(to establish the broker binding), then retry — do not try to work around it.

## Create the schedule

```bash
curl -s "${auth[@]}" -X POST "$BASE/api/schedules" -d "{
  \"displayName\": \"Weekday 9am run\",
  \"cronExpression\": \"0 9 * * 1-5\",
  \"timezone\": \"Asia/Shanghai\",
  \"enabled\": true,
  \"serviceInvocation\": {
    \"identity\": { \"tenantId\": \"$scopeId\", \"appId\": \"default\", \"namespace\": \"default\", \"serviceId\": \"member-<memberId>\" },
    \"endpointId\": \"chat\",
    \"payloadTypeUrl\": \"type.googleapis.com/aevatar.ai.ChatRequestEvent\",
    \"payloadJson\": $(jq -nc '{prompt:"do the thing"} | tojson'),
    \"revisionId\": \"<defaultServingRevisionId>\",
    \"auth\": { \"scopeOwnerNyxId\": { \"scope\": \"proxy\" } }
  }
}"
```

`ScheduledDispatchConfigurationHttpRequest`: `cronExpression` (required); `displayName?`,
`timezone?`, `enabled` (default true), `headers?` (string map), and **exactly one** target:
`serviceInvocation` (above) or `envelope` (a raw actor `EventEnvelope` — advanced).

> **`payloadJson` requires `revisionId`.** If you supply `payloadJson` without a
> `revisionId` (and the service has no *active* serving revision), creation fails with
> 400 "payloadJson requires a revisionId; provide one explicitly or activate a serving
> revision." Pass the service's `defaultServingRevisionId`.

### Auth (`serviceInvocation.auth`)

- **`scopeOwnerNyxId: { scope }`** — fire as the scope **owner**, re-minting their NyxID at
  fire time. The right choice for owner-run schedules, but it requires the owner's broker
  binding (see **Precondition** above), otherwise creation 400s.
- **`senderNyxId: { subject: { platform, externalUserId, tenant? }, scope }`** — fire as a
  specific external subject. Only when the schedule must run as someone other than the
  owner, and that subject already has a durable NyxID binding — otherwise the fire fails at
  credential-mint time.

## Verify, then manage

```bash
sid=$(...)   # scheduleId from the create response
curl -s "${auth[@]}" "$BASE/api/schedules"            | jq '.[] | {scheduleId, displayName, cronExpression, enabled, nextFireUtc}'
curl -s "${auth[@]}" "$BASE/api/schedules/$sid"       | jq .
curl -s "${auth[@]}" -X POST "$BASE/api/schedules/$sid:run-now"   # fire once immediately to test
curl -s "${auth[@]}" -X POST "$BASE/api/schedules/$sid:disable"   # pause
curl -s "${auth[@]}" -X POST "$BASE/api/schedules/$sid:enable"    # resume
curl -s "${auth[@]}" -X PUT  "$BASE/api/schedules/$sid" -d '{ ...updated configuration... }'
curl -s "${auth[@]}" -X DELETE "$BASE/api/schedules/$sid"         # remove
```
Note the action verbs use a colon (`/{scheduleId}:run-now`), not a slash.

After `:run-now`, confirm the fire actually executed — check the service's runs
(`GET /api/scopes/{scopeId}/services/{serviceId}/runs`) or the observatory
(`GET /api/workflow/observatory/runs`). A 2xx on the schedule call means *accepted*, not
*succeeded*; a fire can still fail later at credential-mint or execution time, so read the
run back before reporting success.

## Next

- Need to (re)publish the target service? `aevatar-service-publisher`.
- Want the whole picture? `aevatar-platform-map`.

If you cannot complete a step server-side after a real attempt, hand the original request
back to your caller rather than fabricating — see the fallback skill in this family.
