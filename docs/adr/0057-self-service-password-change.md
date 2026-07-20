# 0057. Self-service password change

## Context

ADR-0052 (password policy enforcement) explicitly flagged this gap when it shipped: the only way
to change a local user's password was an admin deleting and recreating the account (`DELETE
/v1/users/:id` + `POST /v1/users`) — there was no path for a user to change their own password
at all. That's a basic expectation for any enterprise-facing product, not an edge case, so it's
closed now rather than left as a permanent "noted, not fixed" line in an old ADR.

## Decision

New `LocalUserRepository::update_password(id, new_password_hash)` — a plain `UPDATE`, no
audit-log row, same reasoning as the three MFA enrollment mutations (`set_pending_mfa_secret`/
`confirm_mfa`/`disable_mfa`): a user changing their own password isn't an admin action on someone
else, unlike `update_role`/`delete`.

`POST /v1/auth/local/password` (`ChangePasswordRequest { current_password, new_password }`)
requires re-entering the current password before accepting a new one — same trust reasoning as
`post_mfa_disable` (ADR-0051): a hijacked but still-logged-in browser tab shouldn't be able to
silently take over an account via a password change alone, session cookie or not. The new
password goes through the exact same `password_policy::validate_password_strength` check
`create_user` uses, so self-service can't be used to bypass the policy admin-created accounts are
held to.

Console UI: `GET`/`POST /security/password`, self-service (not admin-gated, matching
`/security/mfa`'s access bar — every user manages their own credentials). A `confirm_password`
field is a UI-only typo guard; Auth Service never sees or needs it, since the browser has already
confirmed the two fields match before the request is even sent.

## Consequences

- Does not add a "forgot password" / unauthenticated reset flow — this is *change* (requires
  knowing the current password), not *reset* (recovering access without it). A locked-out user
  still needs an admin to intervene (today: delete + recreate; a proper reset-via-email flow
  would need its own design, deferred).
- No rate limiting specific to this endpoint beyond what already exists platform-wide — a
  compromised session could still brute-force `current_password` guesses against this endpoint.
  Worth revisiting if `login_attempts`-style anomaly tracking (ADR-0053) is ever extended to
  cover more than the login path itself.
