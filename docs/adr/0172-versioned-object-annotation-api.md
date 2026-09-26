# ADR-0172: Versioned object annotation API

## Status

Accepted

## Context

Object annotations were available through Object 360’s browser form and
included in the read model, but API consumers could not address the annotation
resource directly. That limited integrations, automation, and alternate
operator surfaces.

## Decision

Expose `GET` and operator-only `POST`
`/api/v1/ontology/objects/:id/annotations`. The endpoint uses the existing
session or service-account principal, tenant-scoped ontology client, bounded
annotation body, and actor attribution. The browser Object 360 route remains a
presentation-layer convenience over the same ontology contract.

## Consequences

External command surfaces can read and add object context without reconstructing
the Object 360 aggregate. Authorization and append-only semantics remain owned
by the ontology service rather than duplicated in API consumers.
