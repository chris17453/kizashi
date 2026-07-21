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
substantial feature in its own right. Scoping all of that into one PR would be too large a unit
of work to TDD, review, and live-verify safely. This ADR scopes an MVP: manual incident
creation and lifecycle management, with auto-correlation and dedup explicitly deferred to
follow-up ADRs once the core entity and UI exist to build on.

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

**Explicitly deferred** (separate future ADRs): rule-based auto-correlation (group_by on
TriggerDefinition or a new CorrelationRule attaching matching Events to an open Incident
automatically), alert fingerprint/dedup ahead of trigger evaluation, AI-generated incident
summaries, and a Providers-style marketplace reskin of the Sensors page.

## Consequences

Operators get a real place to track "this is one ongoing problem" instead of manually
cross-referencing the flat Events table — the single biggest gap identified against Keep. A new
service means new deployment surface (docker-compose entry, migrations, `scripts/run-local.sh`
wiring) but keeps the same operational shape every other service already has, so it costs no
new operational patterns to learn. Manual-only correlation in this MVP means an operator still
has to notice related Events and group them by hand; auto-correlation is the natural, valuable
next increment once this foundation exists.
