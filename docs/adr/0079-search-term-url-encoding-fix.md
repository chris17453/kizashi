# 0079. Fix unencoded search-term URL-encoding in sort/pagination links

## Context

A third audit pass found the same class of bug as ADR-0076's `before`-cursor fix, in the same
family of links: every sort-column header and "Load older" href across Users, Sessions,
Triggers, Events, and the global Audit Log spliced the raw `q` search term directly into a
query string (`?q={{ q }}&sort=...`) with no URL-encoding. A search term containing `&` or `#`
would be read as starting a new query parameter or URL fragment, silently corrupting or
truncating the `sort`/`dir`/`before` values that follow it in the link. Notably, on
`recent_audit_log.html`'s own "Load older" link, `before` already had ADR-0076's `|urlencode`
fix applied right next to an un-fixed `q` on the same line — the fix for one parameter was
applied without checking the adjacent one used the same encoding.

## Decision

Every `q={{ q }}` interpolated into an `href` (not a form `value=` attribute, which is already
safe — the browser encodes on submit) now uses Askama's `|urlencode` filter:
`triggers.html`, `events.html`, `users.html`, `sessions.html`, `recent_audit_log.html`.
Same filter, same reasoning, same fix shape as ADR-0076.

## Consequences

- Purely additive markup fix, no behavior change for search terms that don't contain special
  characters (the overwhelming majority).
- A new regression test (`get_users_sort_header_links_percent_encode_a_q_containing_an_ampersand`)
  asserts the actual rendered link text is percent-encoded, not just present — the same
  assertion style ADR-0076 established, since a test that only checks a link exists wouldn't
  have caught this bug in the first place.
- This is the second time this exact class of bug (unencoded dynamic value in an href) has been
  found in this codebase. Worth treating any future `{{ value }}` interpolated directly into an
  `href`'s query string (not a `value=`/`action=` attribute) as needing `|urlencode` by default,
  rather than re-discovering this per-occurrence.
