# ADR-0149: Search-to-Ontology Investigation Handoff

- Status: Accepted
- Date: 2026-07-25

## Context

Global search could discover modeled entities, but each result only opened an
individual investigation route. Operators had to repeat the search or select
entities again in Ontology before comparing or applying bounded workbench
actions.

## Decision

Add a typed selection control to modeled-entity search results. Selected IDs
and object-type IDs merge into the existing browser ontology selection store;
operators can load the set into the workbench or open a bounded comparison
containing up to six selected entities. Existing selection state remains
available across search and Ontology surfaces.

## Consequences

Search becomes a first-class investigation entry point without duplicating
selection persistence or mutation controls. The handoff preserves type-aware
eligibility for downstream relationship and governed-action workflows while
keeping comparison bounded by the existing six-entity limit.
