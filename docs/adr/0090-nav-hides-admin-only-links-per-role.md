# 0090. Console UI nav hides admin-only links per role

## Context

A sixth UI audit pass, focused on RBAC/enterprise-compliance completeness, found that
`ui/templates/layout.html`'s sidebar nav renders identically for every signed-in user
regardless of role, while 5 of those links point at pages whose handlers already enforce
`Role::Admin`-only access server-side (`/users`, `/security/sessions`,
`/security/login-attempts`, `/security/backups`, `/security/compliance-report`) — each returns a
bare `403 FORBIDDEN` for a `Viewer`/`Operator`. A lower-privileged user saw all five links, and
clicking any of them hit a dead end with no explanation. `/branding` (GET requires only a
session; only its POST requires Admin, already gated internally via its own `can_write` flag)
and `/audit-log` (intentionally viewable by every role, no gate at all) were confirmed correct
as-is and left unconditional.

## Decision

Every one of the Console UI's ~32 page-handler Askama `*Template` structs gained an `is_admin:
bool` field, computed once per handler as `session.role.at_least(common::Role::Admin)`
immediately after session resolution — threaded through every construction site, the same
pattern `show_nav: bool` already established. `layout.html` wraps exactly the 5 admin-only
`<a>` tags in `{% if is_admin %}...{% endif %}`; every other nav link (including `/branding` and
`/audit-log`) stays unconditional. The three pre-auth-only templates (login, MFA login, SSO
login) also carry `is_admin: false`, required because Askama type-checks a template's fields
statically per struct at compile time regardless of which runtime branch executes.

Two pre-existing test files exceeded CLAUDE.md's 500-line limit before this change
(`sensors_handler_test.rs` at 680 lines, `users_handler_test.rs` at 545 lines, both already over
before this PR's additions) — per §0 rule 6 ("split before it's added to, not after"), both were
split by responsibility: GET/list/search/sort/pagination/RBAC-visibility tests stay in the
original file, POST/mutation tests move to a new sibling `*_handler_mutations_test.rs`, wired
via a second `#[path] mod` declaration — the same test-splitting shape CLAUDE.md's own
`_test.rs` convention already implies for oversized modules.

## Consequences

- No backend change — every server-side 403 already existed; this closes the "the UI still
  offers a link the backend will reject" gap, not a new authorization boundary.
- New tests added to 3 representative handlers (`overview_handler_test.rs`,
  `sensors_handler_test.rs`, `users_handler_test.rs`) assert the admin-only links are absent for
  Viewer/Operator sessions and present for Admin sessions; every other affected handler was
  verified to compile and its existing tests to still pass, but doesn't carry a new
  role-visibility-specific assertion of its own — a reasonable follow-up if a future audit wants
  full per-page coverage of this specific behavior.
- `sensors_handler_test.rs`/`users_handler_test.rs` and their new `*_mutations_test.rs` siblings
  are both now under 500 lines each.
