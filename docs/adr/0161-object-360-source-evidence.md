# ADR-0161: Object 360 source evidence

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 used source lineage to connect modeled entities to downstream signals,
but operators could not see the retained source records in the investigation
workspace. Tracing a property back to primary evidence required leaving Object
360 and reconstructing the lineage manually.

## Decision

The versioned Object 360 read model resolves up to 25 tenant-scoped lineage
record IDs through the ingestion stats client and returns compact source-record
metadata. The Object 360 page renders those records as a dedicated Source
Evidence panel and adds them to the bounded newest-first investigation timeline;
each entry links to the existing Record Journey route.

## Consequences

Operators can move from modeled state to primary evidence without losing object
context. Payload bodies remain behind the existing Record Journey boundary, and
bounded resolution prevents untrusted lineage from causing unbounded reads.
