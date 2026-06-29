---
name: aevatar-team-builder
description: Build an Aevatar agent team and its members over the REST API. Use when a user wants to "create a team", "add a member", "make a workflow member / script member / gagent member", "set the team's entry point", or "assemble agents into a team". It creates the team, creates members whose implementation is a workflow (most common), a script, or a hosted gagent, binds each member's concrete implementation (the workflow YAML is attached here), waits for the async binding to succeed, and sets the team entry member. Author the workflow YAML first with the workflow-authoring skill; publish the result as a service with the service-publisher skill.
version: "1.2"
metadata:
  category: plain
  tag:
    - aevatar
    - team
    - member
    - workflow
    - gagent
    - script
    - studio
    - create-team
---

# Build an Aevatar team and its members

You create a **team**, fill it with **members** (each backed by a workflow, script, or
gagent), bind their implementations, and set the team's entry member — all via REST. The
output is an invocable team. Publishing it as a NyxID service is a separate step
(`aevatar-service-publisher`); scheduling is another (`aevatar-scheduler`).

## Bootstrap

```bash
BASE=https://aevatar-console-backend-api.aevatar.ai
TOK=$(tr -d '\n' < ~/.nyxid/access_token)        # or the agent's own NyxID bearer
scopeId=$(curl -s -H "Authorization: Bearer $TOK" "$BASE/api/studio/context" | jq -r .scopeId)
auth=(-H "Authorization: Bearer $TOK" -H "Content-Type: application/json")
```

> **`jq` is only for convenience** — any JSON reader works (replace `| jq -r .scopeId` with
> `| python3 -c 'import sys,json;print(json.load(sys.stdin)["scopeId"])'`). Make these calls with
> the **`curl` binary**, not Python's `urllib`/`requests` (a WAF may 403 those). And because the
> create/bind calls are async and can occasionally return a **transient empty body**, always read
> the response status/JSON back — retry once on an empty body — rather than assuming success.

Member implementation kinds are the lowercase strings **`workflow`**, **`script`**,
**`gagent`**.

## Step 1 — Create the team

```bash
teamId=$(curl -s "${auth[@]}" -X POST "$BASE/api/scopes/$scopeId/teams" \
  -d '{"displayName":"My Team","description":"what it does"}' | jq -r '.teamId // .id')
```
`CreateStudioTeamRequest`: `displayName` (required), `description?`, `teamId?` (omit to
let the server mint one). Read the returned id back — do not invent it.

## Step 2 — Create the member shell

Create the member as a **shell**. Do **not** pass `implementationRef` here — the concrete
implementation (the workflow + its YAML) is attached in Step 3. Passing a forward
`workflowId` that does not exist yet returns **HTTP 500**.

```bash
wfId="my-workflow"   # the id you will bind in Step 3 (pick a stable kebab-case id)
memberId=$(curl -s "${auth[@]}" -X POST "$BASE/api/scopes/$scopeId/members" -d "{
  \"displayName\": \"My Workflow Member\",
  \"implementationKind\": \"workflow\",
  \"teamId\": \"$teamId\"
}" | jq -r '.memberId')
```
`CreateStudioMemberRequest`: `displayName` + `implementationKind` (required,
`workflow|script|gagent`); `description?`, `memberId?`, `teamId?` (attach now, or add
later via PATCH). The new member returns at `lifecycleStage:"created"` and is already
assigned a `publishedServiceId`; its `implementationRef` stays `null` until Step 3 fills
it in. (Verified: omitting `implementationRef` returns 201; sending it with a not-yet-bound
`workflowId` returns 500.)

- **script / gagent members** are created the same way — just set `implementationKind`
  to `"script"` or `"gagent"`. Discover valid gagent kinds with `GET /api/scopes/gagent-types`.
  The concrete `scriptId` / `agentKind` is supplied in the Step 3 binding, not here.

## Step 3 — Bind the member's implementation (attach the YAML)

This is where the real implementation lands. It starts an **async binding run**.

```bash
# Author the YAML first with aevatar-workflow-authoring; pass it inline.
runId=$(curl -s "${auth[@]}" -X PUT "$BASE/api/scopes/$scopeId/members/$memberId/binding" -d "{
  \"workflow\": { \"workflowId\": \"$wfId\", \"workflowYamls\": [ $(jq -Rs . < workflow.yaml) ] }
}" | jq -r '.bindingRunId')      # returns {status:"accepted", bindingRunId:"bind-...", ...}
```
`UpdateStudioMemberBindingRequest` carries exactly one of:
- `workflow`: `{workflowId, workflowYamls:[<yaml strings>]}`
- `script`:   `{scriptId, scriptRevision?}`
- `gAgent`:   `{agentKind, endpoints?}`

(`jq -Rs .` safely JSON-encodes the YAML file as a string.)

### Wait for the bind to succeed (it is asynchronous — typically ~1–2 minutes)

Poll the binding run **by its id** until `status` is `succeeded`:
```bash
curl -s "${auth[@]}" "$BASE/api/scopes/$scopeId/members/$memberId/binding-runs/$runId" \
  | jq '{status, failure}'
```
Status progresses `accepted → admission_pending → admitted → platform_binding_pending →
… → succeeded` (or `failed`/`rejected`). It commonly sits at `platform_binding_pending`
for a minute or two before flipping to `succeeded` — keep polling (e.g. every 5s, up to
~3 min). On `succeeded` the response carries `result.publishedServiceId` +
`result.revisionId`, and the member reaches `lifecycleStage:"bind_ready"`:
```bash
curl -s "${auth[@]}" "$BASE/api/scopes/$scopeId/members/$memberId" \
  | jq '{stage:.summary.lifecycleStage, svc:.summary.publishedServiceId, ref:.implementationRef}'
```
Do not report success on the 2xx from the PUT alone — that is only `accepted`; wait for the
run to reach `succeeded`.

## Step 4 — Set the team entry member

The entry member is the team's front door (what callers hit by default).

```bash
curl -s "${auth[@]}" -X PUT "$BASE/api/scopes/$scopeId/teams/$teamId/entry-member" \
  -d "{\"memberId\":\"$memberId\"}"
```
Add more members by repeating Steps 2–3 with the same `teamId`. List the roster:
`GET /api/scopes/$scopeId/teams/$teamId/members`.

## Verify

```bash
curl -s "${auth[@]}" "$BASE/api/scopes/$scopeId/teams/$teamId"          | jq .
curl -s "${auth[@]}" "$BASE/api/scopes/$scopeId/teams/$teamId/members"  | jq .
```
Confirm the team exists, the roster contains your member(s), and the entry member is set.

## Edit / clean up

- Rename: `PATCH /api/scopes/{scopeId}/teams/{teamId}` and `PATCH …/members/{memberId}`.
- Move a member into the team later: `PATCH …/members/{memberId}` with `{teamId}`.
- Archive a team: `POST …/teams/{teamId}/archive`.
- Re-bind (new YAML/revision): repeat Step 3; each bind is a new revision.

## Next

- **Publish it as a NyxID-registered service:** `aevatar-service-publisher`.
- **Run it on a schedule:** `aevatar-scheduler`.
- Lost? Load `aevatar-platform-map` for the full panorama.

If you genuinely cannot complete a step server-side, hand the original request back to
your caller rather than fabricating — see the fallback skill in this family.
