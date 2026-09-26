# ADR-0136: Preserve ontology contract identity timestamps

- Status: Accepted
- Date: 2026-07-25

## Context

Ontology contract updates retain the same object identity and append an immutable definition
history entry. Relationship-type and action-type updates were incorrectly replacing
`created_at` with the update time, making a long-lived contract appear newly created and
weakening chronology in operator read models.

## Decision

When updating a relationship type or governed action type, load the existing contract, preserve
its `created_at`, and set only `updated_at` to the mutation time. Missing action types fail with
`404` before attempting an update, matching the relationship and object-type mutation paths.

## Consequences

- Contract identity chronology remains stable across edits and history snapshots.
- Existing update APIs remain compatible; only timestamp correctness and missing-resource
  behavior are tightened.
- The invariant is covered by ontology-service integration-style in-memory router tests for
  both contract families.
