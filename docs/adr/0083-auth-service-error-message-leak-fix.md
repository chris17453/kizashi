# 0083. Auth Service stops leaking raw backend errors on user create/update failures

## Context

A fourth audit pass found `user_handlers.rs::user_error_response` passed
`LocalUserRepositoryError::Backend(msg)` straight through as the HTTP 500 response body for any
failure that wasn't a duplicate-key conflict — a connection-pool timeout, a SQL constraint
violation, or any other internal error string, verbatim, in a response the Console UI then
rendered directly in `UsersTemplate.error` for an Admin to read. Only the duplicate-key case was
pattern-matched into a clean, actionable message; everything else leaked internal implementation
detail into a client-facing surface.

## Decision

Every non-duplicate-key `Backend` error is now logged in full via `tracing::error!` (so the real
detail is still available to whoever's watching the logs) and replaced with a generic
`"an internal error occurred; check server logs for details"` message before it reaches the
client — the same log-then-generalize pattern already used elsewhere in this service
(`local_login_handler.rs`, `branding_handler.rs`). The duplicate-key case is unchanged: it's a
real, expected, actionable outcome ("pick a different username"), not an internal detail.

## Consequences

- No behavior change to the duplicate-key path or any success path.
- A regression test (`create_user_backend_failure_does_not_leak_the_raw_error_to_the_client`)
  asserts the response body never contains the repository's raw error string, using the
  existing `FailingLocalUserRepository` test double.
- This was scoped to `user_handlers.rs` since that's where the audit found the concrete
  instance — a broader sweep for the same pattern elsewhere (other services' error-response
  helpers) wasn't performed and is a reasonable follow-up, not assumed already covered.
