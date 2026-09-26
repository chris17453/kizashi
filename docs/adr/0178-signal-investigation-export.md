# ADR-0178: Signal investigation export

## Status

Accepted — 2026-07-25

## Context

Event detail is the canonical evidence context for a signal, but operators need
to hand a bounded investigation to another analyst or an external console. The
versioned Event 360 API already composes the signal, immutable status history,
source records, linked cases, executions, modeled objects, action contracts,
invocations, reviews, and degradation errors.

## Decision

Add a one-click browser export that downloads the authenticated Event 360 JSON
read model as a signal investigation bundle. The UI calls the existing
tenant-scoped API contract and does not create a second export query or change
the underlying evidence.

## Consequences

Operators can preserve and hand off a complete bounded signal context while API
clients and the browser use the same representation. Export contents inherit
the Event 360 tenant boundary and its bounded joins; a future signed or
server-generated artifact can evolve independently.
