# 0068. Active Sessions page sortable columns

## Context

A UI/UX audit found Active Sessions had search (ADR-0062) but, like most list pages, no column
sorting — it was always shown hardcoded most-recent-first with no way to sort by user or role.

## Decision

`GET /security/sessions` accepts `?sort=username|role` and `?dir=asc|desc`, same shape as the
Users page (ADR-0064), applied after the search filter so search and sort compose. Unlike Users
(which defaults to ascending-by-username when `sort` is unset), this page keeps its original
default — most recently signed-in first — since "who signed in most recently" is the more
useful default for a security-review page than alphabetical order. Column headers are toggle
links with a ▲/▼ indicator on the active column, including "Signed in" itself, which now also
toggles between newest-first and oldest-first instead of being a fixed order.

## Consequences

- Scoped to Active Sessions only. Same `sort_rows` shape as ADR-0064/ADR-0066 is the template
  for any remaining list pages without sorting (Analysis Configs, Normalization Mappings).
- The "Signed in" column's toggle logic is slightly asymmetric from Username/Role (its own
  default direction is descending, not ascending) since the page's existing default behavior
  had to be preserved, not reset to a generic ascending default.
