# 0051. TOTP-based multi-factor authentication for local login

## Context

An explicit enterprise-compliance rubric run this session (mapped to standard SOC 2/ISO 27001
control domains: access control, audit logging, session management, tenant isolation,
service-to-service auth, secrets handling, transport security, data retention, egress control,
federated identity) scored Kizashi 11/16 "done." The most consequential gap: **multi-factor
authentication**. Every other control on that list assumes "the logged-in user is who they say
they are" is already true — until now, that rested entirely on a password, with SSO/OIDC (ADR-0009)
as the only alternative for tenants with an external IdP. Local login had no second factor at all.

## Decision

Add TOTP-based (RFC 6238, `totp-rs` crate) MFA as an opt-in, self-service feature per local user:

**Schema** (`crates/auth-service/migrations/0007_add_mfa_to_local_users.sql`): `local_users` gains
`mfa_secret TEXT` and `mfa_enabled BOOLEAN NOT NULL DEFAULT false`. A new `mfa_challenges` table
bridges the two-step login flow across separate HTTP round trips: `(id, local_user_id, tenant_id,
challenge_token, created_at, expires_at)`, a Postgres table rather than in-memory since
auth-service (unlike Console UI, ADR-0014) has no single-instance assumption to lean on.

**Enrollment is a three-step, explicitly-confirmed flow** — critical because an unconfirmed
secret must never be able to gate login (a typo during QR scanning could otherwise lock a user
out permanently):
1. `POST /v1/auth/local/mfa/enroll` generates a new secret, stores it as *pending*
   (`mfa_enabled` stays `false`), returns a QR code (rendered server-side via `totp-rs`'s `qr`
   feature) and the raw base32 secret for manual entry.
2. `POST /v1/auth/local/mfa/verify` checks a submitted code against the pending secret; only on
   success does `mfa_enabled` flip to `true`.
3. `POST /v1/auth/local/mfa/disable` requires re-entering the account password (not just an
   established session) — a hijacked but still-logged-in browser tab must not be able to
   silently strip a second factor off the account.

**Login flow**: `local_login`'s existing password check is unchanged, but on success for a user
with `mfa_enabled`, it now returns `{mfa_required: true, challenge_token}` instead of a session
grant. The Console UI stashes `challenge_token` and the typed username in two short-lived
`HttpOnly` cookies (`kizashi_mfa_challenge`, `kizashi_mfa_username` — the same bridging pattern
already used for OIDC's `kizashi_oidc_flow`, ADR-0040) and redirects to `/login/mfa`, a
code-entry page. Submitting the code calls `POST /v1/auth/local/mfa/challenge` — deliberately the
only MFA endpoint with **no** `X-Role`/`X-Tenant-Id`/`X-Username` trust, since at this point in
the flow Console UI has no session or verified identity yet; the challenge_token itself (single-use,
5-minute TTL, consumed on read regardless of outcome to prevent replay/brute-force) is the entire
proof of "this is the same login attempt that just passed the password check."

**Console UI additions**: `GET /security/mfa` (self-service settings page, own account only — no
role gate, matching the "you manage your own second factor" nature of the feature) plus its three
POST actions, and `GET /login/mfa` + `POST /login/mfa` for the login-time challenge. New nav entry
under Security & Compliance.

## Consequences

- MFA is per-user opt-in, not tenant-mandated — an admin cannot currently force every user in a
  tenant onto MFA. A future "require MFA for this tenant" policy would need a new tenant-level
  flag and a login-time check against it; tracked as a natural follow-up, not built here.
- OIDC/SSO logins are unaffected — MFA here only gates the local-username-password path;
  federated-identity MFA (if any) is the external IdP's responsibility, consistent with ADR-0009's
  "Auth Service delegates identity verification to the IdP for OIDC" design.
- The challenge-bridging cookies are a second precedent (after OIDC's) for this pattern; if a
  third multi-step auth flow is ever added, factoring a shared "pending flow" cookie helper
  becomes worth doing — not yet, with only two call sites.
- `totp-rs` (plus its `qr`/`image`/`qrcodegen` transitive dependencies) is a new dependency
  footprint; `cargo deny`/`cargo audit` confirm no new advisories introduced.
