# 0054. Data subject rights: export and delete

## Context

The compliance rubric introduced in ADR-0051 names data subject rights (GDPR/CCPA-style "export
everything about me" / "delete everything about me" requests) as another uncovered domain. A
full survey of what "everything about a person" could mean across Kizashi surfaced a hard
boundary: local user accounts (`local_users`) are a structured, identity-keyed entity, but
ingested `RawRecord`/`Event` content (Zendesk tickets, mail) is opaque per-tenant-mapped JSONB
with no guaranteed, indexed identity field (`entity_ref` is a freeform per-mapping string, not a
normalized email column) and no full-text search capability exists anywhere in the platform.
Building "find every ticket/email that mentions this person" is a real, open-ended
search/indexing project — not something to bolt onto this change without silently overscoping
it.

## Decision

**v1 scope is explicitly limited to local user accounts and directly-attributable-by-account
records**: the `local_users` row itself, its `auth_audit_log` entries (already looked up by
entity id via the existing `list_for_entity`), and its `login_attempts` rows (looked up by
username, since that table isn't foreign-keyed to `local_users.id` — a person's username, not
their account id, is what a login attempt actually records). Ingested `RawRecord`/`Event`
content is out of scope for v1, documented here rather than left as a silent gap.

**Export**: new `GET /v1/users/:id/data-subject-export` (auth-service), `Admin`-only,
tenant-scoped (verifies the fetched user's `tenant_id` matches the caller's before returning
anything — a 404, not a 403, for a different tenant's user id, so the endpoint doesn't confirm
whether an id exists at all outside the caller's tenant). Returns the account record (already
`#[serde(skip)]`s `password_hash`/`mfa_secret` on `LocalUser`, so no new redaction needed), the
full audit trail, and every login attempt for that username, as one JSON document. Console UI:
a "Export data" link per row on `/users`, served as a `Content-Disposition: attachment` download
via a new `GET /users/:id/export` proxy route — no new modeling on the UI side, the JSON passes
through as raw bytes since the Console UI only needs to hand it to the admin, not parse it.

**Delete**: no new endpoint. `DELETE /v1/users/:id` (existing, ADR-0016 follow-up) already
hard-deletes the account and writes an audit row recording the deletion itself — that already
satisfies "delete this person's account." We deliberately do **not** delete or mutate the
matching `login_attempts`/`auth_audit_log` rows for a removed user: both tables are
DB-trigger-enforced append-only (CLAUDE.md §5's audit-immutability guarantee), and weakening that
trigger to carve out a "delete for this one identity" exception would undermine the exact
property that makes the audit trail trustworthy to a compliance reviewer in the first place.
Security/audit logs referencing a deleted account persist under the same legitimate-interest
basis security logging generally relies on (fraud/abuse investigation, incident response) — this
is a documented v1 decision, not an oversight.

## Consequences

- A data subject request against ingested ticket/email content cannot be answered by this
  feature. If that becomes a real requirement, it needs its own project (a real identity index
  over normalized records) and its own ADR — not a quiet scope-creep of this one.
- Deleting a `local_users` account does not scrub that username from `login_attempts` or
  `auth_audit_log` — an admin exporting a *different* still-existing account's data, or browsing
  the login-attempts page, may still see the deleted username referenced in historical rows. This
  is intentional (see Decision) but worth remembering if a future auditor asks "why does this
  deleted user still show up here."
- `login_attempts.username` is a plain string, not a foreign key — `list_by_username` is a
  best-effort match on the literal username at record time. If a username is ever reused after an
  account is deleted and recreated, the new account's export would also surface the old account's
  historical login attempts under the same username. Accepted for v1; a future change could scope
  this by a stable per-account identifier instead if it becomes a real problem.
