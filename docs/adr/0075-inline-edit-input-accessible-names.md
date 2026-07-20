# 0075. Accessible names on per-row inline-edit inputs

## Context

A fresh accessibility audit found the Retention Policies page's inline TTL edit `<input>` (one
per row, no `<label>`, relying solely on the column header) had no accessible name distinguishing
which row it belonged to — compounded by the fact column headers weren't even programmatically
associated with cells until ADR-0073's `scope="col"` fix. The same pattern exists on the Users
page's inline role `<select>` per row.

## Decision

Both inline-edit controls now carry an `aria-label` naming the specific row they act on:
`aria-label="TTL in days for {{ policy.data_class }}"` and
`aria-label="Role for {{ user.username }}"` — so a screen reader announces which policy or user
the control affects, not just "spin button" or "combo box" with no context. A repo-wide check
found no other unlabeled per-row inline-edit inputs (Sensors, API Keys, and every other list page
either has no inline-edit control or already labels it).

## Consequences

- Purely additive markup, no behavior change.
- Like ADR-0067's disabled-button labels, this is scoped to the two concretely-flagged
  instances, not a claim that every possible accessibility gap sitewide has been swept.
