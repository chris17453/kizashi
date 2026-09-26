# ADR-0166: Object 360 relationship creation

## Status

Accepted

## Context

Object 360 could edit existing relationship instances, but creating a new edge
required leaving the focused entity for the global Ontology authoring surface.
That made a common investigation-to-modeling transition unnecessarily
disconnected and offered no type-aware target shortlist.

## Decision

The Object 360 read model will expose a bounded list of relationship contracts
compatible with the focused object, including direction, target type, up to 50
tenant-scoped target objects, and the relationship property schema. The browser
will render one governed creation form per contract and submit to the existing
relationship-instance creation endpoint. The ontology service remains the
authoritative endpoint, cardinality, property-schema, actor, and immutable
history boundary.

## Consequences

Operators can create modeled edges while preserving investigation context and
type correctness. The response stays bounded and does not expose unrelated
object types. Existing duplicate/cardinality conflict behavior remains
visible through the normal governed mutation response.
