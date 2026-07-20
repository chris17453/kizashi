# 0070. Triggers page sortable columns

## Context

A UI/UX audit found Triggers had pagination and, as of ADR-0066, search, but still no column
sorting — always shown in whatever order the backend returned (which happens to be
`ORDER BY name` already, but with no way to sort by event type or enabled status, or to reverse
the order).

## Decision

`GET /triggers` accepts `?sort=name|event_type_match|enabled` and `?dir=asc|desc`, same pattern
as Users (ADR-0064) and Active Sessions (ADR-0068), applied after the search filter. Like search,
since `list_triggers` is server-paginated this only reorders the *current page's* rows, not the
tenant's full trigger set — the same accepted "doesn't compose with pagination in one request"
limitation ADR-0063/ADR-0066 already documented. An unset `sort` keeps the existing
`ORDER BY name` default rather than introducing a new one. `q`, `sort`, and `dir` all carry
through the search form and Previous/Next pagination links so none of the three states are lost
when the others change.

## Consequences

- Sorting by "Enabled" groups enabled triggers first (ascending) or disabled first (descending)
  — a boolean column has no natural alphabetical order, so this is the one column where "asc"
  doesn't mean A-to-Z.
- Normalization Mappings, the other list page still without sort, was evaluated and deliberately
  skipped rather than given the same treatment — its backend already returns
  `ORDER BY source_type` and the list is realistically one row per tenant, so a sort UI would add
  no real value (recorded separately, alongside its pagination decision).
