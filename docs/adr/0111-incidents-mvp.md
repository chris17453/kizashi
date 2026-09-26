# ADR-0111: Incidents MVP

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A structured comparison against Keep (keephq/keep), another AIOps/incident platform, identified
Kizashi's single largest feature gap: Keep groups multiple related alerts into a distinct
`Incident` entity (severity, status, assignee, timeline, linked alerts), while Kizashi's Events
are flat — one row per trigger fire, with no way to represent "these five events are all the
same underlying problem." Operators investigating a real incident today have to manually
correlate Events by eye across the flat Events table.

Full parity with Keep's Incidents feature includes rule-based auto-correlation, AI-generated
summaries, and alert deduplication/fingerprinting ahead of trigger evaluation — each a
substantial feature in its own right. The original MVP shipped the core entity and lifecycle
management first; follow-up slices now provide operator-assisted safe correlation, governed AI
briefs, and upstream deduplication telemetry without weakening the audited case boundary.

## Decision

**New service: `incident-service`**, following the same per-service-owned-Postgres-schema
pattern already established by retention-service/action-executor/config-admin-service (ADR-0010)
— Incidents need writes, status lifecycle, and audit logging, which don't fit
`dashboard-api`'s existing read-only ClickHouse-backed shape. Console UI calls it directly with
`X-Tenant-Id`/`X-Role`/`X-Username` headers, the same trust boundary already used for
config-admin-service (no gateway in front of it).

**Entity shape:**
```
Incident { id, tenant_id, title, summary, severity (Low/Medium/High/Critical),
           status (Open/Acknowledged/Resolved), created_at, updated_at, resolved_at }
IncidentEvent { incident_id, event_id, linked_at }  -- many-to-many join, Event stays owned
                                                        by trigger-engine/ClickHouse; this table
                                                        only stores the association
```

**API (operator-gated writes, audit-logged create/update/link/unlink, matching
config-admin-service's audit pattern):**
- `POST /v1/incidents` — create (title, severity, optional initial `event_ids` to link)
- `GET /v1/incidents` — list, tenant-scoped, filterable by status
- `GET /v1/incidents/:id` — detail, includes linked `event_ids`
- `PUT /v1/incidents/:id` — update title/summary/severity/status
- `POST /v1/incidents/:id/events` — link event(s)
- `DELETE /v1/incidents/:id/events/:event_id` — unlink

**Console UI:**
- `GET /incidents` — list page (title, severity, status, linked-event count, created_at)
- `GET /incidents/:id` — detail page (metadata, status controls, linked Events reusing the
  existing Event row/link rendering, unlink action)
- `POST /incidents` — minimal create form (title + severity)
- Events page gains a checkbox column + "Create Incident from Selected" bulk action — the
  natural trigger for incident creation and the same bulk-select UI pattern already used on
  Sensors/API Keys bulk-delete, reused here for a create rather than a delete.

**Still deferred** (separate future ADRs): partial-duplicate-as-update semantics and richer
multi-match correlation policies. Operator-assisted correlation, governed AI briefs, alert
fingerprint suppression, and the Sensors marketplace have shipped as follow-up slices. The
incident service now consumes `event.created` for the narrow, safe case of exactly one active
tenant-scoped match; see ADR-0116.

## Consequences

Operators get a real place to track "this is one ongoing problem" instead of manually
cross-referencing the flat Events table — the single biggest gap identified against Keep. A new
service means new deployment surface (docker-compose entry, migrations, `scripts/run-local.sh`
wiring) but keeps the same operational shape every other service already has, so it costs no
new operational patterns to learn. The event-driven consumer deliberately ignores blank,
ambiguous, and resolved matches, leaving those cases for operator review and the existing
previewed correlation workflow.
