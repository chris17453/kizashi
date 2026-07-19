# ADR-0029: Saved data search queries

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Spec §7 lists "saved queries/views" under Reporting, alongside scheduled PDF/email report
generation. The full Reporting capability is a large, genuinely missing gap (no PDF renderer,
no email-sending scheduler infra exists anywhere in the repo) — out of scope here. "Saved
queries/views" is independently valuable and much smaller: let an operator save a named filter
on the `/data` page's search (`connector_id`, `source_type`, free-text `q`, `subject`,
`email_from`, `attachment_filename`) and revisit it later, without needing PDF/email/scheduling
at all.

`kizashi-ui` owns zero durable state today — no `sqlx`/Postgres dependency anywhere in `ui/`,
every page is a thin proxy/renderer over backend REST calls, and `InMemorySessionStore` is the
only "storage" it has (in-memory, non-durable, and rightly so — session tokens shouldn't
outlive a restart). Two backend homes were considered: `dashboard-api` (despite the name, a
stateless aggregator/proxy with no `sqlx`/migrations at all — same friction as adding a
first-ever DB dependency to `kizashi-ui` itself) and `config-admin-service`, which already has
`sqlx`, a `migrations/` dir, and an established per-tenant-scoped-table pattern (`agents`,
`normalization_mappings`, `trigger_definitions`) the UI already proxies through via its own
client modules.

## Decision

**Saved queries live in `config-admin-service`**, as a new `saved_queries` table
(`id`, `tenant_id`, `name`, `filter JSONB`, `created_at`), mirroring the existing
`agents`/`normalization_mappings` pattern exactly — least friction, no new service, no new
Postgres dependency added to a crate that's never had one.

**No audit log entry for this table**, unlike every other config-admin-service entity. CLAUDE.md
§5's audit-log requirement is scoped to admin/config changes that affect platform behavior
(trigger definitions, mappings, retention policies, RBAC) — a saved search is a personal UI
bookmark with zero effect on the ingestion/normalization/analysis/trigger pipeline. Treating it
as audit-worthy admin config would be scope creep past what the compliance requirement is
actually protecting against.

**No RBAC gating beyond tenant scoping** — unlike trigger/mapping/agent writes (which require
`Operator`+, since they change platform behavior other tenant members depend on), any
authenticated tenant member (including `Viewer`) can save/list/delete their own tenant's saved
queries. A bookmark isn't a privileged action; gating it at `Operator` would block the exact
users (read-only analysts) most likely to want it.

**List/Create/Delete only, no in-place update.** A saved query is a bookmark, not a document —
re-saving under the same or a new name is the natural "edit" workflow; a dedicated update
endpoint would be unused complexity ahead of a real need for it.

## Consequences

- Easier: reuses `config-admin-service`'s entire existing plumbing (pool, migrations,
  tenant-scoped repository pattern, HTTP handler shape) — genuinely additive, no new
  infrastructure. The Console UI's `/data` page gains a small "Saved searches" panel (save
  current filter, click to reload one, delete) without a new page or new client-side JS.
- Harder: saved queries are tenant-wide, not per-user (there's no per-user identity in the
  session yet — the same ADR-0016 limitation `audit_log.rs` already documents) — anyone on the
  tenant can see/delete anyone else's saved search. Acceptable for v1's "team bookmark" framing;
  per-user ownership is a follow-up once user identity flows through sessions. Only `/data`'s
  filter shape is supported — Events-page filters or cross-page saved views are a separate,
  larger effort deferred until there's real demand.
