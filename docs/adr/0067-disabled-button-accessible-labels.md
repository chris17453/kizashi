# 0067. Accessible labels on disabled self-action buttons

## Context

A UI/UX audit found `aria-label` essentially unused sitewide (2 occurrences across all
templates before this change), an enterprise accessibility-compliance gap (WCAG/508). The
concrete instance flagged: the Users page's disabled "Remove" button and the Active Sessions
page's disabled "Revoke" button (both disabled for the caller's own row, to prevent
self-removal/self-revoke) carry only a `title` attribute explaining why. `title` is shown on
mouse hover but is not reliably exposed to screen readers or keyboard-only navigation — a
disabled button with no accessible explanation reads as just "Remove, disabled" with no reason.

## Decision

Both buttons now carry a matching `aria-label` alongside `title`, restating the button's action
and the disabled reason in one string (e.g. `"Remove -- you can't remove yourself"`), so a
screen reader announces the same explanation a sighted mouse user gets from the tooltip.
Deliberately not a sitewide aria-label sweep in one PR — scoped to the two concretely-flagged
disabled-with-title instances found by the audit; a broader accessibility pass (icon-only
controls, pagination arrows, form labeling) is tracked as a follow-up, not silently bundled in.

## Consequences

- No behavior change — purely additive markup, no existing test asserted on the exact button
  HTML so nothing needed updating besides the two new tests added for this change.
- The `aria-label` intentionally duplicates `title`'s wording rather than introducing separate
  copy, so the two can't drift out of sync if one is edited later without the other.
