# ADR-0153: Object 360 Governed Actions

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 displayed governed decisions that had already executed, but an
operator had to leave the object investigation to inspect available contracts
or run an eligible action.

## Decision

Extend the Object 360 read model with up to 50 tenant-scoped action contracts
targeted at the current object type. Each contract includes its parameter
schema, preconditions, effect definition, and current eligibility. The browser
renders eligible contracts as execution forms that submit through the existing
governed action endpoint and return to the same Object 360 route.

## Consequences

Object investigation becomes an action-capable workspace while preserving the
normal contract validation, immutable invocation ledger, and actor attribution
boundaries. Ineligible contracts remain visible with an explicit blocked
posture, making the reason for non-execution inspectable.
