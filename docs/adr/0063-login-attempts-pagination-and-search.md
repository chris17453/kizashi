# 0063. Login attempts pagination and search

## Context

A UI/UX audit flagged the Login Attempts page specifically as a real gap: it's "naturally
high-volume" (every failed login attempt against an actively-targeted tenant lands here) but had
neither search nor pagination — the Console UI's `LoginAttemptsClient::list_recent` never
exposed the `before` cursor `PostgresLoginAttemptRepository::list_recent` (auth-service,
ADR-0053) already implements server-side, so the page was permanently capped at whatever the
backend's default page size (50) happened to return, with no way to see further back.

## Decision

Extended `LoginAttemptsClient::list_recent` to accept the same `before: Option<DateTime<Utc>>`
exclusive keyset cursor `/audit-log`'s "Load older" link already uses (ADR-0045) — same
parameter shape reused rather than inventing a different pagination style for one more page.
`GET /security/login-attempts` now accepts `?before=` and shows a "Load older" link whenever a
full page (50 rows) comes back, mirroring the audit log's exact UX. `?q=` adds the same
in-handler search filter as the Users/API Keys/Sessions pages (ADR-0062), applied to whichever
page was already fetched.

## Consequences

- **Search and pagination don't compose within one request**: `?q=` filters only the page
  `?before=` fetched, so a search for a username whose only matching attempts sit on an older
  page won't find them without first paging back to where they are. This is the same
  accepted-limitation shape ADR-0062 already noted for in-handler-filtered pages, just made more
  visible here because this feed is naturally larger. A combined server-side search+pagination
  query (extending `login_attempt_handler.rs`'s `GET /v1/auth/local/login-attempts` with a
  `username` filter) is the real fix if this proves to matter in practice — not built here.
- `compliance_report_handler.rs`'s failed-login-count tile calls `list_recent` with `before:
  None` (unchanged behavior) — it only ever needed the most recent page for its 7-day rollup, so
  it doesn't participate in pagination.
