# ADR-0134: Relationship-instance history

**Status:** Accepted  
**Date:** 2026-07-25

## Context

Relationship-type definitions now have immutable contract history, but the live graph instances
created from those contracts did not. Operators could see that a relationship existed without
seeing when its endpoints or properties changed, which weakened lineage and investigation review.

## Decision

Add tenant-scoped `link_history` snapshots for relationship-instance create, update, and delete
operations. Snapshots contain actor, change type, timestamp, and before/after graph state. The
protected Ontology service and Query Gateway expose the history; the Console renders it alongside
the Ontology workbench; and the versioned API serves
`GET /api/v1/ontology/links/:id/history`.

## Consequences

Graph-state changes are auditable independently from relationship-contract changes. Existing link
instances receive a created baseline during migration, and all new Console/API mutations propagate
the authenticated actor through the gateway. Current link and link-type resources remain
backward-compatible.
