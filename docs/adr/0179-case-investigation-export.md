# ADR-0179: Case investigation export

- Status: Accepted
- Date: 2026-07-25

## Context

Case detail already supports CSV evidence export and the authenticated API
already exposes a bounded `/api/v1/incidents/:id/360` read model. Operators
need a portable investigation handoff that includes the case timeline and
governed response context without losing tenant and authorization boundaries.

## Decision

Add a one-click JSON export control to case detail that downloads the existing
authenticated Incident 360 response. The browser does not assemble or widen
the case payload; it requests the server-owned bounded contract and downloads
it with a deterministic case-investigation filename.

## Consequences

- Case handoffs include the same evidence, lifecycle, response, model, review,
  and degradation context available to API consumers.
- Authorization, tenant scoping, and payload bounds remain centralized in the
  existing Incident 360 endpoint.
- CSV evidence export remains available for tabular workflows.
