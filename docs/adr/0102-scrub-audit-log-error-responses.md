# ADR-0102: Scrub audit log error responses

- **Status:** accepted
- **Date:** 2026-07-20

## Context

An eleventh audit pass, widened beyond the Console UI to backend error-handling consistency,
found that `auth-service`'s `local_login_handler.rs` and `user_error_response` (used by
create/update/delete user) already established the right pattern for backend failures — log the
real error server-side via `tracing::error!`, return a generic message to the client — but three
handlers in `user_handlers.rs` didn't follow it: `get_user_audit_log`, `get_recent_audit_log`,
and `post_session_revoked_audit` (the last of which I introduced myself in the immediately prior
PR, without noticing the inconsistency at the time). All three passed `e.to_string()` on a
repository/writer error straight into the HTTP response body — any Postgres connection hiccup,
constraint violation, or malformed query would put raw SQL error text in front of an
authenticated Console UI user.

The audit pass also flagged the same shape of gap in `dashboard-api`, `config-admin-service`,
`ingestion-gateway`, and `retention-service`. Scope here is deliberately limited to
`auth-service`'s three sites — the same "fix one concrete, verified instance per PR" discipline
this session has used throughout, rather than a sweeping cross-service change reviewed as one
undifferentiated diff. The other services are a real, tracked follow-up, not resolved by this PR.

A second finding from the same pass — config-admin-service/retention-service returning 403
instead of 404 on tenant mismatch — was investigated and found to be a false positive on closer
reading: those `tenant_mismatch` checks compare the request *body's* `tenant_id` against
`X-Tenant-Id` on create/update, before any repository lookup by id, so there's no existing
resource whose presence could be confirmed to an unauthorized caller. 403 is the correct response
for "you tried to write into a tenant that isn't yours." No change was made for that finding.

## Decision

`get_user_audit_log`, `get_recent_audit_log`, and `post_session_revoked_audit` now log the real
error via `tracing::error!` and return the same generic
`"an internal error occurred; check server logs for details"` message `user_error_response`
already uses for its `Backend` variant — same wording, so the two failure paths in this file are
indistinguishable to the client.

## Consequences

- These three endpoints no longer leak SQL/backend error text to Console UI users.
- The same pattern needs rolling out to `dashboard-api`, `config-admin-service`,
  `ingestion-gateway`, and `retention-service` as tracked follow-up work — not done here.
