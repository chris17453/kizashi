# 0065. API Keys bulk revoke

## Context

A UI/UX audit found no list page anywhere in the Console UI supports a bulk action — every
destructive/state-changing action is one row at a time, even when an operator plausibly wants to
revoke several stale keys at once (e.g. rotating an entire batch of connector credentials). API
Keys is a natural first candidate: revoke is already a single, idempotent, per-row action with no
side effects beyond the row itself.

## Decision

`POST /api-keys/bulk-revoke` accepts one or more `ids` values (checkbox per active row) and loops
over the existing single-item `ApiKeysClient::revoke_api_key` call for each selected id — no new
bulk backend endpoint. A handful of sequential revoke calls triggered by one admin click is not a
real performance concern at this scale, and reusing the existing call keeps the audit-log write
path (one entry per revoke, per ADR-0016) unchanged. Best-effort per key, matching the existing
single-revoke handler: one key failing to revoke (e.g. already revoked) doesn't stop the rest.
An empty selection is a legitimate no-op, not an error.

Template-side, since HTML forms cannot nest and each row already has its own single-revoke
`<form>`, the bulk form is declared empty outside the table and every checkbox/the submit button
references it via the HTML5 `form="bulk-revoke-form"` attribute rather than restructuring the
per-row forms.

Handler-side, `axum::extract::Form` could not be used directly: it deserializes via
`serde_urlencoded`, which does not support collecting repeated same-named form fields (one
checkbox per row, all named `ids`) into a `Vec<T>`-typed struct field. The handler instead takes
the raw request body (`axum::body::Bytes`) and parses it with
`serde_urlencoded::from_bytes::<Vec<(String, String)>>`, which supports flat key-value pair lists,
filters for `key == "ids"`, and parses each value as a `Uuid`. No new dependency was needed —
`serde_urlencoded` is already a direct dependency of the `ui` crate.

## Consequences

- Scoped to API Keys only for now. The same pattern (empty external form + `form=` attribute on
  checkboxes, manual `serde_urlencoded::from_bytes` parsing for repeated fields) is a direct
  template for adding bulk actions to Users or Sessions later if that proves valuable.
- RBAC-gated identically to the existing single-revoke action: `Operator` and above only,
  enforced both by hiding the checkboxes/button in the template for a `Viewer` and by a
  server-side role check in the handler (403 otherwise) — the template hiding is presentation
  convenience, not the only gate, same convention as every other write path in this UI.
