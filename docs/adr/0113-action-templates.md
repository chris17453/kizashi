# ADR-0113: Governed reusable response action templates

## Context

Triggers currently persist a complete `ActionRef` inline. That keeps the first trigger-builder
implementation small, but makes provider configuration, email content, and delivery policy hard
to reuse or review. The Console specification requires configurable automated actions and a
dedicated authoring surface.

## Decision

Add a tenant-scoped, versioned `ActionTemplate` entity in Config/Admin Service. A template stores
the existing `ActionType` plus provider configuration and descriptive metadata, and every create,
update, or delete is audit logged. Trigger definitions continue to accept inline `ActionRef`
records for backward compatibility; a later trigger-builder step may resolve a selected template
into a snapshot at save time so execution remains asynchronous and self-contained.

The first entity slice deliberately does not add secrets or provider delivery logic. Secret
references remain configuration values managed by the existing encrypted configuration paths;
template authoring validates the action shape and leaves execution to Action Executor.

## Consequences

- Operators can author and review reusable response contracts per tenant.
- Existing triggers and consumers remain wire-compatible.
- Template updates do not silently mutate already-persisted trigger action snapshots.
- The Console now exposes tenant-scoped template selection in the trigger editor and the same
  entity through the versioned Console API; inline actions remain supported for compatibility.
