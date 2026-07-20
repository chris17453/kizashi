# 0092. Audit-history links on Branding and AI Analysis pages

## Context

The sixth UI audit pass found `branding.html` and `analysis_config.html` had no link to their
own change history, unlike every other mutable config page (Sensors, Users, Triggers,
Retention Policies, Field Mappings all link to `/audit-log/:service/:entity_id`). Both writes
were already audited on the backend — `auth-service`'s `tenant_branding_repository.rs` logs
every branding update with `entity_id: tenant_id` (ADR-0039), and `config-admin-service`'s
`analysis_config_repository.rs` does the same for analysis config — the gap was purely that the
UI never surfaced a link to view it.

## Decision

Both `BrandingTemplate` and `AnalysisConfigTemplate` gained a `tenant_id: Uuid` field, threaded
through every construction site the same way `is_admin` already is (ADR-0090). Each page renders
a "View change history" link: branding to `/audit-log/auth/{{ tenant_id }}` (branding lives in
auth-service, per `ui/src/audit_log_handler.rs`'s existing `"auth" => &state.auth_audit_log_client`
routing), analysis config to `/audit-log/config/{{ tenant_id }}` (config-admin-service). Both
routes and audit-log clients already existed — this is purely additive template/handler wiring,
no new backend endpoint.

## Consequences

- No backend change.
- API Keys' per-key audit history remains unaddressed: `ingestion-gateway`'s API key audit log
  (`GET /v1/api-keys/:id/audit-log`) has no corresponding entry in
  `ui/src/audit_log_handler.rs`'s three-way `config`/`retention`/`auth` service switch, so even
  adding a link on `api_keys.html` wouldn't resolve today — that's a separate, larger follow-up
  (a new `ingestion-gateway`-scoped audit client plus a fourth switch arm), not fixed here.
