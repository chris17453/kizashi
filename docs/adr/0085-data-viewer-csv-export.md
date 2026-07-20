# 0085. Data Viewer CSV export of the current filtered search

## Context

A fifth audit pass, focused specifically on "data explorer" completeness per an explicit user
request, found the Data Viewer had no way to export a filtered search result set — an
investigator who narrowed a search down to "all Zendesk tickets mentioning 'printer' from July"
had to click into records one at a time to extract them, with no way to hand the filtered set to
another tool or attach it to a case. The global Audit Log already established the exact pattern
needed (`GET /audit-log/export.csv`, ADR-0049) — this closes the same gap for the Data Viewer.

## Decision

`GET /data/export.csv` accepts the exact same query parameters as `GET /data` and honors every
filter (`connector_id`, `source_type`, `q`, `subject`, `email_from`, `attachment_filename`,
`from`/`to` date range, `normalized` status) via a `build_filter` helper now shared between the
HTML view and the CSV export, so the two can never silently diverge on what "the current search"
means. Paginated internally up to `CSV_MAX_PAGES` (20 × `DEFAULT_PAGE_SIZE`) pages, same
bounded-export tradeoff the audit log CSV export already made — a tenant with more matching
records than that can narrow the search (especially the date range) to export the rest in a
follow-up request, rather than this looping unboundedly. Each row includes `id`, `connector_id`,
`source_type`, `ingested_at`, `normalized` (bool), and the raw payload as a JSON string — giving
an investigator the actual record content, not just metadata, to hand to another tool. The
Data Viewer page gets a "Download CSV of this search" link, submitted via the same hidden-field
pattern the existing pagination/save-search forms already use, so it always reflects the
currently-active filters.

## Consequences

- No new dependency, no backend change — the search API `search_records`/`RecordSearchFilter`
  already existed and already supported every filter used here.
- Unlike the audit log export, this doesn't currently support a `?before=`-style continuation
  cursor for resuming past `CSV_MAX_PAGES` — narrowing the date range is the documented way to
  get the rest. A cursor-based continuation is a reasonable follow-up if a real tenant's filtered
  result sets regularly exceed the current cap in practice.
- `raw_payload` is embedded as a single CSV field (properly quoted/escaped, not truncated) —
  large or deeply nested payloads make for a wide CSV cell, which is the expected tradeoff of
  putting real record content in a spreadsheet-friendly format rather than a summary.
