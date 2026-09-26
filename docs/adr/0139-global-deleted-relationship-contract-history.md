# ADR-0139: Global Deleted Relationship Contract History

- Status: Accepted
- Date: 2026-07-25

## Context

Relationship-type history was previously read only for currently existing
contracts. A deleted relationship therefore disappeared from the Ontology
workbench's contract audit, unlike the action-contract history surface.

## Decision

Expose tenant-scoped global relationship-type history through the ontology
service, Query Gateway, and `GET /api/v1/ontology/link-types/history`. The
Console groups immutable records by relationship-type ID, derives deleted names
from retained snapshots, and marks deleted contracts in the audit panel.

## Consequences

The relationship contract audit is complete across active and deleted
definitions without restoring deleted records. Existing per-contract history
and relationship-instance history remain unchanged, and all reads retain the
authenticated tenant boundary.
