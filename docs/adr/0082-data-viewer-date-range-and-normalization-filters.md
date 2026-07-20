# 0082. Data Viewer date-range and normalization-status filters

## Context

A fourth audit pass found the Data Viewer's search form didn't expose `from`/`to`/`normalized`
filters, even though Ingestion Service's `search_records` endpoint (`SearchRecordsQuery`) has
already accepted them since the search API was first built. A compliance investigator scoping a
search to a specific incident window, or an operator checking what's still un-normalized (the
same population `POST /data/reprocess` already acts on, but until now with no way to see it
filtered), had no UI path to either — pure UI wiring gap, no backend work needed.

## Decision

`RecordSearchFilter` (the UI's client-side filter struct) gains `from`/`to: Option<DateTime<Utc>>`
and `normalized: Option<bool>`, forwarded to the backend exactly as `SearchRecordsQuery` expects.
`DataSearchQuery` (the query-string-deserialized form) keeps `from`/`to` as plain `String` —
`<input type="date">` submits `YYYY-MM-DD`, not a full timestamp, so parsing happens by hand in
`parse_date_range`: `from` becomes the start of that day, `to` becomes the end of it, so a range
like "2026-07-15 to 2026-07-20" is fully inclusive of both endpoint days, matching how an
investigator would actually read a date-range filter. `normalized` stays a `String` too (`""`,
`"true"`, or `"false"` from a `<select>`) since a plain `Option<bool>` field can't cleanly
represent the "no filter" empty case. Both new filters are also captured in saved searches
(`SaveSearchForm`) so a bookmarked search preserves them.

## Consequences

- An unparseable or empty date input silently becomes "no filter" rather than an error — matches
  how every other free-text filter field on this page already behaves when left blank.
- The date range is day-granularity only (no time-of-day precision) — appropriate for "which day
  did this happen," not appropriate for narrowing to a specific hour; a future need for hour-level
  precision would mean switching the input type, not the underlying filter/parsing shape.
