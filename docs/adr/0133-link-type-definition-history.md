# ADR-0133: Relationship-type definition history

**Status:** Accepted  
**Date:** 2026-07-25

## Context

Ontology object types and governed action types already retained immutable definition snapshots,
but relationship types did not. A change to cardinality, endpoints, or relationship properties
could therefore alter the operating model without a first-class definition history for review.

## Decision

Add tenant-scoped `link_type_history` snapshots for relationship-type create, update, and delete
operations. Each snapshot records the actor, change type, timestamp, and before/after definition
state. The Ontology service exposes history through its protected API; the Console renders it in
the Ontology workbench; and the versioned Console API exposes
`GET /api/v1/ontology/link-types/:id/history`.

## Consequences

Relationship contracts now have the same immutable governance posture as object and action
contracts. Existing relationship instances are unchanged, and history is additive: callers that
only need the current contract continue using the existing link-type resource. A migration seeds a
created baseline for existing definitions.
