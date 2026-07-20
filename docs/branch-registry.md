# Branch Registry

Tracks every branch ever created, by type, with an auto-incrementing per-type counter. This is
the single source of truth for the next branch number — never guess or reuse a number, always
read the counter below, use it, then increment it in the same commit that creates the branch.

Managed by `scripts/new-branch.sh` (see CLAUDE.md §6/§8) — use that script rather than hand-editing
counters where possible, but the table itself is the audit record and must stay in sync even for
manually created branches.

## Counters (next number to use, per type)

| Type          | Prefix     | Next number |
|---------------|------------|-------------|
| feature       | `feature/` | 0055         |
| fix           | `fix/`     | 0008         |
| debug         | `debug/`   | 0001        |
| docs          | `docs/`    | 0003         |
| chore         | `chore/`   | 0004         |

## Branch log (append-only, newest last)

| # | Branch                       | Type    | Created    | Status | PR | Notes |
|---|-------------------------------|---------|------------|--------|----|-------|
| 0001 | `chore/0001-bootstrap-scaffolding` | chore | 2026-07-18 | merged | #1 | |
| 0001 | `docs/0001-adr-open-items` | docs | 2026-07-18 | merged | #2 | |
| 0001 | `fix/0001-branch-registry-order` | fix | 2026-07-18 | open | pending | |
| 0001 | `feature/0001-ingestion-service` | feature | 2026-07-18 | open | pending | |
| 0002 | `feature/0002-ingestion-gateway` | feature | 2026-07-18 | open | pending | |
| 0003 | `feature/0003-normalization-service` | feature | 2026-07-18 | open | pending | |
| 0004 | `feature/0004-analysis-service` | feature | 2026-07-18 | open | pending | |
| 0005 | `feature/0005-trigger-engine` | feature | 2026-07-18 | open | pending | |
| 0006 | `feature/0006-action-executor` | feature | 2026-07-18 | open | pending | |
| 0007 | `feature/0007-query-gateway-dashboard-api` | feature | 2026-07-18 | open | pending | |
| 0008 | `feature/0008-auth-service` | feature | 2026-07-18 | open | pending | |
| 0009 | `feature/0009-config-admin-service` | feature | 2026-07-18 | open | pending | |
| 0010 | `feature/0010-retention-service` | feature | 2026-07-18 | open | pending | |
| 0011 | `feature/0011-observability` | feature | 2026-07-18 | open | pending | |
| 0012 | `feature/0012-connectors` | feature | 2026-07-18 | open | pending | |
| 0013 | `feature/0013-console-ui` | feature | 2026-07-18 | open | pending | |
| 0002 | `chore/0002-local-dev-launcher` | chore | 2026-07-18 | open | pending | |
| 0014 | `feature/0014-docker-images` | feature | 2026-07-18 | open | pending | |
| 0015 | `feature/0015-ai-analysis-config` | feature | 2026-07-19 | open | pending | |
| 0016 | `feature/0016-agent-scheduler` | feature | 2026-07-19 | open | pending | |
| 0017 | `feature/0017-agent-scheduler-docker-packaging` | feature | 2026-07-19 | open | pending | |
| 0018 | `feature/0018-egress-gateway` | feature | 2026-07-19 | open | pending | |
| 0019 | `feature/0019-egress-proxy-connector-wiring` | feature | 2026-07-19 | open | pending | |
| 0020 | `feature/0020-imap-inbound-connector` | feature | 2026-07-19 | open | pending | |
| 0021 | `feature/0021-smtp-send-action` | feature | 2026-07-19 | open | pending | |
| 0022 | `feature/0022-graph-send-mail-action` | feature | 2026-07-19 | open | pending | |
| 0023 | `feature/0023-entra-token-egress-routing` | feature | 2026-07-19 | open | pending | |
| 0024 | `feature/0024-config-admin-tenant-isolation-tests` | feature | 2026-07-19 | open | pending | |
| 0025 | `feature/0025-query-gateway-tenant-isolation-e2e` | feature | 2026-07-19 | open | pending | |
| 0002 | `fix/0002-agent-rbac-enforcement` | fix | 2026-07-19 | open | pending | |
| 0003 | `fix/0003-egress-allowlist-rbac` | fix | 2026-07-19 | open | pending | |
| 0003 | `chore/0003-update-handler-tenant-mismatch-tests` | chore | 2026-07-19 | open | pending | |
| 0026 | `feature/0026-retention-policy-console-ui` | feature | 2026-07-19 | open | pending | |
| 0027 | `feature/0027-egress-allowlist-console-ui` | feature | 2026-07-19 | open | pending | |
| 0028 | `feature/0028-audit-log-console-ui` | feature | 2026-07-19 | open | pending | |
| 0029 | `feature/0029-normalization-mapping-sync` | feature | 2026-07-19 | open | pending | |
| 0030 | `feature/0030-user-management-role-assignment` | feature | 2026-07-19 | open | pending | |
| 0031 | `feature/0031-last-admin-protection` | feature | 2026-07-19 | open | pending | |
| 0004 | `fix/0004-teams-alert-webhook-payload-shape` | fix | 2026-07-19 | open | pending | |
| 0032 | `feature/0032-retention-sweep-scheduler` | feature | 2026-07-19 | open | pending | |
| 0033 | `feature/0033-cross-source-correlated-triggers` | feature | 2026-07-19 | open | pending | |
| 0034 | `feature/0034-correlated-triggers-console-ui` | feature | 2026-07-19 | open | pending | |
| 0035 | `feature/0035-configurable-webhook-action-body` | feature | 2026-07-19 | open | pending | |
| 0002 | `docs/0002-adr-0016-stale-followups-note` | docs | 2026-07-19 | open | pending | |
| 0036 | `feature/0036-saved-search-queries` | feature | 2026-07-19 | open | pending | |
| 0037 | `feature/0037-trigger-dry-run-test` | feature | 2026-07-19 | open | pending | |
| 0038 | `feature/0038-correlated-trigger-form-more-rows` | feature | 2026-07-19 | merged | #47 | |
| 0039 | `feature/0039-ai-provider-config` | feature | 2026-07-19 | merged | #48 | |
| 0040 | `feature/0040-idempotent-ingestion-dedup` | feature | 2026-07-19 | merged | #49 | |
| 0041 | `feature/0041-imap-since-date-narrowing` | feature | 2026-07-19 | merged | #50 | |
| 0042 | `feature/0042-imap-uid-cursor` | feature | 2026-07-19 | merged | #51 | |
| 0043 | `feature/0043-events-over-time-chart` | feature | 2026-07-19 | merged | #52 | |
| 0044 | `feature/0044-reprocess-unnormalized-records` | feature | 2026-07-19 | merged | #53 | |
| 0045 | `feature/0045-analysis-concurrency` | feature | 2026-07-19 | merged | #54 | |
| 0046 | `feature/0046-reprocess-ui-button` | feature | 2026-07-19 | merged | #55 | |
| 0047 | `feature/0047-record-journey-timing-waterfall` | feature | 2026-07-19 | merged | #56 | |
| 0048 | `feature/0048-sensor-naming-stage1-ui-labels` | feature | 2026-07-19 | open | pending | |
| 0049 | `feature/0049-analysis-service-consumer-liveness-healthcheck` | feature | 2026-07-19 | merged | #59 | |
| 0050 | `feature/0049-sensor-naming-stage2-types-and-routes` | feature | 2026-07-19 | open | pending | branch predates renumbering; kept original branch name, registry # bumped to avoid collision |
| 0005 | `fix/0005-analysis-service-timeout-and-heartbeat-window` | fix | 2026-07-19 | merged | #60 | |
| 0051 | `feature/0051-ui-polish-sensor-picker-and-trigger-form` | feature | 2026-07-19 | merged | #61 | |
| 0006 | `fix/0006-audit-log-real-actor` | fix | 2026-07-20 | open | pending | integrates 6 parallel agent branches (auth-service, config-admin-service, retention-service, ingestion-gateway, ui batch1, ui batch2) into one atomic PR since backend+UI must land together |
| 0052 | `feature/0052-overview-recent-activity` | feature | 2026-07-19 | open | pending | |
| 0053 | `feature/0053-console-ui-oidc-sso-login` | feature | 2026-07-19 | open | pending | |
| 0054 | `feature/0054-tenant-branding-config` | feature | 2026-07-19 | open | pending | |
| 0007 | `fix/0007-rbac-audit-fixes` | fix | 2026-07-19 | open | pending | |
