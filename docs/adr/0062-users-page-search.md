# 0062. Users page search

## Context

A UI/UX audit found that every list page except Data (`ui/src/data_handler.rs`) renders its
entire table with zero filter controls — a real gap for the Users page specifically, where an
admin managing a workspace with more than a screenful of accounts has no way to find one without
scrolling and reading.

## Decision

`GET /users` accepts a `?q=` query param and filters the already-fetched user list in-handler by
a case-insensitive substring match on username, rather than adding a search parameter to
`UsersClient::list_users` itself. `list_users` returns a whole tenant's accounts (a bounded,
realistically small list — nothing like `Data`'s potentially-huge ingested-record volume, which
is why that page's search genuinely needs to be a server-side query param instead). The search
box is a plain `GET` form so the filter is bookmarkable/shareable via URL, matching every other
query-param-driven page in this app, with a "Clear" link when a filter is active and a distinct
"no results" empty state from the "no users at all" one.

## Consequences

- Scoped to Users only, not every list page named in the audit (Sensors, API Keys, Sessions,
  Login Attempts, Egress Allowlist, Normalization Mappings, Retention Policies) — those are
  real, similar gaps but out of scope for this change; Users was picked as the first, most
  clearly admin-facing case. The same `matches_query`-style pattern is a direct template for
  closing the others as follow-ups.
- In-handler filtering means the search only ever operates on what `list_users` already
  returned — fine at today's realistic tenant-user-count scale, but if a tenant's user list ever
  grows large enough that "fetch everything, then filter" becomes a real cost, this should move
  to a proper backend query parameter instead (the same tradeoff `Data`'s search already made in
  the other direction).
