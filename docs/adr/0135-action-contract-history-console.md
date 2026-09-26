# ADR-0135: Governed action contract history in the Ontology workbench

- Status: Accepted
- Date: 2026-07-25

## Context

Governed action types already record immutable definition snapshots and expose them through
the ontology service and versioned API. The Ontology workbench displayed the current contract
and allowed edits, but did not show the history beside the other modeled-contract histories.
That left an operator without local evidence of who changed a governed action or what its
parameter, precondition, and effect definitions were before and after the change.

## Decision

Load action-type history through the existing ontology client while building the Ontology
workbench view model. Render it as a read-only history panel with actor, timestamp, change
type, and before/after JSON snapshots. History lookup failures degrade to an empty panel entry
so current ontology authoring remains usable; the ontology service and versioned API remain the
authoritative audit surfaces.

## Consequences

- Action authoring now has the same visible audit posture as object and relationship contracts.
- The page performs one history read per visible action contract, bounded by the tenant's
  action-type count.
- No mutation path or history storage contract changes.
