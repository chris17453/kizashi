# ADR-0118: Governed Ambiguous Correlation Resolution

- **Status:** accepted
- **Date:** 2026-07-24

## Context

Event-driven correlation correctly refuses to choose when a group key belongs to multiple active
incidents. The previous Console sweep omitted those signals entirely, leaving operators without a
single review surface for deciding which case should own them.

## Decision

The correlation review groups unlinked events by normalized group key and displays the active
incident options for each ambiguous group. An Operator or Admin may explicitly select a target and
one or more signals. The server reloads the tenant-scoped incident/event evidence immediately
before linking and rejects stale groups, inactive targets, or event IDs outside the group.

Links retain the normalized group key and use the existing audited incident link contract. No
automatic target selection or lifecycle transition is introduced. The versioned read API exposes
the same ambiguous groups for machine clients.

## Consequences

Ambiguous correlation becomes actionable without weakening the safe event consumer. Operators get
an auditable decision point, while stale UI state and cross-tenant IDs fail closed.
