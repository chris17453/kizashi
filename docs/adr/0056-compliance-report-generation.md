# 0056. Compliance report generation

## Context

The last domain in the compliance rubric introduced in ADR-0051: today the only export is the
audit-log CSV (ADR-0049) — a raw, flat dump of individual change rows (`changed_at, service,
entity_type, change_type, actor`). It answers "show me the log," not the question a SOC2/
ISO27001 auditor or a customer's compliance team actually opens with: "in one document, tell me
what controls are in place right now." Building that from scratch would be redundant — Security
Overview (ADR-0047, `ui/src/security_overview_handler.rs`) already aggregates RBAC distribution,
retention coverage, egress allowlist size, and recent activity from the same clients this report
needs; the compliance rubric's later domains (MFA, login-attempt monitoring, backup/DR,
ADR-0051/0053/0055) each shipped their own client but were never folded into a dashboard.

## Decision

**No new reporting engine.** `GET /security/compliance-report` (`ui/src/
compliance_report_handler.rs`) assembles a single browser-printable HTML snapshot by calling the
same clients Security Overview already uses (`users_client`, `retention_policies_client`,
`egress_allowlist_client`, the three `AuditLogClient`s) plus the two under-utilized ones
(`login_attempts_client`, `backup_status_client`) — no new data-gathering, just one document
instead of five separate pages. `Admin`-only, matching the access bar of the admin-gated data it
aggregates (login attempts, backup status).

Two real, small backend gaps needed closing rather than papering over with hardcoded UI text:

- **MFA adoption was not queryable.** `UiUser` (`ui/src/users_client.rs`) never carried
  `mfa_enabled`, even though `LocalUser` (auth-service) has had the column since ADR-0051 — the
  field just was never added to the Console UI's response shape. Added as `#[serde(default)]` so
  it degrades to `false` rather than breaking deserialization against an older backend.
- **The password policy's actual parameters had no query path.** Rather than hardcode "min 12
  chars" as UI copy that could silently drift from `password_policy::validate_password_strength`,
  added `password_policy::summary()` (returns `PasswordPolicySummary { min_length, max_length,
  blocklist_size }`) and a new `GET /v1/auth/local/password-policy` endpoint — not tenant-scoped
  or sensitive (it's the rule, not anyone's data), so no `X-Tenant-Id`/`X-Role` check.

Rendered as printable HTML (browser "Print to PDF") rather than generating a PDF server-side —
zero new dependencies, and every browser already does this well.

## Consequences

- This is a live snapshot generated on request, not a scheduled/archived historical report — an
  auditor gets "state as of right now," not "state as of last quarter's audit." Scheduled
  report archival is a real follow-up but a separate, larger piece of work (needs its own
  storage/retention decision), not silently folded into this change.
- No signed/timestamped audit-grade PDF pipeline — the printable-HTML approach is genuinely
  good enough for "hand this to an auditor" but would not satisfy a requirement for
  cryptographically-verifiable report provenance, if that ever becomes a real ask.
- Still does not cover ingested `RawRecord`/`Event` content (same boundary as ADR-0054's data
  subject rights) — the report's intro text says so explicitly rather than silently omitting it.
- `recent_backup_failure_count` counts failures among the last 20 recorded runs (a fixed lookback,
  same "good enough for at-a-glance, not an exact historical count" tradeoff Security Overview
  already accepts for its own recent-activity tile).
