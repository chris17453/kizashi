# ADR-0182: Live Work queue polling

- Status: Accepted
- Date: 2026-07-25

## Context

My Work presented a live-mode control, but its timer only reloaded the entire
page. That disrupted persisted selection context and gave no indication
whether the queue had actually changed. The authenticated Work API already
provided the tenant-scoped incident and decision data needed for a lightweight
posture check.

## Decision

When live mode is enabled, poll `/api/v1/work` every 30 seconds with the active
text/severity scope and current operator identity. Update the four workload
counters in place. If a counter changes, show a clear “changes detected” state
and leave the detailed queue untouched until the operator chooses Refresh now.
Keep the existing server-rendered queue as the authoritative fallback.

## Consequences

- Operators get non-disruptive awareness of new ownership or review pressure.
- Selection state and active filters are not destroyed by background polling.
- Detailed rows remain internally consistent because changed posture prompts an
  explicit full refresh instead of partial optimistic reconstruction.
