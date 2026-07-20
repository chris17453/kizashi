# 0080. Sensors page search and sortable columns

## Context

A third audit pass found Sensors had pagination but, unlike every other list page shipped this
session (Users, API Keys, Active Sessions, Login Attempts, Field Mappings, Triggers, Events,
global Audit Log), no search or column sorting — a real parity gap for a tenant with many
registered sensors.

## Decision

`GET /sensors` accepts `?q=` (case-insensitive substring match on name) and
`?sort=name|connector_type|enabled` with `?dir=asc|desc`, same pattern and same
server-paginated-so-per-page-only caveat as Triggers (ADR-0066/ADR-0070). `q`/`sort`/`dir` carry
through the search form and Previous/Next pagination links.

## Consequences

- Same accepted "doesn't compose with pagination in one request" limitation as every other
  server-paginated list page's search/sort on this platform.
- The connector-registration form and deploy-script generator are unaffected — this only
  changes the read/list view.
