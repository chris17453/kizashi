# Kizashi Console UI guide and screenshot plan

This is the page inventory for the Kizashi Console. It is written as the script for a product/UI PDF: each row is one screenshot target, explains what the page is for, and names the visible components that should be called out in a caption.

## Capture setup

- URL: `http://localhost:8093`
- Demo workspace: `acme`
- Admin: `demo` / `kizashi-local-demo-password`
- Operator: `operator` / `kizashi-local-operator-password`
- Viewer: `viewer` / `kizashi-local-viewer-password`
- Use a desktop viewport (1440 × 1000), browser zoom 100%, and capture the whole page where possible.
- Start every section with the shared command frame: workspace switcher, global search/command palette, primary navigation, account menu, page title, and role-aware actions.
- Capture list pages with the seeded `acme` data. For an empty-state screenshot, use a new workspace rather than deleting the demo records.

## Shared UI anatomy

Every authenticated page is built from the same shell:

1. **Command frame** — workspace context, global navigation, command/search entry point, and session controls.
2. **Page header** — title, short operational context, primary action, and role-aware controls.
3. **Summary layer** — status cards, counts, KPI tiles, or health indicators when the page has meaningful aggregate state.
4. **Work surface** — table, graph, timeline, detail record, or compose form.
5. **Controls** — search, filters, saved views, bulk actions, pagination, export, and destructive-action confirmation where applicable.

## Access and session

| Screenshot target | Route | What it does | Key components |
|---|---|---|---|
| Sign in | `/login` | Local workspace sign-in and SSO entry point. | Workspace, username, password fields; local sign-in; SSO provider selector; error state. |
| MFA challenge | `/login/mfa` | Completes a challenged local sign-in. | One-time-code input, verification action, recovery/error state. |
| Workspace switch | `/workspace/switch` | Changes the active tenant/workspace. | Workspace chooser, current-workspace indicator, confirmation. |
| Session context | `/session/context` | Shows who is signed in and their effective role/workspace. | Identity summary, role badge, workspace data. |
| Security overview | `/security` | Security posture hub. | Posture cards, MFA/SSO/session links, compliance and backup entry points. |
| Permissions reference | `/security/permissions` | Explains what each role can do. | Role matrix, capability groups, navigation guidance. |
| MFA settings | `/security/mfa` | Enrolls, verifies, or disables MFA for the signed-in user. | Enrollment secret/QR state, code form, disable action. |
| Password settings | `/security/password` | Changes a local password. | Current/new-password fields, policy hints, success/error state. |
| Active sessions | `/security/sessions` | Audits and revokes sessions. | Session table, device/session metadata, single and bulk revoke controls. |
| Login attempts | `/security/login-attempts` | Investigates successful and failed local/MFA sign-ins. | Search/filter controls, attempts table, CSV export. |
| Backups | `/security/backups` | Reviews backup history and starts a backup. | Backup status cards/table, retention metadata, run-backup action. |
| Compliance report | `/security/compliance-report` | Presents compliance-oriented security evidence. | Report sections, posture findings, export/print-ready content. |

## Operations and response

| Screenshot target | Route | What it does | Key components |
|---|---|---|---|
| Overview | `/overview` | Landing dashboard for current operational posture. | KPI cards, configurable dashboard layout, recent/attention widgets, saved views. |
| Attention summary | `/work/summary` | Prioritizes items that need action. | Severity/count summary, attention queue links, ownership cues. |
| Work queue | `/work` | Lets operators claim and process work. | Queue table, filters, saved views, claim and bulk-claim actions, CSV export. |
| Workflow exceptions | `/workflows` | Reviews pipeline reconciliation failures and approval requests. | Status/kind filters, SLA/due indicators, case list, approve/reject/resolve decision controls. |
| Events | `/events` | Searches and triages ingested events. | Query/filter bar, saved views, event table, bulk status, incident creation/linking, CSV export. |
| Event detail | `/events/:id` | Investigates a single event. | Event payload/metadata, status history, linked incidents, 360 context, action controls. |
| Incidents | `/incidents` | Creates, filters, and coordinates incident response. | Incident table, filters/saved views, create form, bulk update, export. |
| Incident detail | `/incidents/:id` | Manages one incident end to end. | Status/owner controls, notes timeline, linked events, claim action, AI brief, export. |
| Correlation sweep | `/incidents/correlation-sweep` | Finds candidate event-to-incident correlations. | Candidate list, confidence/context, auto-correlate and resolve controls. |
| Actions | `/actions` | Monitors action executions requiring review or retry. | Status table, filters/saved views, bulk retry/review, dead-letter replay, CSV export. |
| Action detail | `/actions/:id` | Inspects a single action execution. | Execution state, request/result context, review decision, 360 links. |
| Action library | `/actions/library` | Maintains reusable action definitions. | Definition table, create/edit form, delete confirmation. |
| Action templates | `/action-templates` | Maintains parameterized action templates. | Template list, create/edit form, version/content fields, delete control. |
| Triggers | `/triggers` | Defines and enables event-driven automation. | Trigger table, create form, enabled toggle, bulk toggle. |
| Trigger detail | `/triggers/:id` | Edits and tests one trigger. | Condition/configuration editor, test result, enable/delete actions. |
| Pipeline health | `/pipeline` | Shows the ingestion-to-processing operational pipeline. | Stage health, throughput/lag/error indicators, service links. |
| Platform health | `/health` | Shows service availability. | Service health cards/table, dependency status, operational timestamps. |
| Audit log | `/audit-log` | Searches changes across the platform. | Audit timeline/table, service/entity filtering, entity-detail drill-down, CSV export. |
| Entity audit trail | `/audit-log/:service/:entity_id` | Shows an immutable record of one entity's changes. | Ordered history, actor/time/action fields, before/after context. |

## Data, models, and build studio

| Screenshot target | Route | What it does | Key components |
|---|---|---|---|
| Build Studio | `/build` | Entry point for configuring data sources and pipelines. | Studio overview, Data Sources/Pipelines links, explanatory cards. |
| Data Sources | `/build/data-sources` | Lists and creates declarative external data-source definitions. | Source table, connection/type/status fields, create action, empty state. |
| New data source | `/build/data-sources/new` | Creates a source definition. | Name/type/connection/configuration form, validation hints, save/cancel. |
| Data source detail | `/build/data-sources/:id` | Reviews and versioned-edits a source. | Definition metadata, configuration display, version field, update form. |
| Pipelines | `/build/pipelines` | Lists and creates reconciliation pipeline definitions. | Pipeline table, source/target/status fields, create action, empty state. |
| New pipeline | `/build/pipelines/new` | Creates a pipeline definition. | Name, source, mapping/reconciliation configuration, validation, save/cancel. |
| Pipeline detail | `/build/pipelines/:id` | Reviews and versioned-edits a pipeline. | Definition metadata, mapping/config display, version field, update form. |
| Ontology / Models | `/ontology` | Manages the business model and its records. | Object-type panel, link-type panel, object table/graph, search, bulk actions, saved views. |
| Object 360 | `/ontology/objects/:id/360` | Gives a contextual view of one modeled object. | Properties, lineage, annotations, related objects/links, history. |
| Model compare | `/ontology/compare` | Compares model/object data across a selected scope. | Side-by-side comparison, filters, differences/highlights, export context. |
| Data records | `/data` | Searches processed records and sends them to reprocess/model flows. | Query/filter controls, records table, saved searches, bulk reprocess/model, CSV export. |
| Record detail | `/data/:id` | Inspects one processed record. | Canonical record data, status, provenance, individual reprocess/model actions. |
| Record journey | `/data/:id/journey` | Traces a record through ingestion and processing. | Chronological journey/timeline, stage metadata, linked errors/actions. |
| Data compare | `/data/compare` | Compares selected data records. | Selection context, side-by-side fields, difference indicators. |
| Event types | `/event-types` | Defines versioned event contracts. | Contract table, schema editor, create-version action, validation messages. |
| Sensors | `/sensors` | Registers and administers data collection sensors. | Sensor table, registration form, status toggle, bulk delete. |
| Sensor detail | `/sensors/:id` | Edits one sensor. | Configuration, generated script/download guidance, enable/delete actions. |
| Sensor script generator | `/sensors/generate` | Guides creation of a sensor integration script. | Source/type selection, generation form, generated-script output. |
| Normalization mappings | `/normalization-mappings` | Maps source fields into normalized data. | Mapping table, mapping editor, create/edit/delete controls. |
| Retention policies | `/retention-policies` | Controls retention and legal-hold behavior. | Policy table, create/edit/toggle/delete, bulk delete, reimport, holds. |
| Egress allowlist | `/egress-allowlist` | Governs approved outbound destinations. | Allowlist entries, policy form, validation, save state. |
| Analysis configuration | `/analysis-config` | Configures analytical processing behavior. | Configuration fields, policy/help text, save feedback. |

## Apps, reports, and discovery

| Screenshot target | Route | What it does | Key components |
|---|---|---|---|
| Apps | `/apps` | Creates and lists declarative internal apps. | App cards/list, compose form, title/description/definition fields, empty state. |
| App detail | `/apps/:id` | Renders an app from its declared blocks and edits its definition. | Rendered form/table/upload/queue/dashboard/detail blocks, versioned definition editor, update action. |
| Reports | `/reports` | Builds operational report views. | Report summary, filters/saved views, chart/table region, CSV/PDF export. |
| Report schedules | `/reports/schedules` | Creates and operates scheduled reports. | Schedule table, create/edit form, enabled toggle, run-now and delete controls. |
| Search | `/search` | Searches across platform entities. | Global query input, grouped results, filters, saved views. |
| Configuration | `/configuration` | Provides a configuration hub. | Category links, current-state summaries, administrative entry points. |
| Branding | `/branding` | Configures workspace presentation. | Product name/logo/accent controls, live-preview context, save feedback. |

## Identity and access administration

| Screenshot target | Route | What it does | Key components |
|---|---|---|---|
| Users | `/users` | Administers local users and roles. | User table, create form, role controls, bulk role/delete, data-subject export. |
| User detail | `/users/:id` | Reviews a specific user. | Identity/role metadata, user-scoped controls, audit/privacy export link. |
| Service accounts | `/service-accounts` | Creates and revokes service identities. | Service-account table, create form, generated credential handling, revoke action. |
| API keys | `/api-keys` | Issues and revokes ingestion/API credentials. | Key table, create form, one-time key reveal state, single/bulk revoke. |

## Suggested PDF order

1. Cover: Console overview and command frame.
2. Sign-in and workspace identity.
3. Overview, attention, work queue, and workflow exceptions.
4. Events, incidents, actions, and triggers.
5. Build Studio: data sources, pipelines, Ontology/Models, and data records.
6. Apps, reports, and search.
7. Security, users, service accounts, API keys, and audit evidence.
8. Appendix: configuration, sensors, retention, and platform health.

For the PDF, use the list pages as the primary images and add detail/create pages only where they introduce a distinct component pattern. Do not screenshot mutation endpoints (for example `.../delete`, `.../toggle`, or `.../export.csv`); their controls belong in the parent page screenshot.
