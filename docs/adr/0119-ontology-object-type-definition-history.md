# ADR-0119: Ontology object-type definition history

## Status

Accepted

## Context

Object instances and governed action types already expose immutable history, but object-type definitions did not. Editing a schema or source mapping therefore changed the live contract without leaving an auditable definition trail.

## Decision

Persist an append-only history row for every object-type create, update, and delete. Each row is tenant-scoped and records the actor, change type, timestamp, and JSON snapshots before and after the mutation. Existing definitions are backfilled with a system-created snapshot during migration.

Expose the history through the ontology service, the authenticated Console ontology page, and the versioned `/api/v1/ontology/object-types/:id/history` endpoint. The Console renders the snapshots as read-only definition history alongside the selected type.

## Consequences

- Schema and mapping changes are attributable and reviewable.
- Delete operations retain the last known definition for audit and recovery analysis.
- History storage grows with definition mutations and must follow the ontology retention policy.
- The live type remains the source of truth; history is an audit record, not a rollback mechanism.
