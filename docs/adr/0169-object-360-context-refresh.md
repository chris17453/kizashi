# ADR-0169: Object 360 context refresh

## Status

Accepted

## Context

Object 360 loaded its bounded investigation read model once per page visit.
Long-running investigations could therefore show stale relationships, signals,
cases, or governed decision state after other operators changed the workspace.

## Decision

Add an explicit refresh control that re-fetches the same authenticated Object
360 endpoint, re-renders the bounded read model, and reports the last successful
refresh or a degraded refresh while retaining the previous context. Relationship
editor form ownership is specified separately in ADR-0170.

## Consequences

Operators can make a deliberate freshness check before acting without losing
the canonical investigation route. Refresh uses the existing tenant-scoped
read boundary and introduces no second data model or background polling load.
