# 0061. Destructive action confirmation

## Context

A UI/UX audit of the Console UI found a real, repo-wide gap: every destructive control (Revoke
API key, Remove user, Revoke session, Remove retention policy, Remove sensor, Disable MFA,
Remove saved search) is a plain `<form method="post">` with a `.btn-danger` submit button — no
`confirm()`, no modal, no two-step flow anywhere. A misclick permanently revokes an API key or
removes a user with zero chance to back out, on 7 different pages. The backend already enforces
auth/RBAC on all of these, but that's a different concern from "did the person at the keyboard
mean to click that" — a one-click, no-confirmation permanent action is a basic UX safety
expectation this platform was missing everywhere at once.

## Decision

One shared, unobtrusive fix rather than per-page changes: `ui/static/confirm-danger.js`, served
via `GET /static/confirm-danger.js` (same `include_str!`-embedded-in-binary pattern as
`charts.js`, ADR-0015 — no static-file-serving middleware, no path traversal surface), included
once in `layout.html` next to the existing chart script. It listens for `submit` events at the
document level and checks `event.submitter` (the actual button that triggered the submission)
for the `.btn-danger` class already applied consistently across every destructive button in this
codebase — if present, it shows `window.confirm("Are you sure you want to <button label>? This
cannot be undone.")` and cancels the submission on "Cancel."

This required zero changes to any of the 7 existing templates or their handlers: every
destructive button already carried `.btn-danger` for styling, so the fix is purely additive and
automatically covers any future `.btn-danger` submit button too, with no per-page opt-in to
remember.

## Consequences

- Client-side only — a request crafted directly against the backend (curl, a script) bypasses
  this exactly as before; this closes an accidental-misclick gap for a human using the browser
  UI, not a security control. The backend's existing auth/RBAC checks remain the actual
  enforcement boundary.
- `window.confirm()` is a blocking native browser dialog, not a styled in-app modal — chosen
  deliberately for zero new CSS/JS framework surface and because it needs no additional design
  work to look consistent across every page it now covers. A custom modal component is a
  reasonable future polish item if the native dialog's appearance becomes a real complaint.
- Disabled destructive buttons (e.g. "you can't remove yourself," "use Log out instead") are
  unaffected — a disabled button never fires a `submit` event, so there's nothing for this
  listener to intercept.
