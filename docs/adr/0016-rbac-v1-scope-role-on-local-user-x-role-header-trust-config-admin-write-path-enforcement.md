# ADR-0016: RBAC v1 scope — role on local user, X-Role header trust, config-admin-service write-path enforcement

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §8 calls for "OpenShift-project-style" per-tenant RBAC; the gap-closing roadmap flags it as
the highest-priority security/compliance gap. Today every service trusts `X-Tenant-Id` (or a
resolved-from-bearer-token equivalent) with **zero role/permission check** — confirmed by
inspection of `config-admin-service`'s and `retention-service`'s handlers, which only compare
`X-Tenant-Id` against an entity's own `tenant_id`. Any authenticated session, regardless of who
it belongs to, can create/update/delete triggers, mappings, agents, retention policies, and API
keys.

The identity model already distinguishes a user from a tenant: `auth_service.local_users` has a
real per-user row (id, tenant_id, username, password_hash) — this is not "add a users concept
from scratch," it's "add a role to the user that already exists" plus threading that role to
every place identity currently flows:

```
local_login_handler → SessionClient::mint_session → query-gateway's /internal/tokens
  → TokenStore/query_api_tokens → LoginResponse → Console UI's Session
  → Console UI's HTTP clients (agents_client, triggers_client, api_keys_client, ...)
  → config-admin-service / retention-service / ingestion-gateway write-path handlers
```

query-gateway's `proxy_get` only fronts Dashboard API's *read* path (events/reports) — the
write-path services (config-admin-service, retention-service, ingestion-gateway's API key
endpoints) are called **directly** by Console UI's backend with a trusted `X-Tenant-Id` header,
no gateway in front (ADR-0010). That means role enforcement for writes can't live at a gateway
layer the way tenant scoping does — it has to be a header Console UI forwards (`X-Role`,
alongside `X-Tenant-Id`) and each write-path service checks, the same trust boundary already
established for tenant identity.

Doing the full scope in one PR — every write path (config-admin-service, retention-service,
action-executor, ingestion-gateway's API keys) plus a full RBAC admin UI (assign roles to other
users, not just view your own) — is a large, multi-service, multi-PR effort. Per CLAUDE.md §0.4
("make the smallest safe, reversible, well-documented choice... keep moving"), this ADR scopes a
v1 slice that is genuinely useful and sets the pattern every later increment repeats, rather than
attempting the whole surface at once.

## Decision

**Role model:** `common::Role` — `Viewer < Operator < Admin`, ordered (`Role::at_least(min)`).
`Viewer` can read; `Operator` and `Admin` can write (create/update/delete config entities);
`Admin` is reserved for future role-assignment/administration actions. Stored as a single column
on `auth_service.local_users` (not a separate `role_assignments` join table) — v1 is one role per
user per tenant, matching "per tenant" from spec §8; a join table is the natural extension if
multi-role-per-user is ever needed, deferred until there's a real requirement for it.

**Threading:** `role` flows end-to-end through the chain above: `LocalUser.role` →
`SessionClient::mint_session(tenant_id, role, label)` → `query-gateway`'s `/internal/tokens`
request/`TokenStore::mint_token` (stored alongside the token hash so a later `tenant_for_token`
lookup — renamed `session_for_token`, returning `(tenant_id, role)` — can recover it) →
`LocalLoginRequest`'s response (`LoginResponse` gains `role`) → Console UI's `Session` struct →
forwarded as an `X-Role` header by Console UI's write-path HTTP clients, exactly like
`X-Tenant-Id` already is.

**v1 enforcement scope:** `config-admin-service`'s trigger-definition and normalization-mapping
write handlers (`create`/`update`) reject a role below `Operator` with 403. This is the write
path with the most existing surface area (two entity types, already fully CRUD'd) and proves the
pattern end-to-end. `retention-service`, `action-executor`'s trigger CRUD, and
`ingestion-gateway`'s API-key create/revoke are **not** gated in this PR — same shape of change,
explicitly deferred as immediate follow-up work (tracked, not silently dropped) once the pattern
from this PR is proven live.

**Console UI v1 scope:** nav hides the two admin-only entry points a `Viewer` shouldn't act on
(nothing currently gated needs hiding beyond what's shipped in this PR — a fuller nav-hiding pass
follows as more write paths get gated). A dedicated "assign role to another user" admin page is
explicitly **out of scope for v1** — every current user manages only their own role's visibility;
role assignment (`Admin` changing someone else's role) needs its own multi-user-per-tenant
UI/endpoint work, deferred as a separate follow-up. The existing demo user is seeded as `Admin`
so today's demo flow is unaffected by this change.

## Consequences

- Easier: the identity model already has a distinct user row to hang a role on, so this is a
  column-plus-checks change, not a new concept. The pattern (role column → mint_session param →
  token-store column → forwarded header → handler check) is now proven and mechanical to repeat
  for `retention-service`/`action-executor`/`ingestion-gateway` in follow-up PRs.
- Harder (at the time this ADR was written): until follow-up PRs landed, `retention-service` and
  `ingestion-gateway`'s API key endpoints remained unenforced — a `Viewer` could create/revoke
  API keys or edit retention policies. This is a real, acknowledged gap, not silently glossed
  over; it's the direct cost of shipping incrementally rather than blocking this PR on covering
  every write path at once. **Update:** this follow-up has since landed — `retention-service`'s
  `policy_handlers.rs` and `ingestion-gateway`'s `api_key_handlers.rs` both call `require_
  operator` on every write path today (fix/0003, and ingestion-gateway's own follow-up). `query-
  gateway`'s read path (`proxy_get` → Dashboard API) is still not role-gated in v1 — reads are
  lower-risk than writes and every authenticated user can already read within their own tenant,
  so this remains a smaller, still-open gap than the write-path one was.
- Reassigning another user's role had no UI at the time this ADR was written — that was a direct
  SQL update against `auth_service.local_users`, same as API keys were before Phase 1c's UI
  shipped. **Update:** this follow-up has since landed too — see the `/users` page
  (feature/0030-user-management-role-assignment, ADR-0016 follow-up) and its last-admin
  protection (feature/0031-last-admin-protection).
