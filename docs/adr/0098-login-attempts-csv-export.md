# ADR-0098: Login Attempts CSV export

- **Status:** accepted
- **Date:** 2026-07-20

## Context

The eighth Console UI audit pass's second finding: Login Attempts (ADR-0053) was the only
enterprise-compliance security page without a CSV export, while Audit Log (ADR-0049) and Data
(the raw record viewer) both already had one. A tenant investigating a brute-force pattern or
handing evidence to a compliance reviewer needs to get this data out of the browser.

## Decision

Add `GET /security/login-attempts/export.csv`, `Admin`-only like the HTML page, using the same
shape as `recent_audit_log_handler`'s existing CSV export (ADR-0049): loop calling
`LoginAttemptsClient::list_recent` with the `before` cursor advancing each iteration, up to
`CSV_MAX_PAGES` (10) pages, stopping early once a page comes back short of `DEFAULT_LIMIT` (the
natural end of history). Columns: `attempted_at,username,success,reason`. A "Download CSV" link
was added to `login_attempts.html` above the search form, matching Audit Log's placement.

## Consequences

- Login Attempts and Audit Log now use an identical bounded-pagination CSV export shape —
  `recent_audit_log_handler`'s doc comments already explain the tradeoff (bounded rather than
  unbounded export, no silent cap per CLAUDE.md) and this implementation inherits it without
  needing to re-litigate it.
- Unlike Audit Log's export, this one doesn't expose a continuation cursor via `X-Next-Before` —
  Login Attempts history is lower-volume per tenant in practice (auth attempts, not every config
  mutation across every service), so 10 pages × 50 rows (500 rows) was judged sufficient for v1;
  revisit if a tenant's actual attempt volume proves that wrong.
