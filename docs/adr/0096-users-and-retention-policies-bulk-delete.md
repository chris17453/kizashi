# 0096. Users and Retention Policies bulk-delete

## Context

ADR-0095 gave Sensors an equivalent bulk-delete to API Keys' but explicitly scoped Users and
Retention Policies out as a follow-up. This closes that follow-up: both pages had the same
one-at-a-time-only gap.

## Decision

Both pages gained the same bulk-delete shape already established for API Keys/Sensors: a
checkbox per row, an empty `<form id="bulk-delete-form">` referenced via `form=` attributes
(HTML forms can't nest, and each row still needs its own per-row forms — role-select for Users,
edit/toggle for Retention Policies), a "Remove selected" button, and a `POST .../bulk-delete`
handler looping over the existing single-item delete method (`UsersClient::delete_user`,
`RetentionPoliciesClient::delete_policy`) via the same `parse_ids`-from-raw-body pattern every
prior bulk-delete handler uses (`axum::extract::Form` can't collect repeated same-named fields
into a `Vec`).

Users' bulk-delete omits the checkbox for the caller's own row, matching the existing
single-delete button's `disabled`/`aria-label` treatment for self-delete — the backend's own
last-admin/self-delete protections (ADR-0031) still apply per call regardless, this is
presentation-layer only.

## Consequences

- No backend change — both reuse existing single-item delete methods.
- Every list page with a destructive per-row action now has bulk-select parity: API Keys,
  Sensors, Users, Retention Policies.
- `retention_policies_handler_mutations_test.rs` was split again (bulk-delete tests moved to
  their own `_bulk_delete_test.rs` file) to stay under the 500-line limit.
