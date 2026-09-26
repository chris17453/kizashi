# ADR-0150: Case Impact Ontology Handoff

- Status: Accepted
- Date: 2026-07-25

## Context

Incident detail resolved linked signals into modeled impact entities, but the
impact list only supported individual navigation. Operators had to repeat the
selection in Ontology before comparing affected entities or applying bounded
workbench actions.

## Decision

Add typed multi-selection to the modeled-impact section of case detail. The
selection merges object IDs and object-type IDs into the existing Ontology
selection store, with actions to compare up to six impacted entities or load
them into the workbench.

## Consequences

Case investigation can move directly from evidence-derived impact to model
analysis and governed operations. The workflow reuses existing tenant-scoped
selection and comparison limits, preserving type-aware eligibility without
introducing a case-specific mutation path.
