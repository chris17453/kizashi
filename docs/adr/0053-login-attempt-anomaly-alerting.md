# 0053. Login-attempt anomaly alerting

## Context

The compliance rubric introduced in ADR-0051 also names login/auth anomaly alerting as an
uncovered domain: `auth_audit_log` records *changes* to entities (user created, role changed,
MFA enrolled), but a failed login attempt has no entity to attach an audit row to and was never
recorded anywhere. An admin investigating a suspected brute-force attempt, or a compliance
reviewer asking "can you show me every failed login for this tenant in the last 24 hours," had
no answer.

## Decision

Add a dedicated `login_attempts` table and repository, separate from `auth_audit_log`, since the
two have different shapes and different write volume (every login attempt vs. every config
change):

- Columns: `id`, `tenant_id` (nullable — null when the workspace name in the request doesn't
  resolve to a real tenant, so there is no tenant to scope the row to), `username`, `success`,
  `reason`, `attempted_at`.
- Append-only at the database level via a `BEFORE UPDATE OR DELETE` trigger, matching the
  established `auth_audit_log` pattern rather than inventing a new immutability mechanism.
- Recorded at every branch point in `local_login` and `mfa/challenge`: `unknown_workspace`,
  `unknown_username`, `wrong_password`, `password_ok_mfa_pending`, `mfa_code_invalid`,
  `mfa_success`, `success`.
- Recording is best-effort and non-blocking: a `record_attempt` failure is logged and swallowed,
  never surfaced to the caller. A telemetry write must never be able to break the actual login
  path — tested explicitly with a `FailingLoginAttemptRepository` double.
- `GET /v1/auth/local/login-attempts` is Admin-only and tenant-scoped, landing in the
  `X-Internal-Secret`-protected router group like the rest of the internal-trust endpoints. This
  is tenant-wide security telemetry, not a self-service page — a non-admin has no business
  seeing every login attempt against every account in the tenant.
- Console UI: `/security/login-attempts`, Admin-gated the same way as Active Sessions, listed in
  the Security & Compliance nav group.

## Consequences

- `login_attempts` is unbounded growth with no retention policy yet — the same open gap that
  applies to `auth_audit_log` today. A future retention-service sweep could cover both tables
  together; not scoped here.
- No alerting/notification exists yet — this ships the *visibility* half (an admin can look and
  see a pattern) but not proactive alerting (e.g. paging on N failures in M minutes for one
  account). Noted as a follow-up, not silently expanded into this change.
- `login_attempts` intentionally does not reuse `auth_audit_log`'s table or repository — the two
  have different access patterns (every attempt vs. every mutation) and different consumers
  (security telemetry vs. change history), so keeping them separate avoids overloading one
  table's meaning.
