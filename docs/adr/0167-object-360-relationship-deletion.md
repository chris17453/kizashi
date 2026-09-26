# ADR-0167: Object 360 relationship deletion

## Status

Accepted

## Context

Object 360 now supports governed relationship creation and editing, but an
administrator still had to return to the global Ontology workspace to remove a
relationship instance. This interrupted the investigation and made the edge
lifecycle asymmetric in the focused workspace.

## Decision

Administrator-only Object 360 relationship cards will expose a guarded delete
form using the existing relationship mutation endpoint. A validated return
path may bring the operator back to the same Object 360 route; untrusted or
missing return paths fall back to the global Ontology page. The ontology
service remains authoritative for tenant scope, deletion, actor attribution,
and immutable delete history.

## Consequences

The full relationship lifecycle is available from Object 360 while preserving
the existing RBAC and audit boundary. Deleted edges disappear from the live
graph but remain inspectable through immutable relationship history.
