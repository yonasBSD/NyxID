# Agent Isolation Handoff

This document is a reviewer-facing summary of the agent-isolation work that is
currently implemented, the issues that were identified during diff review, and
the follow-up changes that were made to address them.

Use this together with [AGENT_ISOLATION.md](./AGENT_ISOLATION.md) when doing a
fresh review.

## Purpose

This is not the design doc. It is a handoff note for another reviewer so they
can quickly answer:

- What was originally implemented
- What was found broken or incomplete
- What was changed to fix those gaps
- What is still intentionally not done
- Which files and commands are worth checking first

## Original Review Findings

The first review found six material issues:

1. `nyxid ai-setup agent` commands did not match the backend response shapes.
   The CLI expected fields like `key` and envelopes like `api_keys`, while the
   backend actually returned `full_key`, `keys`, `services`, and `bindings`.

2. The bindings UI and bindings API did not agree on payload shape.
   The frontend expected `service_label`, `service_slug`, and
   `credential_label`, but the backend returned only IDs.

3. Phase `7a` was overstated as complete.
   The API key detail page did not yet show usage stats, and the API key table
   rendered `bindings_count` even though the backend did not expose it.

4. Platform handling was inconsistent across backend and frontend.
   The frontend offered unsupported platform values, omitted `cursor`, and
   could not clear an existing platform cleanly.

5. The per-agent rate limiter did not implement the documented semantics.
   The previous version effectively treated burst as the steady one-second cap.

6. Admin audit filtering and observability phases were still incomplete.
   The backend stored `api_key_id` and `api_key_name` on audit records, but the
   admin audit API/UI did not expose or filter by them.

## What Was Fixed

### 1. CLI / API contract alignment

Fixed in:

- `cli/src/commands/ai_setup.rs`

What changed:

- Added shared response-envelope handling for `keys`, `services`, `bindings`,
  and `api_keys`
- Switched agent create / rotate to use `full_key`
- Fixed `agent list`, `agent show`, `agent bind`, and lookup-by-name paths to
  read the correct JSON envelope shapes
- Updated the table output for bindings to use enriched service/credential
  fields when available

Effect:

- `nyxid ai-setup agent create`
- `nyxid ai-setup agent list`
- `nyxid ai-setup agent show`
- `nyxid ai-setup agent bind`
- `nyxid ai-setup agent rotate`
- `nyxid ai-setup agent delete`

now operate against the current backend API shape instead of stale assumptions.

### 2. Enriched binding responses

Fixed in:

- `backend/src/handlers/agent_bindings.rs`
- `backend/src/services/agent_binding_service.rs`
- `frontend/src/components/dashboard/api-key-detail/bindings-card.tsx`
- `frontend/src/hooks/use-keys.ts`
- `frontend/src/types/keys.ts`

What changed:

- The backend now enriches binding responses with:
  - `service_slug`
  - `service_label`
  - `credential_label`
- The frontend bindings card now loads actual external credentials from
  `/api-keys/external` instead of incorrectly reusing `/keys`

Effect:

- The bindings list is meaningful in the UI
- Creating a binding uses a real external credential ID
- CLI and frontend now consume the same enriched response shape

### 3. API key response enrichment and usage API

Fixed in:

- `backend/src/handlers/api_keys.rs`
- `backend/src/routes.rs`
- `backend/src/db.rs`
- `frontend/src/hooks/use-api-keys.ts`
- `frontend/src/types/api.ts`

What changed:

- Added `bindings_count` to API key responses
- Added usage endpoints:
  - `GET /api/v1/api-keys/usage`
  - `GET /api/v1/api-keys/{id}/usage`
- Added audit-log index on `api_key_id` for usage/admin filtering queries

Usage payload includes:

- request count
- success count
- error count
- error rate
- last used timestamp
- top services
- 7-day activity buckets

Effect:

- The API key table can render real binding counts
- The detail page and overview page can render agent-level usage without
  inventing a second metrics store

### 4. Frontend observability additions

Fixed in:

- `frontend/src/pages/api-key-detail.tsx`
- `frontend/src/components/dashboard/api-key-detail/usage-stats-card.tsx`
- `frontend/src/pages/keys.tsx`
- `frontend/src/components/dashboard/api-key-usage-dashboard.tsx`

What changed:

- The API key detail page now includes an agent usage card
- The `/keys` page now includes an agent usage dashboard with:
  - request counts
  - error counts / error rate
  - top services
  - simple recent activity bars

Effect:

- Phase `7a` is materially complete for usage visibility
- Phase `7b` is now implemented for request/error/top-service activity,
  provider-reported token totals, and reported cost when the upstream includes
  a cost field

### 5. Platform handling alignment

Fixed in:

- `backend/src/services/key_service.rs`
- `backend/src/handlers/api_keys.rs`
- `frontend/src/schemas/agent-bindings.ts`
- `frontend/src/schemas/agent-bindings.test.ts`
- `frontend/src/components/dashboard/api-key-detail/platform-card.tsx`

What changed:

- Backend update flow now supports explicit platform clearing via nullable
  update semantics
- Frontend options now align with backend-supported values:
  - `claude-code`
  - `cursor`
  - `codex`
  - `openclaw`
  - `generic`
- Frontend now uses a sentinel value for "None" so clearing an existing
  platform actually works

Effect:

- Platform editing is consistent across backend, CLI, and frontend

### 6. Per-agent rate limiter semantics

Fixed in:

- `backend/src/mw/rate_limit.rs`
- `backend/src/main.rs`

What changed:

- Replaced the simplistic one-second counter with a per-agent token bucket
- `rate_limit_per_second` now controls refill rate
- `rate_limit_burst` now controls bucket capacity
- Added a focused test for burst behavior:
  - `per_agent_uses_burst_without_turning_it_into_sustained_limit`

Effect:

- Burst no longer becomes the steady-state allowed throughput
- Behavior is now much closer to the documented intent

### 7. Admin audit filtering

Fixed in:

- `backend/src/handlers/admin.rs`
- `frontend/src/hooks/use-admin.ts`
- `frontend/src/pages/admin-audit-log.tsx`
- `frontend/src/router.tsx`
- `frontend/src/pages/lazy.ts`
- `frontend/src/components/dashboard/sidebar.tsx`
- `frontend/src/types/admin.ts`

What changed:

- Admin audit endpoint now supports `api_key_id` query filtering
- Audit log items now expose:
  - `api_key_id`
  - `api_key_name`
- Added a new admin audit log page and route:
  - `/admin/audit-log`
- Added sidebar navigation entry for the admin audit log

Effect:

- Phase `7c` is no longer backend-only data without an admin UI

## Current Status By Phase

### Phase 1: Agent identity propagation

Status: Implemented

Evidence:

- `backend/src/mw/auth.rs`
- `backend/src/models/audit_log.rs`
- `backend/src/services/audit_service.rs`
- `backend/src/handlers/proxy.rs`
- `backend/src/handlers/llm_gateway.rs`

Notes:

- `AuthUser` carries `api_key_id`, `api_key_name`,
  `rate_limit_per_second`, and `rate_limit_burst`
- Proxy and LLM paths include `X-NyxID-Agent-Id` on responses
- Audit events for proxy/LLM traffic include agent identity

### Phase 2: Per-agent credential override

Status: Implemented

Evidence:

- `backend/src/models/agent_service_binding.rs`
- `backend/src/services/agent_binding_service.rs`
- `backend/src/services/proxy_service.rs`
- `backend/src/handlers/agent_bindings.rs`

Notes:

- Bindings are stored in `agent_service_bindings`
- Proxy resolution consults per-agent override first, then falls back to the
  default service credential

### Phase 3: CLI profile system

Status: Implemented

Evidence:

- `cli/src/auth.rs`
- `cli/src/cli.rs`
- `cli/src/main.rs`

Notes:

- `--profile` and `NYXID_PROFILE` are supported
- Token/base-url storage is profile-aware
- Profile names are validated

### Phase 4: Node multi-instance

Status: Implemented

Evidence:

- `cli/src/node/config.rs`
- `cli/src/node/daemon.rs`
- `cli/src/commands/node.rs`

Notes:

- Profile-aware service labels and config directories are implemented

### Phase 5: Platform integration

Status: Implemented after CLI contract fixes

Evidence:

- `cli/src/commands/ai_setup.rs`
- `cli/src/cli.rs`
- `backend/src/models/api_key.rs`
- `backend/src/services/key_service.rs`

Notes:

- The major issue here was not missing surface area but broken response parsing
  in the CLI, which is now fixed

### Phase 6: Per-agent rate limiting

Status: Implemented after semantics fix

Evidence:

- `backend/src/mw/rate_limit.rs`

Notes:

- Implementation now uses token-bucket semantics instead of a flat one-second
  request counter

### Phase 7a: API key detail page

Status: Implemented

Evidence:

- `frontend/src/pages/api-key-detail.tsx`
- `frontend/src/components/dashboard/api-key-detail/*`

Includes:

- platform selector
- rate limit editor
- credential bindings CRUD
- usage stats card
- binding count support in API responses / table

### Phase 7b: Agent usage dashboard

Status: Implemented

Evidence:

- `frontend/src/pages/keys.tsx`
- `frontend/src/components/dashboard/api-key-usage-dashboard.tsx`
- `backend/src/handlers/api_keys.rs`
- `backend/src/services/llm_usage_service.rs`
- `backend/src/handlers/llm_gateway.rs`
- `backend/src/handlers/proxy.rs`

Implemented:

- per-agent request counts
- error rates
- top services
- recent activity buckets
- provider-reported prompt/completion/total token totals
- provider-reported cost when the upstream response includes a cost field

### Phase 7c: Admin audit filtering

Status: Implemented

Evidence:

- `backend/src/handlers/admin.rs`
- `frontend/src/pages/admin-audit-log.tsx`

## Remaining Gap

No structural Phase `7` gap remains.

Operational caveat:

- Reported cost depends on what the upstream provider returns. Token totals are
  available whenever the provider includes usage data; reported cost will only
  appear for providers/responses that expose a cost field.

## Recommended Review Focus

If another agent is reviewing this work, these are the best places to start:

### Backend

- `backend/src/handlers/api_keys.rs`
- `backend/src/handlers/agent_bindings.rs`
- `backend/src/handlers/admin.rs`
- `backend/src/services/llm_usage_service.rs`
- `backend/src/mw/rate_limit.rs`
- `backend/src/handlers/proxy.rs`
- `backend/src/handlers/llm_gateway.rs`

Questions to ask:

- Are the new usage aggregations indexed and shaped correctly?
- Is provider-reported usage captured consistently across both `/llm` and
  `/proxy` paths?
- Are audit events consistently attributed for both success and denial paths?
- Are nullable update semantics for `platform` correct and safe?
- Is the token-bucket implementation acceptable for current load?

### CLI

- `cli/src/commands/ai_setup.rs`

Questions to ask:

- Do all agent commands now consume the real API envelopes?
- Is the CLI output still coherent for both JSON and table modes?

### Frontend

- `frontend/src/pages/api-key-detail.tsx`
- `frontend/src/components/dashboard/api-key-detail/bindings-card.tsx`
- `frontend/src/components/dashboard/api-key-usage-dashboard.tsx`
- `frontend/src/pages/admin-audit-log.tsx`

Questions to ask:

- Does the UI match the backend payloads without hidden assumptions?
- Are the new usage views sufficient for phase goals?
- Is the new admin audit route integrated correctly?

## Validation Run

Commands run after the fixes:

```bash
cargo fmt --all
cargo check -p nyxid
cargo check -p nyxid-cli
cargo test -q -p nyxid extracts_usage_from_openai_style_payload
cargo test -q -p nyxid per_agent_uses_burst_without_turning_it_into_sustained_limit
cargo test -q -p nyxid api_key_auth_includes_key_identity
cargo test -q -p nyxid-cli profile_name_validation_accepts_valid

cd frontend
npm run build
npm run lint
npm test -- --run agent-bindings.test.ts
```

Results:

- Backend compile: passed
- CLI compile: passed
- Targeted backend tests: passed
- Targeted CLI test: passed
- Frontend production build: passed
- Frontend schema test: passed
- Frontend lint: warnings only, no errors

## Notes For The Next Reviewer

- This branch already had many unrelated modified files before the follow-up
  fixes. Review should focus on the files listed above rather than trying to
  infer intent from the full repository diff in one pass.
- The biggest risk area is not low-level code correctness; it is contract drift
  between backend, CLI, and frontend. That is where the original issues were.
- For Phase `7b`, the main review question is whether provider-reported usage
  extraction is robust enough across both `/llm` and `/proxy` entry points.
