# 0094. API Key per-key audit history link

## Context

ADR-0092 closed the audit-history-link gap for Branding and AI Analysis, but flagged API Keys as
a separate follow-up: `ingestion-gateway` already writes an audit_log row on every create/revoke
(`api_keys.html` even claims this in its copy), and already exposes it via
`GET /v1/api-keys/:id/audit-log` — but that route doesn't match the shared
`GET /v1/audit-log/:entity_id` shape `HttpAuditLogClient` expects (used by config-admin-service,
retention-service, and auth-service), and `ingestion-gateway` has no tenant-wide "recent
activity" feed of its own. So the existing `AuditLogClient`/`HttpAuditLogClient` pair couldn't be
reused as-is, and `ui/src/audit_log_handler.rs`'s `service` switch had no fourth arm for it.

## Decision

New `IngestionGatewayApiKeyAuditLogClient`, a second implementation of the existing
`AuditLogClient` trait: `list_for_entity` calls the real `GET /v1/api-keys/:id/audit-log` shape,
`list_recent` returns an error (not a panic) since nothing routes there — `ingestion-gateway` has
no global feed to serve. Wired into `AppState` as `ingestion_audit_log_client`, and
`audit_log_handler.rs`'s `service` switch gained `"ingestion" => &state.ingestion_audit_log_client`.
`api_keys.html` gained a per-row "History" column linking to `/audit-log/ingestion/{{ key.id }}`,
matching the pattern already used on Sensors/Users/Triggers/Retention Policies/Field Mappings.

## Consequences

- No new backend endpoint — `ingestion-gateway`'s `GET /v1/api-keys/:id/audit-log` already
  existed (CLAUDE.md §5 compliance was already satisfied at the API layer; only the Console UI
  couldn't reach it).
- `AuditLogClient::list_recent` is no longer safe to assume "works for every implementor" — a
  future caller must check which concrete client it's holding before calling `list_recent`
  blindly across all of `AppState`'s four `Arc<dyn AuditLogClient>` fields.
- This closes every "backend audits it, UI never linked to it" gap the sixth audit pass found —
  Sensors/Users/Triggers/Retention Policies/Field Mappings/Branding/AI Analysis/API Keys all now
  link to their own history.
