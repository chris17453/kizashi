# 0043. Tenant isolation audit fixes and session cookie hardening

## Context

As part of the ongoing push toward an enterprise/audit-ready compliance bar (spec §8
Multi-Tenancy & Security), a targeted tenant-isolation audit was run across the read/write paths
added or touched so far this session. It surfaced two real cross-tenant data-isolation defects,
and a direct inspection of session-cookie handling in the Console UI (`ui/`) surfaced a missing
`Secure` attribute on every cookie the service sets — a standard OWASP session-management
control.

### Finding 1 — `auth-service` tenant branding write has no tenant check

`PUT /v1/tenants/:id/branding` (added in ADR-0041, this session) checked the caller's role
(admin) and recorded a real actor for the audit log, but never checked that the caller's own
tenant matched the `:id` in the path. Any authenticated Admin — from any tenant — could overwrite
any other tenant's product name, logo URL, or accent color.

### Finding 2 — `trigger-engine` trigger read has no tenant check

`GET /v1/triggers/:id` had no tenant scoping at all. Any caller who could reach the service (in
practice: Action Executor's service identity, but nothing prevented any other caller) could read
any tenant's full `TriggerDefinition` — including its condition DSL and action targets, which can
contain webhook URLs and email addresses — just by guessing or enumerating a UUID. The sibling
`POST /v1/triggers/:id/test` handler in the same file already enforced this correctly
(`if trigger.tenant_id != tenant_id { 404 }`); `get_trigger` was simply missed when it was added.

### Finding 3 — Console UI session cookies never set `Secure`

None of the three cookie-setting call sites in `ui/` (`login_handler.rs`, `sso_login_handler.rs`,
`logout_handler.rs`) set the `Secure` attribute, meaning a session cookie could be sent over a
plaintext HTTP connection if one ever existed between a client and a non-TLS-terminated deploy of
the UI.

## Decision

**Tenant checks.** Both endpoints now require and validate an `X-Tenant-Id` header against the
resource's own `tenant_id`, following the pattern already established elsewhere in the codebase:

- `trigger-engine`'s `get_trigger` returns `404 Not Found` on a tenant mismatch (not `403`),
  matching `test_trigger`'s existing convention — a 404 doesn't let a caller distinguish "wrong
  tenant" from "doesn't exist," which would otherwise let a caller enumerate other tenants' valid
  trigger ids via response-code side channel.
- `auth-service`'s `put_branding` returns `403 Forbidden` on a tenant mismatch instead, since this
  is an explicit admin write action reached only after an authenticated session already knows its
  own tenant — there's no enumerable-lookup concern to hide behind a 404 here, and a 403 is more
  informative for an admin's own tooling/logs.

`action-executor`'s `TriggerClient` trait gained a `tenant_id: Uuid` parameter (the firing
event's own tenant) so `HttpTriggerClient` can send `X-Tenant-Id` on its trigger lookups — the
only caller of trigger-engine's `GET /v1/triggers/:id` in production, and now compliant with the
new requirement.

**Cookie hardening.** A new `ui/src/cookie_security.rs` module reads a `COOKIE_SECURE` env var
(default `false`) and appends `; Secure` to a cookie string when set to `"true"`. Default is
`false`, not `true`, because local/dev environments run the Console UI over plain HTTP and a
`Secure` cookie is silently dropped by browsers on non-HTTPS connections — defaulting to `true`
would break every local login. Production deployments behind a real TLS-terminating
load balancer/ingress must set `COOKIE_SECURE=true` explicitly.

## Consequences

- Any other read/write handler added in the future that takes a resource id in the path must
  include the same tenant-scoping check — this class of bug (a handler added correctly overall,
  but missing the tenant check that its sibling handler already had) is exactly the kind of gap a
  periodic isolation audit is meant to catch, not a one-time fix.
- `COOKIE_SECURE=true` must be set as part of any real deployment runbook/Helm values once
  Phase 2 (deployability) ships — tracked as an operational requirement, not automatically
  inferred from environment.
- No schema changes; both tenant-check fixes are handler-logic-only. The cookie change is
  additive (a new module + one line changed at each of three call sites).
