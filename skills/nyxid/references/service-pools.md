# Service Pools

Service pools let a user or org expose one stable proxy slug for several interchangeable configured services. They are for capacity and redundancy, not for mixing unrelated APIs: every member should accept the same downstream paths, methods, headers, and request body shapes.

## Proxying

Call a pool slug the same way you call a service slug:

```bash
nyxid proxy request <pool_slug> <path> -m <METHOD> -d '<body>'
```

Raw HTTP clients use the normal slug route:

```text
/api/v1/proxy/s/{pool_slug}/{path}
```

NyxID authenticates the caller, resolves the slug in the caller's personal or org scope, selects a viable active member service, injects that member's stored credential, and forwards the request to the selected downstream endpoint. The caller does not see member credentials.

## Resolution Rules

- Prefer a direct service slug when one exists. A normal active `UserService` with the requested slug wins before pool resolution.
- If no direct service resolves, NyxID checks for an active `ServicePool` with that slug in the same owner scope.
- Only enabled pool members whose target `UserService` is active and belongs to the same owner scope are viable.
- If the pool slug exists but has no viable member, the proxy returns a service-pool error instead of falling back to an unrelated service.

## Agent Guidance

- Discover configured services before assuming a slug exists. If the user says a slug is a pool, treat it as a proxy target and call it through `nyxid proxy request` or `/api/v1/proxy/s/{pool_slug}/{path}`.
- Keep downstream API paths relative to the selected members' base URLs, just as with direct services.
- Do not depend on a specific member being selected for a given call. A pool represents interchangeable capacity.
- Do not document or script pool-management commands from this reference yet. The `nyxid pool ...` command surface, member/weight workflow, and strategy details are intentionally outside this shipped-proxy reference until their CLI contract is settled.
