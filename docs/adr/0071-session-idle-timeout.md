# 0071. Console UI session idle timeout

## Context

A fresh gap audit found `InMemorySessionStore` had no expiry logic at all — a session lived
until explicit logout, an admin revoking it via `/security/sessions`, or the UI process
restarting, no matter how long it sat idle. Session timeout is a baseline enterprise/compliance
control (an unattended, still-authenticated browser tab is a real exposure), and was genuinely
missing, not a cosmetic gap.

## Decision

`InMemorySessionStore` now enforces a sliding idle timeout, defaulting to 30 minutes,
configurable via `SESSION_IDLE_TIMEOUT_MINUTES`. Every successful `SessionStore::get()` call
(i.e. every authenticated request) both checks whether the session has been idle longer than
the timeout — deleting it and returning `None` if so, which `session_guard`'s existing
`require_session` already turns into a redirect to `/login` — and, if not, refreshes the
session's `last_active_at` to now. Continued activity keeps a session alive indefinitely; only
genuine idleness expires it. `list_for_tenant` (powering the `/security/sessions` admin page,
ADR-0046) also prunes expired sessions as a side effect, so an idled-out session doesn't keep
showing as "active" until something else happens to touch it.

`last_active_at` is tracked internally by the store (`HashMap<String, (Session, DateTime<Utc>)>`)
rather than added as a field on `Session` itself — `Session` is constructed directly across
every handler test in this crate, and a new required field would mean touching every one of
those call sites for a concern that's purely session-store bookkeeping, not part of the
session's own identity/claims.

## Consequences

- Single-instance-only, same limitation `list_for_tenant`'s existing doc comment already
  states: a multi-replica UI deployment would need a shared session backend (e.g. Redis) before
  idle timeout — or anything else about sessions — works consistently across replicas. Not a
  new limitation introduced by this change.
- 30 minutes is a reasonable default, not a value derived from a specific compliance framework's
  mandated number; operators with a stricter or looser policy set `SESSION_IDLE_TIMEOUT_MINUTES`.
- No "your session is about to expire" client-side warning — a session simply stops working on
  the next request past the timeout, redirecting to `/login`. A warning banner is a reasonable
  follow-up if idle timeout in practice interrupts users mid-task often enough to matter.
