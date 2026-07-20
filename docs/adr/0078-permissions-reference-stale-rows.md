# 0078. Permissions Reference page had drifted stale

## Context

The Permissions Reference page (ADR-0048) exists specifically so an auditor or new admin can
answer "what can each role do" without reading source code — its own header text says "if this
table and the running system ever disagree, the running system is right; file that as a bug
against this page." A review this session found exactly that disagreement: four areas added in
later features (Login Attempts, Backups, Compliance Report, Security Overview) were never added
to the table's hand-maintained row list, so the page silently under-represented what actually
exists and who can access it.

## Decision

Added the four missing rows, transcribed the same way as every existing row — read directly
from each handler's own role-gate code, not assumed:
- **Login Attempts** — Admin-only (`login_attempts_handler.rs`'s `require_admin_session`).
- **Backups** — Admin-only, platform-wide not tenant-scoped (`backup_status_handler.rs`).
- **Compliance Report** — Admin-only (`compliance_report_handler.rs`).
- **Security Overview** — every role can view; sections aggregating Admin-only data (RBAC
  counts) degrade gracefully to zero for a non-Admin caller rather than showing real numbers,
  confirmed by tracing `security_overview_handler.rs`'s `list_users` error-handling path.

## Consequences

- This table is still hand-maintained, not generated from route/role-gate metadata — the same
  drift can recur the next time a new Admin-gated page ships without a matching row added in
  the same PR. A generated-from-source-of-truth version is a reasonable future improvement, not
  attempted here (would need every handler's role gate to be introspectable in a structured
  way, not just enforced ad hoc per-handler).
- No behavior change — this is a documentation-accuracy fix, not a permissions change; every
  role gate this row addition describes was already enforced exactly as stated, just not
  written down.
