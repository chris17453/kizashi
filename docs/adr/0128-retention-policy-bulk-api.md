# ADR-0128: Retention policy bulk API

## Status

Accepted

## Context

The Retention Policies Console supported selecting and deleting multiple policies, but the
versioned API only exposed one-policy deletion. External administrative tooling could not perform
the same bounded lifecycle cleanup.

## Decision

Add `POST /api/v1/retention-policies/bulk-delete` with 1–100 policy UUIDs. The Console API handler
loops through the existing tenant-scoped `delete_policy` client operation and returns per-item
`deleted` and `failed` counts. Operator authorization is required.

## Consequences

- API consumers have parity with the Console bulk deletion workflow.
- Retention Service keeps one audited mutation path and remains the source of authorization and
  tenant validation.
- Partial success is explicit in the response; callers can retry failed IDs safely.
