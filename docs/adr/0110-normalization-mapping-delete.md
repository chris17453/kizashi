# ADR-0110: Normalization Mapping Delete

- **Status:** accepted
- **Date:** 2026-07-20

## Context

ADR-0109 closed an identical gap for `TriggerDefinition`: no delete capability anywhere in the
stack despite full create/update support. A follow-up audit pass found the sibling entity,
`NormalizationMapping`, had the exact same shape of gap — `NormalizationMappingRepository` had
no `delete`, `MappingPublisher` only ever published a bare `NormalizationMapping` (no way to
signal removal), and normalization-service's own mirrored copy of every mapping (the one it
actually applies during normalization, per ADR-0010/ADR-0018's sync pattern) had no
corresponding delete-sync path. An operator who created a mapping by mistake, or one that
should be retired, had no way to remove it — only to create new (superseding) versions, which
doesn't help for a mapping that should never have existed for that source_type at all.

## Decision

Mirror ADR-0109's fix exactly, on the sibling entity:

- New `common::MappingChangeEvent` enum (`Upserted(NormalizationMapping)` / `Deleted { id,
  tenant_id }`), replacing the bare `NormalizationMapping` previously published on
  `mapping.changed` (a breaking wire-format change, accepted since config-admin-service and its
  sole consumer, normalization-service, ship together in this PR).
- `NormalizationMappingRepository::delete` (config-admin-service, audit-logged transaction) and
  `MappingRepository::delete` (normalization-service, simple id-scoped delete).
- `DELETE /v1/normalization-mappings/:id` (operator-gated, actor-attributed), publishing
  `MappingChangeEvent::Deleted` after a successful delete.
- normalization-service's `mapping.changed` consumer now matches on `Upserted` vs `Deleted` and
  calls `upsert`/`delete` on its own repository accordingly, instead of always upserting.
- `NormalizationMappingsClient::delete_mapping` (Console UI) and `POST
  /normalization-mappings/:id/delete` (`normalization_mapping_delete_handler.rs`), with a
  confirm() dialog on a new Remove button — same shape as `post_delete_trigger`.

## Consequences

A Field Mapping's full lifecycle (create, edit-as-new-version, delete) is now available
end-to-end through the Console UI, with proper RBAC gating, audit-log actor attribution, and —
critically — normalization-service's own applied copy staying in sync on delete, not just on
create/update. The `mapping.changed` wire format changed shape (enum instead of bare struct);
any future consumer of that exchange must deserialize `MappingChangeEvent`, not
`NormalizationMapping` directly. Deleting a mapping does not retroactively affect records
already normalized under it — normalization-service simply stops applying that mapping (or, if
older versions of the same source_type still exist, falls back to the highest remaining
version) going forward, consistent with how the existing version-supersession model already
behaves.
