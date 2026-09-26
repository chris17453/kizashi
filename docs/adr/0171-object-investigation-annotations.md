# ADR-0171: Object investigation annotations

## Status

Accepted

## Context

Object 360 joins modeled state, source evidence, signals, cases, decisions, and
immutable model history, but operators had no durable place to record an
observation or handoff directly on the investigated entity. Case notes are not
an adequate substitute when the context applies to the modeled object across
multiple cases.

## Decision

Add tenant-scoped, append-only object annotations owned by the ontology service.
Each annotation stores the modeled object, acting operator, bounded body, and
creation time. Operators with write access can add annotations; all operators
can read the bounded annotation feed. Object 360 includes the feed in its read
model and investigation timeline.

## Consequences

Investigation context survives page changes and case transitions while remaining
inside the object’s tenant and authorization boundary. Annotations do not mutate
modeled state or replace immutable object history, so analytical commentary and
governed model changes remain distinct.
