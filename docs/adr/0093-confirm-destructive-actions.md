# 0093. Confirmation prompt on destructive actions

## Context

The sixth UI audit pass found zero uses of `confirm(` anywhere in the Console UI's templates:
Delete User, Delete Sensor, Delete Retention Policy, and Revoke/bulk-revoke API Key all submit
immediately on click with no "are you sure" step. For an enterprise console this is a real
safety gap — one misclick permanently removes a user or a retention policy, with the only
recovery path being whatever the destination service's own undo story is (usually none, since
these are hard deletes/revokes, not soft-disables).

## Decision

Each destructive `<form>` gained a plain `onsubmit="return confirm('...');"` attribute — the
smallest possible amount of JS, consistent with this codebase's existing no-JS-by-default stance
(ADR-0014): it's inline, has no dependency, and degrades safely (a browser with JS disabled just
submits without the extra confirmation step, same as today, rather than breaking). This mirrors
`analysis_config.html`'s existing inline `onchange` handler — progressive-enhancement JS was
already an accepted pattern here, just never applied to destructive actions.

Confirmation messages are deliberately generic ("Remove this sensor? This cannot be undone.")
rather than interpolating the entity's name/label into the JS string: Askama's `{{ }}`
HTML-escapes but does not JS-escape, so embedding an operator-controlled string (a sensor name,
a user's username) directly inside an `onsubmit` attribute's JS string risks a broken/injected
confirm() call if that string contains a quote. A generic message avoids the escaping pitfall
entirely rather than adding a JS-string-escaping helper for a cosmetic improvement.

## Consequences

- No backend change — purely a client-side safety net. The server-side write path is unaffected
  and remains the actual authority (a confirm() dialog can always be bypassed by a direct POST,
  which is fine — this closes a UX/misclick gap, not an authorization gap).
- Bulk-revoke's confirmation lives on the `<form id="bulk-revoke-form">` element itself (its
  submit button lives elsewhere in the DOM, referenced via `form="bulk-revoke-form"`) — this
  works because a form's `onsubmit` fires regardless of where the triggering button is located,
  confirmed live against the real `watkinslabs` tenant.
