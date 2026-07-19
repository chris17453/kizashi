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
| feature       | `feature/` | 0026         |
| fix           | `fix/`     | 0003         |
| debug         | `debug/`   | 0001        |
| docs          | `docs/`    | 0002         |
| chore         | `chore/`   | 0003         |

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
