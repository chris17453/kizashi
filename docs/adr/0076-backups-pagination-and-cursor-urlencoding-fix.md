# 0076. Backups page pagination, and a cursor URL-encoding bug fix

## Context

A fresh audit found the Backups page (`GET /security/backups`) had no pagination — the backend
already accepted a `limit` internally but the HTTP handler hardcoded `DEFAULT_STATUS_LIMIT = 20`
with no way to page further back, unlike Login Attempts and the global Audit Log, which both use
an exclusive keyset cursor (`?before=`, ADR-0063).

While implementing the same cursor pattern here, a real, already-shipped bug surfaced: every
existing `?before={{ before }}` "Load older" link (Login Attempts, global Audit Log's HTML page
and CSV export link) interpolates a `DateTime<Utc>`'s rendered string directly into an `href`
with no URL-encoding. `chrono::DateTime<Utc>::to_rfc3339()` (and its `Display` impl) renders the
UTC offset as `+00:00`, and an un-encoded `+` in a query string is decoded as a space by the
`application/x-www-form-urlencoded` convention `serde_urlencoded` (what axum's `Query` extractor
uses) follows — so clicking any of those "Load older" links would send a corrupted timestamp to
the server. Askama's `{{ }}` only HTML-escapes, it doesn't percent-encode, so this was never
caught by existing tests (which all hand-wrote pre-encoded literal `...Z`-suffixed cursor values
in their URIs rather than rendering a real link and following it).

## Decision

**Pagination:** `BackupRunRepository::list_recent` gains a `before: Option<DateTime<Utc>>`
parameter (same exclusive-cursor shape as `LoginAttemptRepository`/the audit log clients),
implemented as two static SQL queries (`WHERE started_at < $1` vs. unconditional) rather than
one dynamically-built query, since sqlx doesn't cleanly support an optional bind in one
prepared statement. `GET /v1/backup/status` accepts `?before=`, the UI's `BackupStatusClient`
forwards it, and `GET /security/backups` shows a "Load older" link when a full page (20 rows)
comes back, mirroring `login_attempts_handler`'s `DEFAULT_LIMIT` pattern exactly.

**URL-encoding fix:** every `{{ before }}` (and `{{ q }}&before={{ before }}`) in an `href` now
uses Askama's built-in `|urlencode` filter (already in Askama's default feature set, no new
dependency), fixed in `login_attempts.html`, `recent_audit_log.html` (both the HTML page's link
and the CSV export link), and the new `backups.html`. `|urlencode` percent-encodes any
`Display`-able value, so it's correct regardless of the exact underlying string format, not just
today's `+00:00` offset.

## Consequences

- This retroactively fixes a real pagination bug in two already-shipped features (Login
  Attempts, global Audit Log) as a side effect of building Backups' pagination the same way —
  not a scope-widening choice, the bug was directly in the pattern being replicated.
- `q={{ q }}` (search terms) reflected into hrefs elsewhere in the UI have the same theoretical
  encoding gap for search terms containing `&`, `+`, or `#` — noted as a candidate follow-up,
  not fixed here, since it's a different root cause (arbitrary user input, not a fixed date
  format) and a broader sweep than this PR's scope.
- New tests assert against the actual rendered link text (not just its presence) to prove the
  fix, e.g. that the `before=` link contains no raw `+` character.
