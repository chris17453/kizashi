# ADR-0100: Events CSV export

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A ninth Console UI audit pass found Events was structurally identical to Data and Login
Attempts/Audit Log — search, date-range filter, sortable columns, server-side pagination — but
was the only one of the four missing a CSV export. Events is trigger-firing history, directly
compliance-relevant (the same "what happened, and when" question a reviewer asks of Data or
Audit Log), so the gap had no functional justification.

## Decision

Add `GET /events/export.csv`, following the same bounded-pagination export shape as
`data_handler`/`login_attempts_handler`/`recent_audit_log_handler` (ADR-0049): loop
`EventsClient::list_events` with `offset` advancing by `DEFAULT_PAGE_SIZE` each iteration, up to
`CSV_MAX_PAGES` (10), stopping when a page's `has_more` is false. Honors the same `?from=`/`?to=`
date-range filter the HTML page already supports (`?q=`/sort are not forwarded, same
accepted-limitation shape as the other client-side-filtered list pages — the export is a
superset of the raw filtered-by-date feed, not the exact narrowed-by-search view). Columns:
`occurred_at,event_type,group_key,status`. A "Download CSV" link/form was added above the search
bar, passing the current `from`/`to` filter through as hidden fields, matching Data's export
form placement.

## Consequences

- Events, Data, Login Attempts, and Audit Log now all share the same CSV-export shape and
  bounded-pagination tradeoff — the fourth and last of this session's audit-pass-driven exports.
- Like Login Attempts' export, this doesn't expose a continuation cursor — 10 pages ×
  `DEFAULT_PAGE_SIZE` was judged sufficient for v1, consistent with the other exports' sizing
  rationale.
