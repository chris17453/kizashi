# ADR-0129: Ontology bulk relationship API

## Status

Accepted

## Context

The Ontology workbench supports selecting up to 25 source objects and creating relationship
instances against one governed target. The versioned API exposed single relationship creation but
not that object-centric batch workflow.

## Decision

Add `POST /api/v1/ontology/links/instances/bulk` with one link type, one target object, optional
properties, and 1–25 source object IDs. The Console API handler loops through the existing
ontology-service relationship creation contract and reports `created` and `failed` counts.

## Consequences

- External operator tooling has parity with the relationship workbench.
- Ontology Service remains authoritative for tenant scope, cardinality, object types, and audit
  behavior.
- Partial success is explicit and does not roll back already-created valid relationships.
