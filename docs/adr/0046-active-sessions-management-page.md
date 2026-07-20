# 0046. Active sessions management page

## Context

The Console UI's session layer (ADR-0014) is a simple in-memory map: `SessionStore` supports
`create`/`get`/`delete`, keyed by a random session id set as an `HttpOnly` cookie. There was no
way for an admin to see who currently has a live login, or to force one out — a standard
enterprise-security control ("show me every active session," "log this person out right now"
after an employee leaves or a credential is suspected compromised) that a compliance-minded buyer
expects as table stakes, alongside the audit trail added in ADR-0045.

## Decision

Extend `Session` with a `created_at` timestamp and `SessionStore` with a `list_for_tenant(tenant_id)
-> Vec<(session_id, Session)>` method. Add a new admin-only page, `GET /security/sessions`, listing
every active session for the caller's tenant (most-recently-created first), flagging which row is
the viewer's own current session, and a `POST /security/sessions/:id/revoke` action that deletes a
target session — but only after confirming (via the same `list_for_tenant` call) that it belongs to
the caller's own tenant, so an admin can't blind-guess another tenant's session id to force someone
else's user out. Both routes require `Admin` (matching `/users`' access bar, ADR-0016 follow-up):
seeing and terminating every session in the tenant, not just your own, is a step above ordinary
write access.

This stays entirely within the Console UI process and its existing in-memory store — no new
backend service, no schema, no cross-service call. `Session { .. }`'s two production construction
sites (`login_handler.rs`, `sso_login_handler.rs`) now stamp `created_at` at session creation.

## Consequences

- Session listing is single-instance-only, the same limitation ADR-0014 already accepted for the
  session store as a whole: a multi-replica Console UI deployment would only show sessions created
  on the instance handling the request, not a global view. This is an existing, documented
  constraint of the in-memory session design, not a new one introduced here — a shared session
  backend (Redis, or a `sessions` table) is the eventual fix and would make this page accurate
  across replicas for free, once built.
  - Revoking a session on a different UI replica than the one currently serving the admin's own
    request would silently no-op today (the revoke endpoint only ever sees its own instance's
    map). Acceptable for the current single-instance docker-compose deployment target; flagged
    here so it isn't mistaken for a bug if the deployment target changes.
- A user can't revoke their own current session from this page (the button is disabled with an
  explanatory tooltip) — `/logout` is the correct action for that, and disabling it here avoids a
  confusing "am I about to log myself out" moment on a page meant for managing *other* sessions.
