# ADR-0162: Object 360 bounded export

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 composes a bounded investigation package from modeled state, graph
neighbors, source evidence, signals, cases, governed decisions, and immutable
history. Operators needed a way to carry that exact reviewed context into an
external handoff without reconstructing it from multiple endpoints.

## Decision

Object 360 provides a client-side JSON export of the already-loaded versioned
read model. The export contains only the bounded response rendered for the
current tenant and object; it does not fetch additional records or bypass any
server-side authorization boundary. The downloaded file is named for the
investigated object ID.

## Consequences

Investigation handoffs are one action and remain faithful to the visible read
model. Export size and scope stay bounded by the API contract, while operators
retain the existing Record Journey and Audit routes for deeper evidence.
