# Service Pool Routing Proof

Status: design proof and implementation plan for NyxID#974. This document is
not a user-visible feature spec and does not introduce a `ServicePool` API.

## Governing Practice

The established pattern for service discovery and load balancing is a single
authoritative routing control plane: a logical service name resolves to a
bounded set of concrete targets, a policy chooses one target, health/failover
filters unsafe targets, and the proxy data plane consumes the chosen target.
Routing truth must not be split across competing registries.

NyxID's current authoritative boundary is the proxy target resolver:

- `proxy_service::resolve_proxy_target_from_user_service()` resolves a logical
  service slug or catalog service id to one concrete `UserServiceResolution`.
- `finish_resolution()` turns that `UserService` into one `ProxyTarget`,
  including the endpoint URL, credential, identity propagation settings,
  default headers, WebSocket frame injection rules, and optional `node_id`.
- `node_routing_service::resolve_node_route()` runs below that boundary. It
  chooses a node route for an already selected catalog service; it does not
  choose among endpoint/credential/service members.

## Insufficiency Proof

The existing `resolve_node_route()` / `fallback_node_ids` contract cannot model
identical `UserService` instance balancing by itself.

1. `user_services` has an active unique index on `("user_id", "slug")`.
   A stable user-facing slug can name exactly one active `UserService` for an
   owner, so multiple active members cannot share the same slug.
2. `UserService` stores exactly one `endpoint_id` and at most one `api_key_id`.
   It has no member set, strategy, weight, capacity signal, or pool-level
   health state.
3. `NodeServiceBinding` stores `node_id`, `user_id`, `service_id`, and
   `priority`. It can order node candidates for one catalog service, but it
   cannot represent different endpoint URLs, credentials, per-member weights,
   or direct HTTP members.
4. `resolve_node_route()` is intentionally node-specific. It filters `Node`
   records by DB status, WebSocket connectivity, and node metrics, then returns
   `NodeRoute { node_id, fallback_node_ids }`. Direct services without a node
   never enter that health model.
5. `resolve_proxy_target_from_user_service()` relies on one selected
   `UserService` to preserve personal-before-legacy-before-org precedence,
   approval ownership, API-key service scope, and org membership scope. Simply
   relaxing the slug uniqueness constraint would make those semantics
   ambiguous instead of adding a safe balancing policy.

Conclusion: the existing routing layer is the correct control-plane boundary,
but its node-only failover contract is insufficient for balancing multiple
identical service instances. A future `ServicePool` model is justified only as
an extension of the proxy target resolver, not as a parallel routing system.

## Implementation Plan

When the product surface is approved separately, implement pools at the proxy
target-resolution boundary and keep `UserService` as the concrete member type.

1. Add a backend `ServicePool` model:
   - `id`, `user_id`, `slug`, `strategy`, `is_active`, `created_at`,
     `updated_at`.
   - The pool owns the stable slug. Active pool slugs and active
     `UserService.slug` values must be mutually exclusive per owner.
   - Strategies should start with `round_robin` and `weighted_round_robin`.
2. Add a backend `ServicePoolMember` model:
   - `id`, `pool_id`, `user_service_id`, `weight`, `is_active`,
     `created_at`, `updated_at`.
   - The member points at an existing `UserService`, preserving endpoint,
     credential, node, identity propagation, and header behavior.
   - Member ownership must match the pool owner.
3. Extend `resolve_proxy_target_from_user_service()`:
   - Keep the existing personal, legacy, and org precedence order.
   - For each owner scope, look up a direct `UserService` and a `ServicePool`
     by the same slug space. Reject conflicting active rows during writes.
   - If a pool matches, select a member and call the existing
     `finish_resolution()` on that member. This keeps credential decryption,
     approval hints, node routing, and audit metadata on the established path.
4. Use MongoDB for policy state:
   - `round_robin` and weighted selection must be safe across multiple NyxID
     API instances, so the cursor/counter cannot live only in process memory.
   - The selection attempt count must be bounded by the active member count.
5. Reuse node health underneath selected members:
   - If a member is node-routed, call the existing node viability checks and
     fall back to another member when no viable node route exists.
   - If a member is direct HTTP, start with passive health from proxy outcomes
     before adding active probes or a standardized status endpoint.
6. Add management surfaces only after the model and resolver are implemented:
   - Backend REST handlers.
   - `nyxid pool` CLI commands.
   - Frontend pool management UI.
   - NyxID skill documentation.

## Non-Goals For This Proof

- No quota, usage counting, or metering across pool members.
- No sticky routing or session affinity.
- No new error-code allocation.
- No CLI, frontend, or skill surface before the backend contract is approved.
