# ADR-0141: Global Deleted Runtime Entity History

- Status: Accepted
- Date: 2026-07-25

## Context

The Ontology workbench showed object and relationship-instance history only for
records still present in the live graph. Deletion consequently removed the
runtime entity from the visible audit surface, even though immutable snapshots
were already being stored.

## Decision

Expose tenant-scoped global object history and relationship-instance history
through the ontology service, Query Gateway, and versioned API resources:

- `GET /api/v1/ontology/objects/history`
- `GET /api/v1/ontology/links/history`

The Console groups records by runtime ID, derives readable labels from live or
retained snapshots, and marks deleted objects and edges explicitly. Existing
per-record history resources remain available for focused detail views.

## Consequences

Deleted runtime entities remain auditable without reintroducing them into the
live ontology. History reads remain tenant-scoped and append-only, and the
workbench can present a complete model-change timeline across definitions and
runtime graph state.
