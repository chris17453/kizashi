# ADR-0143: Dedicated Object 360 Workspace

- Status: Accepted
- Date: 2026-07-25

## Context

The Ontology page displayed object context inline, while the platform already
exposed a bounded object-centric read model covering graph neighbors, lineage
signals, incidents, governed actions, and immutable history. Operators had no
dedicated workspace for following one modeled entity across those domains.

## Decision

Add an authenticated `/ontology/objects/:id/360` workspace. It hydrates from
the existing tenant-scoped `GET /api/v1/ontology/objects/:id/360` resource and
renders object state, related objects, signals/cases, governed decisions, and
history with deep links into the existing Console surfaces. Ontology
investigation focus links now open this workspace as the object handoff.

## Consequences

Browser operators and external clients share one object-360 contract. The
workspace keeps the existing shell and tenant/session boundary, caps content
according to the API read model, and avoids a second divergent object-join
implementation in the UI.
