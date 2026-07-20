# 0095. Sensors bulk-delete and Sessions revoke confirmation

## Context

A seventh UI audit pass found two remaining gaps:

1. API Keys is the only list page with a bulk-select-and-act capability (checkboxes +
   "Revoke selected", ADR-0065). Sensors, Users, and Retention Policies each only support
   removing one row at a time despite having the same per-row destructive action shape — the
   same "one page has a capability, its peers don't" class every prior audit pass has found real
   instances of.
2. Sessions' "Revoke" button was the only destructive action anywhere in the Console UI with no
   `onsubmit="return confirm(...)"` (ADR-0093 covered every other destructive action but missed
   this one) — one misclick force-logs-out another user with no warning.

## Decision

Sensors gained a bulk-delete capability, mirroring API Keys' exact shape: a checkbox per row
(`aria-label="Select {{ sensor.name }} for bulk removal"`), an empty `<form
id="bulk-delete-form">` referenced via each checkbox/button's `form=` attribute (HTML forms can't
nest, and each row still needs its own single-sensor delete/toggle forms), a "Remove selected"
button, and `POST /sensors/bulk-delete` (`post_bulk_delete_sensors`) looping over the existing
single-item `SensorsClient::delete_sensor` — same `parse_ids`-from-raw-body pattern as
`post_bulk_revoke_api_keys`, since `axum::extract::Form` can't collect repeated same-named
fields into a `Vec`. Users and Retention Policies remain one-at-a-time only — this PR scopes to
Sensors as the first of the three, not a promise all three ship together.

Sessions' revoke form gained the missing `onsubmit="return confirm(...)"`, closing the gap
ADR-0093 missed.

## Consequences

- No backend change beyond the new `POST /sensors/bulk-delete` route, which reuses the existing
  `delete_sensor` client method — no new capability was added to `SensorsClient` itself.
- Users' and Retention Policies' bulk-delete remain a follow-up, not addressed here.
- `sensors_handler_test.rs` was split a second time (into `_test.rs`/`_mutations_test.rs`/
  `_pagination_test.rs`) to stay under the 500-line limit after this PR's own additions.
