# Kizashi Console UI specification

**Status:** canonical product/UI contract

This document is the single source of truth for the Console's human-facing pages. The router and templates implement this contract; screenshots and the PDF are presentation artifacts, not the specification. Mutation-only URLs, CSV/PDF exports, and JSON API routes are deliberately excluded unless they change a visible page.

## Product frame

Kizashi is an operational intelligence console. A user must be able to move from a signal, to its modeled business context, to a decision or action, without losing workspace, role, ownership, status, or evidence.

Every authenticated page must answer four questions immediately:

1. **Where am I?** — page title, breadcrumb/context, active navigation item.
2. **What needs attention?** — counts, health, SLA, severity, failures, or a clear empty state.
3. **What can I do?** — one obvious primary action plus role-appropriate secondary actions.
4. **What happened and why?** — timestamps, actors, source/provenance, status history, and links to related records.

## Global shell and visual language

All authenticated pages use the command frame.

- **Left navigation:** grouped by Operations, Data & Pipeline, Build, and Configuration. It shows the active location and may show attention counts.
- **Top command bar:** workspace and user/role identity, global search, command palette, live/refresh indicator, theme toggle, and session controls.
- **Page header:** eyebrow/category, title, one-sentence purpose, current scope/filter summary, and the primary action aligned right.
- **Work surface:** a max-width content area containing summary cards followed by the page's table, canvas, detail view, or form.
- **Evidence and safety:** destructive actions require confirmation; state-changing controls show the result; records display identity, status, owner/actor, time, and source where available.

### Visual rules

- Default theme is dark operational UI: near-black canvas, elevated charcoal panels, thin muted borders, cyan/teal for active and constructive controls, amber for attention, red only for destructive/failed state, and green only for healthy/success state.
- Use a compact mono/technical treatment for IDs, routes, timestamps, versions, and machine state. Use a highly legible sans-serif face for titles, labels, and narrative content.
- Tables prioritize scanability: sticky or clear headers, restrained row separators, status badges, no dense paragraph cells, and row-level drill-down.
- Do not use color as the only meaning. Every severity/status color has text and, where useful, an icon.
- Empty states state what is absent, why it matters, and provide the next safe action. Error states retain context and state a recovery path.
- Responsive behavior must preserve task completion: navigation may collapse, tables may scroll horizontally or become cards, and primary actions remain reachable.

## Shared component contracts

| Component | Required behavior |
|---|---|
| Status badge | Text label plus semantic color; supports healthy, pending, warning, failed, disabled, and terminal states. |
| Filter/query bar | Preserves filter values in the URL where applicable; clear/reset is always available. |
| Saved view | Saves the current query/filter/layout under a human name; list pages expose apply, rename, and delete. |
| Data table | Shows loading, empty, error, and populated states; supports pagination or a stated bounded result set. |
| Detail header | Shows record name/identifier, current status, metadata, and the record's primary transition/action. |
| Timeline/history | Ordered newest-first by default, with actor, timestamp, action, and useful before/after evidence. |
| Versioned editor | Shows current version, requires it on update, and warns on stale/conflicting edits. |
| Permission gate | Hidden or disabled actions explain the required role; server-side authorization remains authoritative. |
| Export | Is presented as a secondary action, names the format, and reflects the current scope/filter. |

## Roles

| Role | Intended experience |
|---|---|
| Viewer | Inspect dashboards, records, evidence, and permitted exports; cannot mutate operational configuration. |
| Operator | Triage work, claim/resolve operational items, execute permitted reviews, and manage operational resources. |
| Admin | Configure workspace/platform resources, manage identities and security policy, and perform all permitted operator actions. |

## Page specifications

### Access, workspace, and security

| Page | Route | Required content and behavior |
|---|---|---|
| Sign in | `/login` | Workspace, username, and password inputs; local sign-in as the primary action; SSO provider entry as a clear alternative; concise credential/error feedback; no authenticated shell. |
| MFA challenge | `/login/mfa` | One-time-code input, verify action, recovery/error message, and the prior workspace/user context. |
| Workspace switch | `/workspace/switch` | Current workspace, available workspace choices, explicit confirmation, and clear post-switch destination. |
| Session context | `/session/context` | Signed-in user, workspace, effective role, session metadata, and safe sign-out/session-management links. |
| Security overview | `/security` | Security posture summary; MFA, SSO, active session, login-attempt, backup, compliance, and permissions entry points; unresolved risks are prominent. |
| Permissions reference | `/security/permissions` | A readable role-capability matrix grouped by functional area; no secret implementation detail. |
| MFA settings | `/security/mfa` | Enrollment state; QR/secret only while enrolling; verify and disable flows; recovery guidance and confirmation for disable. |
| Password settings | `/security/password` | Current password, new password, confirmation, visible policy guidance, success state, and no password echo. |
| Active sessions | `/security/sessions` | Session/device table with actor, created/last-used time, and current-session marker; individual and bulk revoke with confirmation. |
| Login attempts | `/security/login-attempts` | Search/filterable attempts table, outcome/reason/status, time and user columns, and scoped CSV export. |
| Backups | `/security/backups` | Backup health/status, history, retention facts, and admin-only run-now action with start/result feedback. |
| Compliance report | `/security/compliance-report` | Print-friendly compliance evidence grouped by control/outcome; timestamps and source links; no raw secrets. |

### Overview, work, and response

| Page | Route | Required content and behavior |
|---|---|---|
| Overview | `/overview` | Configurable dashboard of current health, attention, activity, and work; widgets have labels, time scope, empty/error states, and drill-down destinations. |
| Attention summary | `/work/summary` | Severity/urgency summary and direct links to the queues that explain each count. |
| Work queue | `/work` | Claimable work table with type, priority, age/SLA, owner, status, filters, saved views, individual/bulk claim, and export. |
| Workflow exceptions | `/workflows` | Pending review/approval/reconciliation cases with kind, linked execution, age/due time, decision history, evidence, and approve/reject/resolve controls limited by role. |
| Events | `/events` | Searchable event stream with time, type, source, severity, status, and linked incident; supports saved views, bulk status, create/link incident, and export. |
| Event detail | `/events/:id` | Header with type/time/status; canonical payload and metadata; status history; related incidents; provenance/360 links; safe status and incident actions. |
| Incidents | `/incidents` | Incident list with severity, status, owner, event count, age, filters, saved views, create, bulk update, and export. |
| Incident detail | `/incidents/:id` | Status/ownership header, event links, notes/activity timeline, claim and transition controls, AI brief state, export, and 360 context. |
| Correlation sweep | `/incidents/correlation-sweep` | Candidate correlations with confidence/evidence, reviewed state, and explicit auto-correlate or resolve actions. |
| Actions | `/actions` | Executions list with action type, target, state, retry/review state, time, and failure context; supports filters, saved views, bulk retry/review, dead-letter replay, export. |
| Action detail | `/actions/:id` | Action definition/version, request target, execution output/error, attempts, review decision, related records, and allowable retry/review action. |
| Action library | `/actions/library` | Reusable action definitions with descriptive list, create/edit form, version/configuration context, and guarded delete. |
| Action templates | `/action-templates` | Parameterized reusable templates with template body/schema, create/edit/delete, validation, and useful empty state. |
| Triggers | `/triggers` | Trigger list with event condition, target action, enabled state, last activity, create, individual/bulk toggle, and detail drill-down. |
| Trigger detail | `/triggers/:id` | Readable condition/action definition, current state, test control and result, edit, enable/disable, and guarded delete. |
| Pipeline map | `/pipeline` | End-to-end processing health: each stage, throughput/lag/error condition, current status, and links to affected resources. |
| Platform health | `/health` | Service/dependency health with status, last check, impact context, and no misleading all-green state when a dependency is degraded. |
| Audit log | `/audit-log` | Cross-service, filterable activity history with time, actor, entity, action, and drill-down/export. |
| Entity audit trail | `/audit-log/:service/:entity_id` | A scoped immutable timeline of one entity with before/after evidence where available. |

### Data, pipeline, and model

| Page | Route | Required content and behavior |
|---|---|---|
| Build Studio | `/build` | Landing page for declarative configuration. It contains separate Data Sources and Pipelines panels, item counts, short explanations, and primary manage/create paths. Models is a visible related destination. |
| Data Sources | `/build/data-sources` | List of source definitions with name, type, connection/status, version, and updated time; create action; empty state explains that sources power pipelines. |
| New data source | `/build/data-sources/new` | Focused create form for name, source type, connection/configuration, and validation; save/cancel; no unrelated dashboard clutter. |
| Data source detail | `/build/data-sources/:id` | Readable definition summary and versioned edit form; exposes source type/configuration safely and links to dependent pipelines when known. |
| Pipelines | `/build/pipelines` | List of pipeline definitions with source/target, state, reconciliation behavior, version, and update time; create action and clear empty state. |
| New pipeline | `/build/pipelines/new` | Focused form for identity, source, mapping/configuration, review/reconciliation behavior, validation, save/cancel. |
| Pipeline detail | `/build/pipelines/:id` | Definition detail, mapping/reconciliation configuration, versioned update form, and linked execution/failure context where available. |
| Ontology / Models | `/ontology` | The model workbench: object types, link types, objects, and actions; graph/list surfaces; search/filter/saved views; creation and bulk tools; the page labels the domain as Models in navigation. |
| Object 360 | `/ontology/objects/:id/360` | Object identity/properties, source lineage, annotations, relationships, history, and actions in one evidence-oriented detail view. |
| Model compare | `/ontology/compare` | Side-by-side comparison with selected scope, clear differences, and links back to underlying modeled records. |
| Data records | `/data` | Processed-record list with search/filter/saved searches, canonical status/provenance, bulk reprocess/model actions, and export. |
| Record detail | `/data/:id` | Canonical content, status, provenance, related model/event context, and individual reprocess/model controls. |
| Record journey | `/data/:id/journey` | A chronological stage-by-stage journey for one record, including times, outcomes, errors, and linked artifacts. |
| Data compare | `/data/compare` | Side-by-side record comparison with selection context and unambiguous difference treatment. |
| Event types | `/event-types` | Versioned event contract list and schema editor; creation/versioning validates shape and shows version/history clearly. |
| Sensors | `/sensors` | Sensor inventory with state, source/type, recent activity, registration, toggle, bulk deletion, and detail drill-down. |
| Sensor detail | `/sensors/:id` | Sensor identity/configuration/state, script/integration guidance, edit/toggle/delete controls, and failure context. |
| Sensor script generator | `/sensors/generate` | Stepwise source/type selection, generation inputs, generated script output, copy/download affordance, and setup guidance. |
| Normalization mappings | `/normalization-mappings` | Mapping inventory and editor showing source field, target field, transform/rule, validation, and guarded mutation controls. |
| Retention policies | `/retention-policies` | Policy list with scope, duration, state, holds, and lifecycle controls; separate legal-hold and reimport actions are visibly high-risk. |
| Egress allowlist | `/egress-allowlist` | Clear policy explanation, allowed destinations, validation of destination format, and save feedback. |
| Analysis configuration | `/analysis-config` | A bounded configuration form with defaults/help text, validation, save feedback, and auditability. |

### Apps, reports, discovery, and administration

| Page | Route | Required content and behavior |
|---|---|---|
| Apps | `/apps` | App catalog and composer. Lists declarative apps with title/description/version and provides a create form; clearly explains that apps are composed from supported blocks, not arbitrary scripts. |
| App detail | `/apps/:id` | Renders declared form, table, upload, queue, dashboard, and detail blocks in a coherent app surface; includes a versioned definition editor for authorized users. |
| Reports | `/reports` | Report summary with filters/saved views, visual/table result area, clear time scope, and CSV/PDF export of the current scope. |
| Report schedules | `/reports/schedules` | Schedule inventory with report, recipient/destination, cadence, next/last run, enabled state, create/edit, run-now, toggle, and delete. |
| Search | `/search` | Global query page with grouped cross-entity results, result type, snippets/context, filters, saved views, and direct record links. |
| Configuration | `/configuration` | Administrative hub that organizes configuration areas and exposes concise current-state summaries; it is not a duplicate of every form. |
| Branding | `/branding` | Workspace product name, logo, and accent controls; preview/impact context, validation, and explicit save feedback. |
| Users | `/users` | User list with identity, role, state, create, role change, bulk operations, user detail, and privacy/data-subject export. |
| User detail | `/users/:id` | Identity and authorization facts, permitted role management, relevant audit/privacy links, and no credential disclosure. |
| Service accounts | `/service-accounts` | Service identity inventory, create flow, one-time credential reveal, scope/metadata, and guarded revoke. |
| API keys | `/api-keys` | Credential inventory with label, creation/last-use/revocation state; create reveals a secret once only; supports individual/bulk revoke. |

## Required page states

Each list, detail, and editor must implement the following where applicable:

- **Loading:** stable skeleton or progress treatment that does not shift the page frame.
- **Empty:** explanation, visual restraint, and the primary safe creation/import action.
- **Error:** preserve user input/current context, identify whether retry is safe, and give a retry or support path.
- **Unauthorized:** do not expose protected data; explain access at the page level without leaking sensitive resource details.
- **Stale version/conflict:** preserve the submitted edit, show the competing version, and offer reload/copy guidance.
- **Success:** confirm the exact completed operation and leave the user at the most useful follow-up state.

## Acceptance criteria

A page is complete only when it satisfies its row above, uses the shared shell and component contracts, has meaningful empty/error/permission states, and has a screenshot in the current UI PDF when it is a primary destination. New routes must be added to this document in the same change as their UI implementation.
