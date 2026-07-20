# ADR-0099: Sessions bulk-revoke

- **Status:** accepted
- **Date:** 2026-07-20

## Context

A ninth Console UI audit pass compared Active Sessions against API Keys, both admin-only pages
with per-row destructive actions on the same kind of resource (a live credential). API Keys has
had bulk-revoke since ADR-0065, and Sensors/Users/Retention Policies picked it up in
ADR-0095/ADR-0096, but Sessions was left with only a per-row "Revoke" button — an inconsistency
with no functional justification: revoking five stale sessions after an incident response one at
a time is the exact kind of friction the bulk-action pattern exists to remove.

## Decision

Add `POST /security/sessions/bulk-revoke`, following the established shape exactly: checkboxes
(`name="ids"`, `form="bulk-revoke-form"`) + an empty `<form id="bulk-revoke-form">` + a
`parse_ids` helper parsing the raw POST body via `serde_urlencoded` (since `axum::extract::Form`
can't collect repeated same-named fields into a `Vec`) + a loop over the existing tenant-scoped
delete logic already in `post_revoke_session`. One difference from the `Uuid`-keyed precedents
(API Keys/Users/Sensors/Retention Policies): session ids are opaque `String`s issued by
`SessionStore`, not `Uuid`s, so `parse_ids` here returns `Vec<String>` with no further parsing.
The caller's own current session is excluded from the checkbox column, matching the existing
per-row self-protection (a disabled "Revoke" button telling the caller to use Log out instead).

## Consequences

- Every list page in the Console UI with a destructive per-row action now has bulk-select parity
  — closes the last page-level inconsistency the bulk-action pattern's rollout (ADR-0065 →
  ADR-0095 → ADR-0096) left open.
- The tenant-membership check runs per-id inside the bulk handler (same guarantee as the
  single-revoke handler), so a malicious or malformed `ids` list can't be used to revoke another
  tenant's sessions.
