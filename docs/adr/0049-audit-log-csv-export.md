# 0049. Audit log CSV export

## Context

The global Audit Log page (ADR-0045) is browsable but only on-screen, one 50-row page at a time.
A compliance officer preparing an audit report needs to pull a range of activity out of the
system entirely — into a spreadsheet, an email attachment, a ticket — not just scroll through
pages in a browser. This is a standard enterprise-compliance expectation the HTML-only page
didn't meet.

## Decision

Add `GET /audit-log/export.csv`, reusing the exact same merge-across-three-services logic the
HTML page already uses (factored out into a shared `fetch_merged_page` helper so the two can
never silently diverge in what counts as "the tenant's recent activity"), but internally
paginating up to `CSV_MAX_PAGES` (10) pages of `CSV_PAGE_LIMIT` (200, the backend's own per-call
maximum) per source — up to 6000 rows — instead of returning just one page like the HTML view.
Columns: `changed_at, service, entity_type, change_type, actor`. Standard CSV field-escaping
(quote-wrap and double any embedded quote) for any value containing a comma, quote, or newline.

The row cap is a deliberate bound, not an attempt at exhaustive export: an unbounded loop against
a very long-lived, high-activity tenant could make a single request take an unreasonable amount
of time or produce an unreasonably large response. A "Download CSV" link on the Audit Log page is
the only entry point — no separate date-range picker in this v1, matching the HTML page's own
lack of date filtering (both share the same `before`-cursor pagination model underneath).

## Consequences

- A tenant with more than 6000 audit entries across the three services can't get a single
  complete export from this endpoint — they'd need multiple exports using the last row's
  timestamp as a manual starting point (not currently wired as a query param on the CSV route,
  since the immediate need was "get *a* useful export working," not full historical
  completeness). A follow-up could add `?before=` support to the CSV route itself, reusing the
  same cursor the HTML page already exposes, if full-history export becomes a real need.
- No new backend endpoints — this is pure aggregation in the Console UI over calls the audit
  clients already support (`list_recent`), the same shape as ADR-0047's dashboard aggregation.
