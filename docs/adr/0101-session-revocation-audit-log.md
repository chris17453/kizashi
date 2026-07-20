# ADR-0101: Session revocation audit log

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A tenth Console UI audit pass, cross-referencing every destructive admin action against its
audit-log coverage, found that forcing a session out via `/security/sessions/:id/revoke` (or its
bulk variant) wrote no audit entry anywhere — every other destructive admin action in the
platform (user delete/role-change, API key revoke, sensor delete, retention policy delete, the
egress allowlist fix from ADR-0097) does. Console UI's session store is a purely in-memory,
per-process map (ADR-0014's `SessionStore` trait) with no database of its own, so this isn't a
missing call to an existing repository method — there was genuinely nowhere for the trail to
live.

## Decision

Auth Service already owns `auth_audit_log` (user/branding mutations) and already exposes the
generic, entity-type-agnostic `GET /v1/audit-log/:entity_id` read path the Console UI's shared
`AuditLogClient`/`audit_log_handler.rs` already know how to render. Rather than standing up a new
service or a new audit table, a session revocation is recorded there too, under
`entity_type = "session"`:

- New `crates/auth-service/src/session_audit_writer.rs`: a `SessionAuditWriter` trait +
  `PostgresSessionAuditWriter` impl, structurally similar to the other `*_repository.rs` audit
  writers but with no accompanying row mutation of its own — the entity being audited (the
  session) lives in a different process entirely, so this is a write-only "record that this
  happened" call, not a repository method wrapping a CRUD operation.
- New `POST /v1/audit-log/session-revoked` (`Admin`-only), taking `{session_id, revoked_username}`
  and writing one `auth_audit_log` row via the existing `record_audit_entry`/immutability-trigger
  infrastructure.
- `ui/src/users_client.rs`'s `UsersClient` trait (already the Console UI's client for
  Admin-gated calls to Auth Service) gained `record_session_revocation`, called from both
  `post_revoke_session` and `post_bulk_revoke_sessions` after the in-memory delete succeeds.
- `sessions.html` gained a per-row "History" link to `/audit-log/auth/{{ s.id }}` — no new
  service arm needed in `audit_log_handler.rs`'s switch, since Auth Service's
  `get_user_audit_log` was already generic on `entity_id` regardless of `entity_type`.

Unlike every prior audit-log fix this session (all same-service, same-transaction writes), this
one crosses a process boundary: Console UI's session store and Auth Service's Postgres audit log
have no shared transaction. The write is deliberately best-effort — a failure to *record* the
revocation must never undo or block the revocation itself, same "log even if it fails, but don't
let it block the primary action" philosophy `record_attempt` already uses for login attempts.

## Consequences

- Session revoke now has the same audit coverage as every other destructive admin action in the
  platform — closes the last gap the tenth audit pass found of this shape.
- Best-effort means a rare case exists where a session was genuinely revoked but Auth Service was
  briefly unreachable, leaving no audit row for that one revocation — accepted, since blocking
  the revocation on Auth Service's availability would make forcing a compromised user out
  *harder* in exactly the scenario (Auth Service under strain from an incident) where that
  matters most.
- Live-verified against a running stack: revoking a session now shows a real, immutable
  `deleted`-type audit row (actor, `revoked_username`) at `/audit-log/auth/<session_id>`.
