# ADR-0108: Trigger Enable/Disable Toggle

- **Status:** accepted
- **Date:** 2026-07-20

## Context

An audit pass comparing Console UI's config-admin list pages for feature parity found that
Trigger Definitions was the odd one out: Sensors and Retention Policies both let an operator
disable/edit/delete a row in place, but the Triggers page only ever supported list + create +
test-dry-run. Once created, a trigger could only be disabled or edited via a raw API call
straight to config-admin-service — bypassing the Console UI's session/RBAC layer entirely and,
more importantly, its audit-log actor attribution (`X-Username`), which only the UI's session
forwards. Given `TriggerDefinition` already has the same `enabled: bool` field the other two
entities expose as a toggle, and config-admin-service's `PUT /v1/trigger-definitions/:id`
(`update_trigger`) already exists, already audit-logs the change, and already enforces
operator-only writes, no new backend surface was needed to close this gap.

## Decision

Add `GET /v1/trigger-definitions/:id` and `PUT /v1/trigger-definitions/:id` calls to
`TriggersClient` (`get_trigger`/`update_trigger`), and a new `POST /triggers/:id/toggle` route
in `kizashi-ui` (`trigger_toggle_handler.rs`) that fetches the current definition, flips
`enabled`, and PUTs the whole record back — the same fetch-flip-PUT shape already used by
`post_toggle_retention_policy`. The Triggers table gains a Disable/Enable button per row,
gated behind `can_write` (Operator+) exactly like the equivalent column on Retention Policies
and Sensors.

Full condition/action editing and delete are deliberately out of scope for this ADR: the
trigger condition DSL (count/threshold/correlated shapes) is a materially bigger editor UI than
a single boolean flip, and config-admin-service has no `DELETE /v1/trigger-definitions/:id`
endpoint at all yet — both remain open backlog items, tracked separately rather than folded
into this narrower, immediately actionable fix.

## Consequences

Operators can now disable a misbehaving or accidentally-created trigger from the Console UI,
with the action properly RBAC-gated and audit-logged under the real actor's username, instead
of needing direct API access that skips both of those controls. Triggers now has toggle parity
with its two structurally-similar peer pages. Full edit/delete for triggers remain follow-up
work, gated on either a condition-DSL editor UI or a new backend delete endpoint depending on
which is tackled first.
