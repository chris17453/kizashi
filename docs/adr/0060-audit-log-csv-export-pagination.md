# 0060. Audit log CSV export pagination

## Context

ADR-0049 (audit-log CSV export) capped the export at `CSV_MAX_PAGES * CSV_PAGE_LIMIT * 3` (6000)
rows per request and noted the gap explicitly: "A follow-up could add `?before=` support to the
CSV route itself... if full-history export becomes a real need." A tenant with more than 6000
audit rows had no way to get the rest — the export silently stopped, with nothing telling the
caller more history existed.

## Decision

`GET /audit-log/export.csv` now accepts the same `?before=` cursor the HTML page's "Load older"
link already uses, and starts its internal pagination loop from that cursor instead of always
`None`. The loop tracks whether it stopped because a source ran genuinely dry (`exhausted`) or
because it hit `CSV_MAX_PAGES` while a source still had more to give; in the latter case the
response carries an `X-Next-Before` header with the cursor to continue from, so a caller (or a
future scripted export) can chain requests to walk arbitrarily far back in history instead of
being silently truncated at 6000 rows (CLAUDE.md's "no silent caps" — a truncated export that
doesn't say so is worse than no export). The HTML page's "Load older" section grew a matching
"Download CSV from here" link using the same `before` cursor, so exporting from any point in the
browsed history is one click, not a hand-edited URL.

## Consequences

- Still no single-request unlimited export — a tenant with a very long history makes multiple
  requests, each following the previous one's `X-Next-Before` header. A "keep paging until done"
  convenience (server-side looping past `CSV_MAX_PAGES`, or a background export job) is a
  larger, different feature (unbounded response time/size) and stays out of scope here, same
  reasoning ADR-0049 gave for the original cap.
- No UI affordance reads the `X-Next-Before` header automatically (e.g. auto-chaining downloads)
  — a human clicking "Download CSV from here" after "Load older" is the intended v1 workflow.
