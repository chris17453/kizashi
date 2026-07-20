# ADR-0103: Error-scrub rollout to dashboard-api, config-admin-service, ingestion-gateway, retention-service

- **Status:** accepted
- **Date:** 2026-07-20

## Context

ADR-0102 fixed three auth-service handlers that passed raw backend error text straight into
HTTP responses, and explicitly scoped out the same pattern in `dashboard-api`,
`config-admin-service`, `ingestion-gateway`, and `retention-service` as tracked follow-up. A
twelfth audit pass confirmed all the sites (with exact file/line citations) and this PR closes
them.

One site was deliberately left alone: `retention-service/src/ops_handlers.rs`'s `trigger_sweep`
and `trigger_reimport`. Both require `X-Internal-Secret` — only the scheduler service (or
another trusted internal caller who already knows the shared secret) can reach them, not any
Console UI user. The "an authenticated end user sees raw SQL error text" threat model this whole
line of fixes addresses doesn't apply there. `trigger_reimport` additionally returns 404 (not
500) on its own failure path using the raw error text as the reason — collapsing that to a
generic message would lose real 404-vs-other-failure semantics for its one caller, and that's a
genuine design question, not a mechanical scrub, so it's left for a future PR to decide
deliberately rather than resolved as a side effect of this sweep.

## Decision

Applied the same fix as ADR-0102 at every remaining site: log the real error via
`tracing::error!`, return the generic `"an internal error occurred; check server logs for
details"` message.

- `dashboard-api/src/handlers.rs`: `list_events`, `get_event`, `daily_event_counts` (3 sites).
- `config-admin-service/src/analysis_config_handlers.rs`: `get_analysis_config`,
  `put_analysis_config`'s existing-key-lookup branch, `put_analysis_config`'s upsert branch (3
  sites). `config-admin-service/src/handlers.rs`: `get_audit_log`, `get_recent_audit_log` (2
  sites).
- `ingestion-gateway/src/api_key_handlers.rs`: `create_api_key`, `list_api_keys`,
  `revoke_api_key` (already had `tracing::error!`, just needed the message swapped),
  `get_api_key_audit_log` (needed both the log call and the scrub — added a
  `FailingAuditLogReader` test double to `audit_log_test.rs` since none existed yet).
- `retention-service/src/policy_handlers.rs`: `get_audit_log`, `get_recent_audit_log` (2 sites).

Every site gained/updated a test asserting the 500 response body does not contain the
`FailingXRepository`'s `"simulated failure"` marker string, following the exact TDD discipline
(red confirmed, then fixed) used for ADR-0102's sites.

## Consequences

- The 12 confirmed audit-log/data-lookup sites across all five services touched by this session's
  audit-driven work (auth-service in ADR-0102, plus these four) no longer leak backend error
  detail to clients.
- `ops_handlers.rs`'s two sites remain open — not a leak given the internal-secret gate, but
  `trigger_reimport`'s 404-vs-500 semantics are worth a deliberate look in a future pass rather
  than blind pattern-matching.
