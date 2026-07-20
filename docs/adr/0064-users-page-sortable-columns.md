# 0064. Users page sortable columns

## Context

A UI/UX audit found no table anywhere in the Console UI supports column sorting — every list is
always shown in whatever order the backend happens to return it. Users was the first page to get
search (ADR-0062); sorting is the natural next affordance for the same page.

## Decision

`GET /users` accepts `?sort=username|role` and `?dir=asc|desc`, applied in-handler after the
search filter (same "small enough list to sort client-side-of-the-fetch" reasoning as
ADR-0062's search). Column headers are links that toggle: clicking "Username" while already
sorted ascending by username switches to descending, and vice versa; clicking a different column
always starts ascending. An arrow (▲/▼) next to the active column's header shows the current
direction. Unset `sort`/`dir` falls back to ascending-by-username (a stable, predictable default
rather than "whatever order the backend returned").

## Consequences

- Scoped to Users only — the same `sort_rows`-style function is a direct template for the other
  list pages if sorting proves valuable there too (most are short enough that it matters less).
- Sorting is case-insensitive for username (`to_lowercase()` before compare) so "Bob" and "alice"
  interleave the way a human expects, not ASCII-code order.
- Search and sort compose correctly in one request (`?q=...&sort=...&dir=...`) since sorting is
  applied to the already-filtered row list, not the raw fetch — no version of the "doesn't
  compose with pagination" caveat ADR-0063 documented for Login Attempts applies here, since
  Users has no pagination to interact with.
