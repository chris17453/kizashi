# ADR-0144: Object Investigation Handoff Canonicalization

- Status: Accepted
- Date: 2026-07-25

## Context

Several existing Console surfaces still emitted legacy links back to the
Ontology list with an `object_id` query parameter. Those links showed useful
context but did not open the dedicated Object 360 workspace.

## Decision

The authenticated shell canonicalizes legacy object links and investigation
focus metadata to `/ontology/objects/:id/360` at render time. This provides a
safe migration layer across search, cases, reports, actions, and lineage while
new templates adopt the canonical route directly.

## Consequences

Entity handoffs consistently preserve the selected object and open the same
bounded object-centric workspace. Existing bookmarks remain valid through the
original Ontology route, while in-console navigation converges on Object 360.
