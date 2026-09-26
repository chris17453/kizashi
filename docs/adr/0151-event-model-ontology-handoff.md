# ADR-0151: Event-to-Model Ontology Handoff

- Status: Accepted
- Date: 2026-07-25

## Context

Event Detail already connected signals to source evidence, cases, and modeled
context. When a signal resolved to multiple modeled entities, those entities
could only be opened one at a time, forcing operators to repeat selection in
Ontology before comparing impact.

## Decision

Expose typed multi-selection for the related modeled entities on Event Detail.
The handoff merges object IDs and object-type IDs into the existing Ontology
selection store and supports comparison of up to six entities or loading the
selection into the workbench.

## Consequences

Signal investigations can move from lineage into model analysis without losing
entity type context. The workflow reuses existing browser selection state and
bounded comparison/action controls rather than adding a signal-specific
mutation path.
