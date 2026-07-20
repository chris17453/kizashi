# 0052. Password policy enforcement

## Context

The compliance rubric introduced in ADR-0051 identified password policy enforcement as another
uncovered control domain: `hash_password` (auth-service) would happily hash and store any
string, including a one-character password or the literal word "password" — no length,
strength, or blocklist check existed anywhere on the only path that ever sets a password
(`create_user`; there is no self-service password-change endpoint yet).

## Decision

Add `password_policy::validate_password_strength(password, username)`, enforced in `create_user`
before hashing, checking:

- **Minimum length 12** — NIST 800-63B's modern guidance deprioritizes composition rules ("must
  contain a digit and a symbol") in favor of length plus a blocklist, since composition rules
  push users toward predictable substitutions ("Password1!") that don't meaningfully resist
  guessing, while length does.
- **Maximum length 128** — bounds Argon2's hashing cost, a free close on a minor DoS surface (an
  unbounded password makes a single request expensive to hash).
- **Not equal to the username** (case-insensitive).
- **Not in a small blocklist of known-weak values** (~10 entries — not an exhaustive breached-
  password dataset, which is a larger, externally-sourced effort out of scope for v1; just the
  handful of values common enough that allowing them at all would be a compliance-review red
  flag on its own).

A violation returns `400` with a specific reason (`"password must be at least 12 characters"`,
etc.) rather than a generic rejection. This surfaced a real, separate UX gap while wiring it
through: the Console UI's `UsersClientError::Rejected` only ever carried an HTTP status code, not
the backend's actual error message — an admin hitting the new policy would have seen "HTTP 400"
with no indication why. Fixed alongside: `Rejected` now carries the backend's `{"error": "..."}`
body text when present, falling back to a generic message otherwise. The Users page form also
gained a matching `minlength="12"` attribute and inline hint text for immediate client-side
feedback, though the server-side check remains the actual enforcement point.

## Consequences

- Existing local users created before this change with weak passwords are unaffected — the
  policy is enforced only at creation time, not retroactively against stored hashes (a stored
  Argon2 hash can't be reverse-checked against a policy without the plaintext). A future
  "require password reset" flow could close this gap if it becomes a real concern.
- There is still no self-service password-change endpoint at all — a user can never change their
  own password today, only an admin can reset it via `delete` + `create`. This is a bigger,
  separate gap, noted here rather than silently expanded into scope.
- The `UsersClientError::Rejected` shape change (`u16` → `{status, message}`) is scoped to
  `users_client.rs` only; no other client in the Console UI shares this enum, so the blast radius
  is contained to this one file and its test double.
